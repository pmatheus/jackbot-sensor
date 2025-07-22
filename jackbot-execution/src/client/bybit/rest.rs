use crate::{
    balance::{AssetBalance, Balance},
    error::{UnindexedClientError, UnindexedOrderError, UnindexedApiError, ConnectivityError},
    order::{
        id::{ClientOrderId, OrderId},
        request::{OrderRequestCancel, OrderRequestOpen},
        state::{Cancelled, Open},
        Order, OrderKey, OrderKind, TimeInForce,
    },
    trade::{Trade, TradeId, AssetFees},
};
use chrono::{DateTime, TimeZone, Utc};
use hmac::{Hmac, Mac};
use jackbot_instrument::{
    asset::{name::AssetNameExchange, QuoteAsset},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
    Side,
};
use reqwest::{Client, Method};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::str::FromStr;
use tracing::error;

type HmacSha256 = Hmac<Sha256>;

const BYBIT_API_BASE: &str = "https://api.bybit.com";
const BYBIT_TESTNET_BASE: &str = "https://api-testnet.bybit.com";

#[derive(Clone)]
pub struct BybitRestConfig {
    pub api_key: String,
    pub api_secret: String,
    pub testnet: bool,
}

#[derive(Clone)]
pub struct BybitRestClient {
    config: BybitRestConfig,
    client: Client,
    base_url: String,
}

impl BybitRestClient {
    pub fn new(config: BybitRestConfig) -> Self {
        let base_url = if config.testnet {
            BYBIT_TESTNET_BASE.to_string()
        } else {
            BYBIT_API_BASE.to_string()
        };

        Self {
            config,
            client: Client::new(),
            base_url,
        }
    }

    fn sign_request(&self, timestamp: u64, params: &str) -> String {
        let sign_str = format!("{}{}{}{}", timestamp, &self.config.api_key, "5000", params);
        
        let mut mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(sign_str.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    async fn send_signed_request<T: for<'de> Deserialize<'de>>(
        &self,
        method: Method,
        endpoint: &str,
        params: Option<HashMap<String, String>>,
    ) -> Result<T, UnindexedClientError> {
        let timestamp = Utc::now().timestamp_millis() as u64;
        let recv_window = "5000";
        
        let mut query_params = params.unwrap_or_default();
        
        // Build param string for signing
        let param_str = if method == Method::GET || method == Method::DELETE {
            let mut param_vec: Vec<(String, String)> = query_params.clone().into_iter().collect();
            param_vec.sort_by(|a, b| a.0.cmp(&b.0));
            param_vec.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<String>>()
                .join("&")
        } else {
            serde_json::to_string(&query_params)
                .map_err(|e| UnindexedClientError::AccountSnapshot(e.to_string()))?
        };

        let sign = self.sign_request(timestamp, &param_str);
        
        let url = format!("{}{}", self.base_url, endpoint);
        let mut request = self.client
            .request(method.clone(), &url)
            .header("X-BAPI-API-KEY", &self.config.api_key)
            .header("X-BAPI-TIMESTAMP", timestamp.to_string())
            .header("X-BAPI-RECV-WINDOW", recv_window)
            .header("X-BAPI-SIGN", sign);

        if method == Method::GET || method == Method::DELETE {
            request = request.query(&query_params);
        } else {
            request = request
                .header("Content-Type", "application/json")
                .json(&query_params);
        }

        let response = request
            .send()
            .await
            .map_err(|e| UnindexedClientError::Connectivity(ConnectivityError::Socket(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Bybit API error: {} - {}", status, error_text);
            return Err(UnindexedClientError::Api(UnindexedApiError::OrderRejected(
                format!("API error: {} - {}", status, error_text)
            )));
        }

        let api_response: BybitResponse<T> = response
            .json()
            .await
            .map_err(|e| UnindexedClientError::AccountSnapshot(e.to_string()))?;

        if api_response.ret_code != 0 {
            return Err(UnindexedClientError::Api(UnindexedApiError::OrderRejected(
                format!("Bybit error {}: {}", api_response.ret_code, api_response.ret_msg)
            )));
        }

        api_response.result
            .ok_or_else(|| UnindexedClientError::AccountSnapshot("No data in response".to_string()))
    }

    pub async fn fetch_balances(
        &self,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        let mut params = HashMap::new();
        params.insert("accountType".to_string(), "UNIFIED".to_string());

        let response: WalletBalanceResponse = self
            .send_signed_request(Method::GET, "/v5/account/wallet-balance", Some(params))
            .await?;

        let mut balances = Vec::new();
        for account in response.list {
            for coin in account.coin {
                let total = Decimal::from_str(&coin.wallet_balance).unwrap_or(Decimal::ZERO);
                let locked = Decimal::from_str(&coin.locked).unwrap_or(Decimal::ZERO);
                let free = total - locked;
                
                if total > Decimal::ZERO {
                    balances.push(AssetBalance {
                        asset: AssetNameExchange::new(coin.coin),
                        balance: Balance { total, free },
                        time_exchange: Utc::now(),
                    });
                }
            }
        }

        Ok(balances)
    }

    pub async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError> {
        let mut params = HashMap::new();
        params.insert("category".to_string(), "spot".to_string());

        let response: OrderListResponse = self
            .send_signed_request(Method::GET, "/v5/order/realtime", Some(params))
            .await?;

        let parsed_orders = response
            .list
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
        params.insert("category".to_string(), "spot".to_string());
        params.insert("symbol".to_string(), request.key.instrument.as_str().to_string());
        params.insert(
            "side".to_string(),
            match request.state.side {
                Side::Buy => "Buy",
                Side::Sell => "Sell",
            }.to_string(),
        );
        params.insert("qty".to_string(), request.state.quantity.to_string());

        // Map order type
        match request.state.kind {
            OrderKind::Market => {
                params.insert("orderType".to_string(), "Market".to_string());
            }
            OrderKind::Limit => {
                params.insert("orderType".to_string(), "Limit".to_string());
                params.insert("price".to_string(), request.state.price.to_string());
                
                // Map time in force
                match request.state.time_in_force {
                    TimeInForce::GoodUntilCancelled { post_only } => {
                        if post_only {
                            params.insert("timeInForce".to_string(), "PostOnly".to_string());
                        } else {
                            params.insert("timeInForce".to_string(), "GTC".to_string());
                        }
                    }
                    TimeInForce::ImmediateOrCancel => {
                        params.insert("timeInForce".to_string(), "IOC".to_string());
                    }
                    TimeInForce::FillOrKill => {
                        params.insert("timeInForce".to_string(), "FOK".to_string());
                    }
                    TimeInForce::GoodUntilEndOfDay => {
                        params.insert("timeInForce".to_string(), "GTC".to_string());
                    }
                }
            }
            OrderKind::Stop => {
                params.insert("orderType".to_string(), "Market".to_string());
                // Stop orders would need additional parameters
            }
            OrderKind::StopLimit => {
                params.insert("orderType".to_string(), "Limit".to_string());
                params.insert("price".to_string(), request.state.price.to_string());
            }
            OrderKind::Jackpot | OrderKind::Prophetic | OrderKind::EventTriggered => {
                params.insert("orderType".to_string(), "Market".to_string());
            }
        }

        // Add client order ID if provided
        if !request.key.cid.is_unknown() {
            params.insert("orderLinkId".to_string(), request.key.cid.to_string());
        }

        let response: PlaceOrderResponse = self
            .send_signed_request(Method::POST, "/v5/order/create", Some(params))
            .await
            .map_err(|e| match e {
                UnindexedClientError::Connectivity(ce) => UnindexedOrderError::Connectivity(ce),
                UnindexedClientError::Api(ae) => UnindexedOrderError::Rejected(ae),
                _ => UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(e.to_string())),
            })?;

        let order_info = response;
        let time = Utc::now();

        Ok(Order {
            key: OrderKey {
                exchange: ExchangeId::BybitSpot,
                instrument: request.key.instrument,
                strategy: request.key.strategy,
                cid: if order_info.order_link_id.is_empty() {
                    request.key.cid
                } else {
                    ClientOrderId::new(&order_info.order_link_id)
                },
            },
            side: request.state.side,
            price: request.state.price,
            quantity: request.state.quantity,
            kind: request.state.kind,
            time_in_force: request.state.time_in_force,
            state: Open {
                id: OrderId::new(&order_info.order_id),
                time_exchange: time,
                filled_quantity: Decimal::ZERO,
            },
        })
    }

    pub async fn cancel_order(
        &self,
        request: OrderRequestCancel<ExchangeId, InstrumentNameExchange>,
    ) -> Result<Cancelled, UnindexedOrderError> {
        let mut params = HashMap::new();
        params.insert("category".to_string(), "spot".to_string());
        params.insert("symbol".to_string(), request.key.instrument.as_str().to_string());

        // Use order ID if provided, otherwise use client order ID
        if let Some(order_id) = &request.state.id {
            params.insert("orderId".to_string(), order_id.to_string());
        } else if !request.key.cid.is_unknown() {
            params.insert("orderLinkId".to_string(), request.key.cid.to_string());
        } else {
            return Err(UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(
                "Either order ID or client order ID must be provided".to_string(),
            )));
        }

        let _response: CancelOrderResponse = self
            .send_signed_request(Method::POST, "/v5/order/cancel", Some(params))
            .await
            .map_err(|e| match e {
                UnindexedClientError::Connectivity(ce) => UnindexedOrderError::Connectivity(ce),
                UnindexedClientError::Api(ae) => UnindexedOrderError::Rejected(ae),
                _ => UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(e.to_string())),
            })?;

        Ok(Cancelled {
            id: request.state.id.unwrap_or_else(|| OrderId::default()),
            time_exchange: Utc::now(),
        })
    }

    pub async fn fetch_trades(
        &self,
        time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        let mut params = HashMap::new();
        params.insert("category".to_string(), "spot".to_string());
        params.insert("startTime".to_string(), time_since.timestamp_millis().to_string());

        let response: TradeHistoryResponse = self
            .send_signed_request(Method::GET, "/v5/execution/list", Some(params))
            .await?;

        let parsed_trades = response
            .list
            .into_iter()
            .filter_map(|t| {
                let price = Decimal::from_str(&t.exec_price).ok()?;
                let quantity = Decimal::from_str(&t.exec_qty).ok()?;
                let fee = Decimal::from_str(&t.exec_fee).ok()?;
                let time = Utc.timestamp_millis_opt(t.exec_time.parse::<i64>().ok()?).single()?;

                Some(Trade {
                    id: TradeId::new(t.exec_id),
                    order_id: OrderId::new(&t.order_id),
                    instrument: InstrumentNameExchange::new(t.symbol),
                    strategy: crate::order::id::StrategyId::unknown(),
                    time_exchange: time,
                    side: match t.side.as_str() {
                        "Buy" => Side::Buy,
                        "Sell" => Side::Sell,
                        _ => return None,
                    },
                    price,
                    quantity,
                    fees: AssetFees {
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
        resp: OrderInfo,
    ) -> Option<Order<ExchangeId, InstrumentNameExchange, Open>> {
        let price = Decimal::from_str(&resp.price).ok()?;
        let quantity = Decimal::from_str(&resp.qty).ok()?;
        let filled = Decimal::from_str(&resp.cum_exec_qty).ok()?;
        let time = Utc.timestamp_millis_opt(resp.created_time.parse::<i64>().ok()?).single()?;

        let side = match resp.side.as_str() {
            "Buy" => Side::Buy,
            "Sell" => Side::Sell,
            _ => return None,
        };

        let kind = match resp.order_type.as_str() {
            "Market" => OrderKind::Market,
            "Limit" => OrderKind::Limit,
            _ => return None,
        };

        let time_in_force = match resp.time_in_force.as_str() {
            "GTC" => TimeInForce::GoodUntilCancelled { post_only: false },
            "PostOnly" => TimeInForce::GoodUntilCancelled { post_only: true },
            "IOC" => TimeInForce::ImmediateOrCancel,
            "FOK" => TimeInForce::FillOrKill,
            _ => TimeInForce::GoodUntilCancelled { post_only: false },
        };

        Some(Order {
            key: OrderKey {
                exchange: ExchangeId::BybitSpot,
                instrument: InstrumentNameExchange::new(resp.symbol),
                strategy: crate::order::id::StrategyId::unknown(),
                cid: if resp.order_link_id.is_empty() {
                    ClientOrderId::default()
                } else {
                    ClientOrderId::new(&resp.order_link_id)
                },
            },
            side,
            price,
            quantity,
            kind,
            time_in_force,
            state: Open {
                id: OrderId::new(&resp.order_id),
                time_exchange: time,
                filled_quantity: filled,
            },
        })
    }
}

// Response types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BybitResponse<T> {
    ret_code: i32,
    ret_msg: String,
    result: Option<T>,
    time: u64,
}

#[derive(Debug, Deserialize)]
struct WalletBalanceResponse {
    list: Vec<AccountInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountInfo {
    account_type: String,
    coin: Vec<CoinBalance>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoinBalance {
    coin: String,
    wallet_balance: String,
    locked: String,
}

#[derive(Debug, Deserialize)]
struct OrderListResponse {
    list: Vec<OrderInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderInfo {
    order_id: String,
    order_link_id: String,
    symbol: String,
    price: String,
    qty: String,
    side: String,
    order_type: String,
    time_in_force: String,
    order_status: String,
    cum_exec_qty: String,
    created_time: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceOrderResponse {
    order_id: String,
    order_link_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelOrderResponse {
    order_id: String,
    order_link_id: String,
}

#[derive(Debug, Deserialize)]
struct TradeHistoryResponse {
    list: Vec<TradeInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TradeInfo {
    exec_id: String,
    order_id: String,
    symbol: String,
    side: String,
    exec_price: String,
    exec_qty: String,
    exec_fee: String,
    fee_currency: Option<String>,
    exec_time: String,
}