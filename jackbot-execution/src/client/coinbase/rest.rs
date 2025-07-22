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
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::str::FromStr;
use tracing::error;

type HmacSha256 = Hmac<Sha256>;

const COINBASE_API_BASE: &str = "https://api.coinbase.com";
const COINBASE_ADVANCED_TRADE_PATH: &str = "/api/v3/brokerage";

#[derive(Clone)]
pub struct CoinbaseRestConfig {
    pub api_key: String,
    pub api_secret: String,
    pub api_passphrase: String,  // Coinbase requires passphrase in addition to key/secret
    pub sandbox: bool,
}

#[derive(Clone)]
pub struct CoinbaseRestClient {
    config: CoinbaseRestConfig,
    client: Client,
    base_url: String,
}

impl CoinbaseRestClient {
    pub fn new(config: CoinbaseRestConfig) -> Self {
        let base_url = if config.sandbox {
            "https://api-public.sandbox.exchange.coinbase.com".to_string()
        } else {
            COINBASE_API_BASE.to_string()
        };

        Self {
            config,
            client: Client::new(),
            base_url,
        }
    }

    fn sign_request(&self, method: &str, request_path: &str, body: &str) -> CoinbaseAuth {
        let timestamp = Utc::now().timestamp();
        let message = format!("{}{}{}{}", timestamp, method, request_path, body);
        
        use base64::{Engine as _, engine::general_purpose};
        
        let decoded_secret = general_purpose::STANDARD
            .decode(&self.config.api_secret)
            .expect("Invalid API secret");
        let mut mac = HmacSha256::new_from_slice(&decoded_secret)
            .expect("HMAC can take key of any size");
        mac.update(message.as_bytes());
        let signature = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        CoinbaseAuth {
            key: self.config.api_key.clone(),
            signature,
            timestamp: timestamp.to_string(),
            passphrase: self.config.api_passphrase.clone(),
        }
    }

    pub async fn fetch_balances(
        &self,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        let request_path = format!("{}/accounts", COINBASE_ADVANCED_TRADE_PATH);
        let auth = self.sign_request("GET", &request_path, "");

        let url = format!("{}{}", self.base_url, request_path);
        let response = self
            .client
            .get(&url)
            .header("CB-ACCESS-KEY", auth.key)
            .header("CB-ACCESS-SIGN", auth.signature)
            .header("CB-ACCESS-TIMESTAMP", auth.timestamp)
            .header("CB-ACCESS-PASSPHRASE", auth.passphrase)
            .send()
            .await
            .map_err(|e| UnindexedClientError::Connectivity(ConnectivityError::Socket(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Coinbase API error: {} - {}", status, error_text);
            return Err(UnindexedClientError::Api(UnindexedApiError::OrderRejected(
                format!("API error: {} - {}", status, error_text)
            )));
        }

        let accounts_response: AccountsResponse = response
            .json()
            .await
            .map_err(|e| UnindexedClientError::AccountSnapshot(e.to_string()))?;

        let balances = accounts_response
            .accounts
            .into_iter()
            .filter_map(|account| {
                let available = Decimal::from_str(&account.available_balance.value).ok()?;
                let hold = Decimal::from_str(&account.hold.value).ok()?;
                let total = available + hold;
                
                if total > Decimal::ZERO {
                    Some(AssetBalance {
                        asset: AssetNameExchange::new(account.currency),
                        balance: Balance { total, free: available },
                        time_exchange: Utc::now(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(balances)
    }

    pub async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError> {
        let request_path = format!("{}/orders/historical/batch", COINBASE_ADVANCED_TRADE_PATH);
        let body = r#"{"order_status":["OPEN","PENDING"]}"#;
        let auth = self.sign_request("GET", &request_path, body);

        let url = format!("{}{}", self.base_url, request_path);
        let response = self
            .client
            .get(&url)
            .header("CB-ACCESS-KEY", auth.key)
            .header("CB-ACCESS-SIGN", auth.signature)
            .header("CB-ACCESS-TIMESTAMP", auth.timestamp)
            .header("CB-ACCESS-PASSPHRASE", auth.passphrase)
            .header("Content-Type", "application/json")
            .body(body)
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

        let orders_response: OrdersResponse = response
            .json()
            .await
            .map_err(|e| UnindexedClientError::AccountSnapshot(e.to_string()))?;

        let parsed_orders = orders_response
            .orders
            .into_iter()
            .filter_map(|o| self.parse_order_response(o))
            .collect();

        Ok(parsed_orders)
    }

    pub async fn place_order(
        &self,
        request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Result<Order<ExchangeId, InstrumentNameExchange, Open>, UnindexedOrderError> {
        let order_config = match request.state.kind {
            OrderKind::Market => OrderConfig::MarketOrder {
                quote_size: None,
                base_size: Some(request.state.quantity.to_string()),
            },
            OrderKind::Limit => {
                let mut config = OrderConfig::LimitOrder {
                    limit_price: request.state.price.to_string(),
                    base_size: request.state.quantity.to_string(),
                    post_only: false,
                    end_time: None,
                };
                
                match request.state.time_in_force {
                    TimeInForce::GoodUntilCancelled { post_only } => {
                        if let OrderConfig::LimitOrder { post_only: ref mut po, .. } = config {
                            *po = post_only;
                        }
                    },
                    TimeInForce::ImmediateOrCancel => {
                        // IOC not directly supported, use GTT with 1 second
                        if let OrderConfig::LimitOrder { end_time: ref mut et, .. } = config {
                            *et = Some(Utc::now().timestamp() + 1);
                        }
                    },
                    TimeInForce::FillOrKill => {
                        // FOK not directly supported, would need to simulate
                    },
                    TimeInForce::GoodUntilEndOfDay => {
                        // GTC for unsupported TIF
                    },
                }
                config
            }
            OrderKind::Stop => {
                // Stop orders not directly supported, use market order
                OrderConfig::MarketOrder {
                    quote_size: None,
                    base_size: Some(request.state.quantity.to_string()),
                }
            }
            OrderKind::StopLimit => {
                // Stop limit orders not directly supported, use limit order
                OrderConfig::LimitOrder {
                    limit_price: request.state.price.to_string(),
                    base_size: request.state.quantity.to_string(),
                    post_only: false,
                    end_time: None,
                }
            }
            OrderKind::Jackpot | OrderKind::Prophetic | OrderKind::EventTriggered => {
                // Sensor-specific order types - default to market order
                OrderConfig::MarketOrder {
                    quote_size: None,
                    base_size: Some(request.state.quantity.to_string()),
                }
            }
        };

        let place_order_request = PlaceOrderRequest {
            client_order_id: if request.key.cid.is_unknown() {
                None
            } else {
                Some(request.key.cid.to_string())
            },
            product_id: request.key.instrument.as_str().to_string(),
            side: match request.state.side {
                Side::Buy => "BUY",
                Side::Sell => "SELL",
            }.to_string(),
            order_configuration: order_config,
        };

        let body = serde_json::to_string(&place_order_request)
            .map_err(|e| UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(e.to_string())))?;
        
        let request_path = format!("{}/orders", COINBASE_ADVANCED_TRADE_PATH);
        let auth = self.sign_request("POST", &request_path, &body);

        let url = format!("{}{}", self.base_url, request_path);
        let response = self
            .client
            .post(&url)
            .header("CB-ACCESS-KEY", auth.key)
            .header("CB-ACCESS-SIGN", auth.signature)
            .header("CB-ACCESS-TIMESTAMP", auth.timestamp)
            .header("CB-ACCESS-PASSPHRASE", auth.passphrase)
            .header("Content-Type", "application/json")
            .body(body)
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

        let order_resp: PlaceOrderResponse = response
            .json()
            .await
            .map_err(|e| UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(e.to_string())))?;

        if !order_resp.success {
            return Err(UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(
                order_resp.error_response.map(|e| e.message).unwrap_or_else(|| "Unknown error".to_string())
            )));
        }

        order_resp.success_response
            .and_then(|resp| self.parse_order_response(resp.order))
            .ok_or_else(|| UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected("Failed to parse order response".to_string())))
    }

    pub async fn cancel_order(
        &self,
        request: OrderRequestCancel<ExchangeId, InstrumentNameExchange>,
    ) -> Result<Cancelled, UnindexedOrderError> {
        let order_ids = if let Some(order_id) = &request.state.id {
            vec![order_id.to_string()]
        } else if !request.key.cid.is_unknown() {
            // Coinbase doesn't support cancelling by client order ID directly
            return Err(UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(
                "Coinbase requires order ID for cancellation".to_string(),
            )));
        } else {
            return Err(UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(
                "Either order ID or client order ID must be provided".to_string(),
            )));
        };

        let cancel_request = CancelOrdersRequest { order_ids };
        let body = serde_json::to_string(&cancel_request)
            .map_err(|e| UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(e.to_string())))?;
        
        let request_path = format!("{}/orders/batch_cancel", COINBASE_ADVANCED_TRADE_PATH);
        let auth = self.sign_request("POST", &request_path, &body);

        let url = format!("{}{}", self.base_url, request_path);
        let response = self
            .client
            .post(&url)
            .header("CB-ACCESS-KEY", auth.key)
            .header("CB-ACCESS-SIGN", auth.signature)
            .header("CB-ACCESS-TIMESTAMP", auth.timestamp)
            .header("CB-ACCESS-PASSPHRASE", auth.passphrase)
            .header("Content-Type", "application/json")
            .body(body)
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

        let cancel_resp: CancelOrdersResponse = response
            .json()
            .await
            .map_err(|e| UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(e.to_string())))?;

        if let Some(result) = cancel_resp.results.first() {
            if result.success {
                Ok(Cancelled {
                    id: request.state.id.unwrap_or_else(|| OrderId::default()),
                    time_exchange: Utc::now(),
                })
            } else {
                Err(UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(
                    result.failure_reason.clone().unwrap_or_else(|| "Unknown error".to_string())
                )))
            }
        } else {
            Err(UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(
                "No response from cancel request".to_string()
            )))
        }
    }

    pub async fn fetch_trades(
        &self,
        time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        let request_path = format!("{}/orders/historical/fills", COINBASE_ADVANCED_TRADE_PATH);
        let params = format!("?start_sequence_timestamp={}", time_since.to_rfc3339());
        let full_path = format!("{}{}", request_path, params);
        let auth = self.sign_request("GET", &full_path, "");

        let url = format!("{}{}", self.base_url, full_path);
        let response = self
            .client
            .get(&url)
            .header("CB-ACCESS-KEY", auth.key)
            .header("CB-ACCESS-SIGN", auth.signature)
            .header("CB-ACCESS-TIMESTAMP", auth.timestamp)
            .header("CB-ACCESS-PASSPHRASE", auth.passphrase)
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

        let fills_response: FillsResponse = response
            .json()
            .await
            .map_err(|e| UnindexedClientError::AccountSnapshot(e.to_string()))?;

        let parsed_trades = fills_response
            .fills
            .into_iter()
            .filter_map(|f| {
                let price = Decimal::from_str(&f.price).ok()?;
                let size = Decimal::from_str(&f.size).ok()?;
                let commission = Decimal::from_str(&f.commission).ok()?;
                let time = DateTime::parse_from_rfc3339(&f.trade_time).ok()?.with_timezone(&Utc);

                Some(Trade {
                    id: TradeId::new(f.trade_id),
                    order_id: OrderId::new(&f.order_id),
                    instrument: InstrumentNameExchange::new(f.product_id),
                    strategy: crate::order::id::StrategyId::unknown(),
                    time_exchange: time,
                    side: match f.side.as_str() {
                        "BUY" => Side::Buy,
                        "SELL" => Side::Sell,
                        _ => return None,
                    },
                    price,
                    quantity: size,
                    fees: AssetFees::quote_fees(commission),
                })
            })
            .collect();

        Ok(parsed_trades)
    }

    fn parse_order_response(
        &self,
        resp: OrderResponse,
    ) -> Option<Order<ExchangeId, InstrumentNameExchange, Open>> {
        let config = resp.order_configuration?;
        let (price, quantity, kind, time_in_force) = match config {
            OrderConfig::MarketOrder { base_size, .. } => {
                let quantity = base_size.and_then(|s| Decimal::from_str(&s).ok())?;
                (Decimal::ZERO, quantity, OrderKind::Market, TimeInForce::ImmediateOrCancel)
            }
            OrderConfig::LimitOrder { limit_price, base_size, post_only, .. } => {
                let price = Decimal::from_str(&limit_price).ok()?;
                let quantity = Decimal::from_str(&base_size).ok()?;
                (price, quantity, OrderKind::Limit, TimeInForce::GoodUntilCancelled { post_only })
            }
        };

        let filled_size = Decimal::from_str(&resp.filled_size).ok()?;
        let time = DateTime::parse_from_rfc3339(&resp.created_time).ok()?.with_timezone(&Utc);

        let side = match resp.side.as_str() {
            "BUY" => Side::Buy,
            "SELL" => Side::Sell,
            _ => return None,
        };

        Some(Order {
            key: OrderKey {
                exchange: ExchangeId::Coinbase,
                instrument: InstrumentNameExchange::new(resp.product_id),
                strategy: crate::order::id::StrategyId::unknown(),
                cid: resp.client_order_id.map(|id| ClientOrderId::new(&id)).unwrap_or_default(),
            },
            side,
            price,
            quantity,
            kind,
            time_in_force,
            state: Open {
                id: OrderId::new(&resp.order_id),
                time_exchange: time,
                filled_quantity: filled_size,
            },
        })
    }
}

#[derive(Clone)]
struct CoinbaseAuth {
    key: String,
    signature: String,
    timestamp: String,
    passphrase: String,
}

// Request/Response types
#[derive(Debug, Serialize)]
struct PlaceOrderRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    client_order_id: Option<String>,
    product_id: String,
    side: String,
    order_configuration: OrderConfig,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum OrderConfig {
    MarketOrder {
        #[serde(skip_serializing_if = "Option::is_none")]
        quote_size: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        base_size: Option<String>,
    },
    LimitOrder {
        limit_price: String,
        base_size: String,
        #[serde(default)]
        post_only: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        end_time: Option<i64>,
    },
}

#[derive(Debug, Deserialize)]
struct PlaceOrderResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    success_response: Option<SuccessResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_response: Option<ErrorResponse>,
}

#[derive(Debug, Deserialize)]
struct SuccessResponse {
    order_id: String,
    product_id: String,
    side: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_order_id: Option<String>,
    order: OrderResponse,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
    message: String,
    error_details: String,
}

#[derive(Debug, Deserialize)]
struct OrderResponse {
    order_id: String,
    product_id: String,
    side: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_order_id: Option<String>,
    created_time: String,
    filled_size: String,
    status: String,
    order_configuration: Option<OrderConfig>,
}

#[derive(Debug, Serialize)]
struct CancelOrdersRequest {
    order_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CancelOrdersResponse {
    results: Vec<CancelResult>,
}

#[derive(Debug, Deserialize)]
struct CancelResult {
    success: bool,
    order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AccountsResponse {
    accounts: Vec<Account>,
}

#[derive(Debug, Deserialize)]
struct Account {
    currency: String,
    available_balance: CurrencyValue,
    hold: CurrencyValue,
}

#[derive(Debug, Deserialize)]
struct CurrencyValue {
    value: String,
    currency: String,
}

#[derive(Debug, Deserialize)]
struct OrdersResponse {
    orders: Vec<OrderResponse>,
}

#[derive(Debug, Deserialize)]
struct FillsResponse {
    fills: Vec<Fill>,
}

#[derive(Debug, Deserialize)]
struct Fill {
    trade_id: String,
    product_id: String,
    order_id: String,
    side: String,
    price: String,
    size: String,
    commission: String,
    trade_time: String,
}