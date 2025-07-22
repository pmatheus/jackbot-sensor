use crate::{
    balance::{AssetBalance, Balance},
    error::{UnindexedClientError, UnindexedOrderError, UnindexedApiError, ConnectivityError},
    order::{
        id::{ClientOrderId, OrderId},
        request::{OrderRequestCancel, OrderRequestOpen},
        state::{Cancelled, Open},
        Order, OrderKey, OrderKind, TimeInForce,
    },
    trade::Trade,
};
use chrono::{DateTime, TimeZone, Utc};
use hmac::{Hmac, Mac};
use jackbot_instrument::{
    asset::{name::AssetNameExchange, QuoteAsset},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
    Side,
};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use sha2::Sha256;
use std::collections::HashMap;
use std::str::FromStr;
use tracing::error;

type HmacSha256 = Hmac<Sha256>;

const BINANCE_API_BASE: &str = "https://api.binance.com";
const RECV_WINDOW: u64 = 5000; // 5 seconds

#[derive(Clone)]
pub struct BinanceRestConfig {
    pub api_key: String,
    pub api_secret: String,
    pub testnet: bool,
}

#[derive(Clone)]
pub struct BinanceRestClient {
    config: BinanceRestConfig,
    client: Client,
    base_url: String,
}

impl BinanceRestClient {
    pub fn new(config: BinanceRestConfig) -> Self {
        let base_url = if config.testnet {
            "https://testnet.binance.vision".to_string()
        } else {
            BINANCE_API_BASE.to_string()
        };

        Self {
            config,
            client: Client::new(),
            base_url,
        }
    }

    async fn sign_request(&self, params: &mut HashMap<String, String>) {
        let timestamp = Utc::now().timestamp_millis();
        params.insert("timestamp".to_string(), timestamp.to_string());
        params.insert("recvWindow".to_string(), RECV_WINDOW.to_string());

        // Create query string
        let mut query_parts: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        query_parts.sort(); // Ensure consistent ordering
        let query_string = query_parts.join("&");

        // Sign the query string
        let mut mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(query_string.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        params.insert("signature".to_string(), signature);
    }

    pub async fn fetch_balances(
        &self,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        let mut params = HashMap::new();
        self.sign_request(&mut params).await;

        let url = format!("{}/api/v3/account", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("X-MBX-APIKEY", &self.config.api_key)
            .query(&params)
            .send()
            .await
            .map_err(|e| UnindexedClientError::Connectivity(ConnectivityError::Socket(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Binance API error: {} - {}", status, error_text);
            return Err(UnindexedClientError::Api(UnindexedApiError::OrderRejected(
                format!("API error: {} - {}", status, error_text)
            )));
        }

        let account: AccountResponse = response
            .json()
            .await
            .map_err(|e| UnindexedClientError::AccountSnapshot(e.to_string()))?;

        let balances = account
            .balances
            .into_iter()
            .filter(|b| {
                let free = Decimal::from_str(&b.free).unwrap_or(Decimal::ZERO);
                let locked = Decimal::from_str(&b.locked).unwrap_or(Decimal::ZERO);
                free > Decimal::ZERO || locked > Decimal::ZERO
            })
            .map(|b| {
                let free = Decimal::from_str(&b.free).unwrap_or(Decimal::ZERO);
                let locked = Decimal::from_str(&b.locked).unwrap_or(Decimal::ZERO);
                AssetBalance {
                    asset: AssetNameExchange::new(b.asset),
                    balance: Balance {
                        total: free + locked,
                        free,
                    },
                    time_exchange: Utc::now(),
                }
            })
            .collect();

        Ok(balances)
    }

    pub async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError> {
        let mut params = HashMap::new();
        self.sign_request(&mut params).await;

        let url = format!("{}/api/v3/openOrders", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("X-MBX-APIKEY", &self.config.api_key)
            .query(&params)
            .send()
            .await
            .map_err(|e| UnindexedClientError::Connectivity(ConnectivityError::Socket(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(UnindexedClientError::Api(UnindexedApiError::OrderRejected(
                format!("API error: {} - {}", status, error_text)
            )));
        }

        let orders: Vec<OrderResponse> = response
            .json()
            .await
            .map_err(|e| UnindexedClientError::AccountSnapshot(e.to_string()))?;

        let parsed_orders = orders
            .into_iter()
            .filter_map(|o| self.parse_order_response(o))
            .collect();

        Ok(parsed_orders)
    }

    pub async fn place_order(
        &self,
        request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Result<Order<ExchangeId, InstrumentNameExchange, Open>, UnindexedOrderError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), request.key.instrument.as_str().to_string());
        params.insert(
            "side".to_string(),
            match request.state.side {
                Side::Buy => "BUY",
                Side::Sell => "SELL",
            }
            .to_string(),
        );
        params.insert("quantity".to_string(), request.state.quantity.to_string());

        // Map order type and parameters
        match request.state.kind {
            OrderKind::Market => {
                params.insert("type".to_string(), "MARKET".to_string());
            }
            OrderKind::Limit => {
                params.insert("type".to_string(), "LIMIT".to_string());
                params.insert("price".to_string(), request.state.price.to_string());
                
                // Map time in force
                match request.state.time_in_force {
                    TimeInForce::GoodUntilCancelled { post_only } => {
                        params.insert("timeInForce".to_string(), 
                            if post_only { "GTX" } else { "GTC" }.to_string()
                        );
                    }
                    TimeInForce::ImmediateOrCancel => {
                        params.insert("timeInForce".to_string(), "IOC".to_string());
                    }
                    TimeInForce::FillOrKill => {
                        params.insert("timeInForce".to_string(), "FOK".to_string());
                    }
                    TimeInForce::GoodUntilEndOfDay => {
                        params.insert("timeInForce".to_string(), "GTC".to_string()); // Default to GTC for unsupported TIF
                    }
                }
            }
            OrderKind::Stop => {
                params.insert("type".to_string(), "STOP_LOSS".to_string());
                params.insert("stopPrice".to_string(), request.state.price.to_string());
            }
            OrderKind::StopLimit => {
                params.insert("type".to_string(), "STOP_LOSS_LIMIT".to_string());
                params.insert("price".to_string(), request.state.price.to_string());
                params.insert("stopPrice".to_string(), request.state.price.to_string());
            }
            OrderKind::Jackpot | OrderKind::Prophetic | OrderKind::EventTriggered => {
                // Sensor-specific order types - default to market order for now
                params.insert("type".to_string(), "MARKET".to_string());
            }
        }

        // Add client order ID if provided
        if !request.key.cid.is_unknown() {
            params.insert("newClientOrderId".to_string(), request.key.cid.to_string());
        }

        self.sign_request(&mut params).await;

        let url = format!("{}/api/v3/order", self.base_url);
        let response = self
            .client
            .post(&url)
            .header("X-MBX-APIKEY", &self.config.api_key)
            .form(&params)
            .send()
            .await
            .map_err(|e| UnindexedOrderError::Connectivity(ConnectivityError::Socket(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Failed to place order: {} - {}", status, error_text);
            return Err(UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(
                format!("Failed to place order: {} - {}", status, error_text)
            )));
        }

        let order_resp: OrderResponse = response
            .json()
            .await
            .map_err(|e| UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(e.to_string())))?;

        self.parse_order_response(order_resp)
            .ok_or_else(|| UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected("Failed to parse order response".to_string())))
    }

    pub async fn cancel_order(
        &self,
        request: OrderRequestCancel<ExchangeId, InstrumentNameExchange>,
    ) -> Result<Cancelled, UnindexedOrderError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), request.key.instrument.as_str().to_string());

        // Use order ID if provided, otherwise use client order ID
        if let Some(order_id) = &request.state.id {
            params.insert("orderId".to_string(), order_id.to_string());
        } else if !request.key.cid.is_unknown() {
            params.insert("origClientOrderId".to_string(), request.key.cid.to_string());
        } else {
            return Err(UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(
                "Either order ID or client order ID must be provided".to_string(),
            )));
        }

        self.sign_request(&mut params).await;

        let url = format!("{}/api/v3/order", self.base_url);
        let response = self
            .client
            .delete(&url)
            .header("X-MBX-APIKEY", &self.config.api_key)
            .query(&params)
            .send()
            .await
            .map_err(|e| UnindexedOrderError::Connectivity(ConnectivityError::Socket(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(
                format!("Failed to cancel order: {} - {}", status, error_text)
            )));
        }

        let cancel_resp: CancelResponse = response
            .json()
            .await
            .map_err(|e| UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(e.to_string())))?;

        Ok(Cancelled {
            id: OrderId(cancel_resp.order_id.to_string().into()),
            time_exchange: Utc::now(),
        })
    }

    pub async fn fetch_trades(
        &self,
        time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        let mut params = HashMap::new();
        params.insert("startTime".to_string(), time_since.timestamp_millis().to_string());
        self.sign_request(&mut params).await;

        let url = format!("{}/api/v3/myTrades", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("X-MBX-APIKEY", &self.config.api_key)
            .query(&params)
            .send()
            .await
            .map_err(|e| UnindexedClientError::Connectivity(ConnectivityError::Socket(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(UnindexedClientError::Api(UnindexedApiError::OrderRejected(
                format!("API error: {} - {}", status, error_text)
            )));
        }

        let trades: Vec<TradeResponse> = response
            .json()
            .await
            .map_err(|e| UnindexedClientError::AccountSnapshot(e.to_string()))?;

        let parsed_trades = trades
            .into_iter()
            .filter_map(|t| {
                let price = Decimal::from_str(&t.price).ok()?;
                let quantity = Decimal::from_str(&t.qty).ok()?;
                let fee = Decimal::from_str(&t.commission).ok()?;
                let time = Utc.timestamp_millis_opt(t.time).single()?;

                Some(Trade {
                    id: crate::trade::TradeId::new(t.id.to_string()),
                    order_id: crate::order::id::OrderId::default(), // Binance doesn't provide order ID in trades API
                    instrument: InstrumentNameExchange::new(t.symbol),
                    strategy: crate::order::id::StrategyId::unknown(),
                    time_exchange: time,
                    side: if t.is_buyer { Side::Buy } else { Side::Sell },
                    price,
                    quantity,
                    fees: crate::trade::AssetFees {
                        asset: QuoteAsset,
                        fees: fee,
                    },
                })
            })
            .collect();

        Ok(parsed_trades)
    }

    fn parse_order_response(
        &self,
        resp: OrderResponse,
    ) -> Option<Order<ExchangeId, InstrumentNameExchange, Open>> {
        let price = Decimal::from_str(&resp.price).ok()?;
        let quantity = Decimal::from_str(&resp.orig_qty).ok()?;
        let filled = Decimal::from_str(&resp.executed_qty).ok()?;
        let time = Utc.timestamp_millis_opt(resp.time).single()?;

        let side = match resp.side.as_str() {
            "BUY" => Side::Buy,
            "SELL" => Side::Sell,
            _ => return None,
        };

        let kind = match resp.order_type.as_str() {
            "MARKET" => OrderKind::Market,
            "LIMIT" => OrderKind::Limit,
            _ => return None,
        };

        let time_in_force = match resp.time_in_force.as_str() {
            "GTC" => TimeInForce::GoodUntilCancelled { post_only: false },
            "GTX" => TimeInForce::GoodUntilCancelled { post_only: true },
            "IOC" => TimeInForce::ImmediateOrCancel,
            "FOK" => TimeInForce::FillOrKill,
            _ => TimeInForce::GoodUntilCancelled { post_only: false },
        };

        Some(Order {
            key: OrderKey {
                exchange: ExchangeId::BinanceSpot,
                instrument: InstrumentNameExchange::new(resp.symbol),
                strategy: crate::order::id::StrategyId::unknown(),
                cid: if resp.client_order_id.is_empty() {
                    ClientOrderId::default()
                } else {
                    ClientOrderId::new(&resp.client_order_id)
                },
            },
            side,
            price,
            quantity,
            kind,
            time_in_force,
            state: Open {
                id: OrderId(resp.order_id.to_string().into()),
                time_exchange: time,
                filled_quantity: filled,
            },
        })
    }
}

// Response types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountResponse {
    balances: Vec<BalanceResponse>,
    update_time: i64,
}

#[derive(Debug, Deserialize)]
struct BalanceResponse {
    asset: String,
    free: String,
    locked: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderResponse {
    symbol: String,
    order_id: i64,
    client_order_id: String,
    price: String,
    orig_qty: String,
    executed_qty: String,
    status: String,
    time_in_force: String,
    #[serde(rename = "type")]
    order_type: String,
    side: String,
    time: i64,
    update_time: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelResponse {
    symbol: String,
    order_id: i64,
    orig_client_order_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TradeResponse {
    id: i64,
    symbol: String,
    price: String,
    qty: String,
    commission: String,
    commission_asset: String,
    time: i64,
    is_buyer: bool,
    is_maker: bool,
}