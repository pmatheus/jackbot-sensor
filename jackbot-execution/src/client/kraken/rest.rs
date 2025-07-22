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
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{error, warn};

type HmacSha512 = Hmac<sha2::Sha512>;

const KRAKEN_API_BASE: &str = "https://api.kraken.com";
const KRAKEN_API_VERSION: &str = "/0";

/// Kraken's rate limit counter system
#[derive(Clone)]
pub struct KrakenRateLimit {
    counter: Arc<AtomicI32>,
    max_counter: i32,
    decay_rate: f64, // points per second
    last_decay: Arc<Mutex<Instant>>,
}

impl KrakenRateLimit {
    pub fn new(tier: KrakenTier) -> Self {
        let (max_counter, decay_rate) = match tier {
            KrakenTier::Starter => (15, 0.33),
            KrakenTier::Intermediate => (20, 0.5),
            KrakenTier::Pro => (20, 1.0),
        };

        Self {
            counter: Arc::new(AtomicI32::new(0)),
            max_counter,
            decay_rate,
            last_decay: Arc::new(Mutex::new(Instant::now())),
        }
    }

    async fn can_make_request(&self, cost: i32) -> bool {
        self.decay_counter().await;
        let current = self.counter.load(Ordering::Relaxed);
        current + cost <= self.max_counter
    }

    async fn consume(&self, cost: i32) -> bool {
        self.decay_counter().await;
        let current = self.counter.load(Ordering::Relaxed);
        if current + cost <= self.max_counter {
            self.counter.store(current + cost, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    async fn decay_counter(&self) {
        let mut last_decay = self.last_decay.lock().await;
        let now = Instant::now();
        let elapsed = now.duration_since(*last_decay).as_secs_f64();
        
        if elapsed > 0.0 {
            let decay_amount = (elapsed * self.decay_rate) as i32;
            if decay_amount > 0 {
                let current = self.counter.load(Ordering::Relaxed);
                let new_value = (current - decay_amount).max(0);
                self.counter.store(new_value, Ordering::Relaxed);
                *last_decay = now;
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum KrakenTier {
    Starter,
    Intermediate,
    Pro,
}

#[derive(Clone)]
pub struct KrakenRestConfig {
    pub api_key: String,
    pub api_secret: String,
    pub tier: KrakenTier,
    pub sandbox: bool,
}

#[derive(Clone)]
pub struct KrakenRestClient {
    config: KrakenRestConfig,
    client: Client,
    base_url: String,
    rate_limit: KrakenRateLimit,
}

impl KrakenRestClient {
    pub fn new(config: KrakenRestConfig) -> Self {
        let base_url = if config.sandbox {
            "https://api.demo-futures.kraken.com".to_string()
        } else {
            KRAKEN_API_BASE.to_string()
        };

        let rate_limit = KrakenRateLimit::new(config.tier.clone());

        Self {
            config,
            client: Client::new(),
            base_url,
            rate_limit,
        }
    }

    fn sign_request(&self, uri_path: &str, nonce: u64, postdata: &str) -> KrakenAuth {
        use base64::{Engine as _, engine::general_purpose};
        
        let nonce_str = nonce.to_string();
        let postdata = format!("nonce={}&{}", nonce_str, postdata);
        
        // Create the signature
        let message = format!("{}{}", uri_path, sha2::Digest::digest(sha2::Sha256::new(), postdata.as_bytes()));
        
        let decoded_secret = general_purpose::STANDARD
            .decode(&self.config.api_secret)
            .expect("Invalid API secret");
        
        let mut mac = HmacSha512::new_from_slice(&decoded_secret)
            .expect("HMAC can take key of any size");
        mac.update(message.as_bytes());
        let signature = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        KrakenAuth {
            key: self.config.api_key.clone(),
            signature,
        }
    }

    async fn make_private_request<T>(&self, endpoint: &str, params: &str, cost: i32) -> Result<T, UnindexedClientError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        // Check rate limit
        if !self.rate_limit.can_make_request(cost).await {
            return Err(UnindexedClientError::Api(UnindexedApiError::RateLimited));
        }

        let nonce = Utc::now().timestamp_nanos() as u64;
        let uri_path = format!("{}/private/{}", KRAKEN_API_VERSION, endpoint);
        let auth = self.sign_request(&uri_path, nonce, params);
        
        let postdata = if params.is_empty() {
            format!("nonce={}", nonce)
        } else {
            format!("nonce={}&{}", nonce, params)
        };

        let url = format!("{}{}", self.base_url, uri_path);
        
        // Consume rate limit
        if !self.rate_limit.consume(cost).await {
            return Err(UnindexedClientError::Api(UnindexedApiError::RateLimited));
        }

        let response = self
            .client
            .post(&url)
            .header("API-Key", auth.key)
            .header("API-Sign", auth.signature)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(postdata)
            .send()
            .await
            .map_err(|e| UnindexedClientError::Connectivity(ConnectivityError::Socket(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Kraken API error: {} - {}", status, error_text);
            return Err(UnindexedClientError::Api(UnindexedApiError::OrderRejected(
                format!("API error: {} - {}", status, error_text)
            )));
        }

        let kraken_response: KrakenResponse<T> = response
            .json()
            .await
            .map_err(|e| UnindexedClientError::AccountSnapshot(e.to_string()))?;

        if !kraken_response.error.is_empty() {
            let error_msg = kraken_response.error.join(", ");
            if error_msg.contains("EAPI:Rate limit exceeded") {
                return Err(UnindexedClientError::Api(UnindexedApiError::RateLimited));
            }
            return Err(UnindexedClientError::Api(UnindexedApiError::OrderRejected(error_msg)));
        }

        kraken_response.result.ok_or_else(|| {
            UnindexedClientError::Api(UnindexedApiError::OrderRejected("No result in response".to_string()))
        })
    }

    pub async fn fetch_balances(
        &self,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        let balance_response: BalanceResponse = self
            .make_private_request("Balance", "", 1)
            .await?;

        let balances = balance_response
            .into_iter()
            .filter_map(|(asset, balance_str)| {
                let total = Decimal::from_str(&balance_str).ok()?;
                if total > Decimal::ZERO {
                    Some(AssetBalance {
                        asset: AssetNameExchange::new(kraken_asset_to_standard(&asset)),
                        balance: Balance { total, free: total }, // Kraken Balance endpoint doesn't distinguish free/locked
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
        let orders_response: OpenOrdersResponse = self
            .make_private_request("OpenOrders", "", 1)
            .await?;

        let parsed_orders = orders_response
            .open
            .into_iter()
            .filter_map(|(order_id, order_info)| {
                self.parse_kraken_order(&order_id, order_info)
            })
            .collect();

        Ok(parsed_orders)
    }

    pub async fn place_order(
        &self,
        request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Result<Order<ExchangeId, InstrumentNameExchange, Open>, UnindexedOrderError> {
        let pair = standard_symbol_to_kraken(&request.key.instrument.as_str());
        let order_type = match request.state.kind {
            OrderKind::Market => "market",
            OrderKind::Limit => "limit",
            OrderKind::Stop => "stop-loss",
            OrderKind::StopLimit => "stop-loss-limit",
            _ => "limit", // Default for sensor-specific orders
        };
        
        let side = match request.state.side {
            Side::Buy => "buy",
            Side::Sell => "sell",
        };

        let mut params = vec![
            format!("pair={}", pair),
            format!("type={}", side),
            format!("ordertype={}", order_type),
            format!("volume={}", request.state.quantity),
        ];

        if request.state.kind == OrderKind::Limit || request.state.kind == OrderKind::StopLimit {
            params.push(format!("price={}", request.state.price));
        }

        // Add time in force
        match request.state.time_in_force {
            TimeInForce::ImmediateOrCancel => params.push("timeInForce=IOC".to_string()),
            TimeInForce::FillOrKill => params.push("timeInForce=FOK".to_string()),
            TimeInForce::GoodUntilCancelled { post_only } => {
                if post_only {
                    params.push("oflags=post".to_string());
                }
            },
            _ => {},
        }

        // Add client order ID if provided
        if !request.key.cid.is_unknown() {
            params.push(format!("userref={}", request.key.cid.to_string()));
        }

        let params_str = params.join("&");
        
        let add_order_response: AddOrderResponse = self
            .make_private_request("AddOrder", &params_str, 1)
            .await
            .map_err(|e| UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(e.to_string())))?;

        if let Some(order_id) = add_order_response.txid.first() {
            Ok(Order {
                key: OrderKey {
                    exchange: ExchangeId::Kraken,
                    instrument: request.key.instrument.clone(),
                    strategy: request.key.strategy,
                    cid: request.key.cid.clone(),
                },
                side: request.state.side,
                price: request.state.price,
                quantity: request.state.quantity,
                kind: request.state.kind,
                time_in_force: request.state.time_in_force,
                state: Open {
                    id: OrderId::new(order_id),
                    time_exchange: Utc::now(),
                    filled_quantity: Decimal::ZERO,
                },
            })
        } else {
            Err(UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(
                "No order ID returned".to_string()
            )))
        }
    }

    pub async fn cancel_order(
        &self,
        request: OrderRequestCancel<ExchangeId, InstrumentNameExchange>,
    ) -> Result<Cancelled, UnindexedOrderError> {
        let params = if let Some(order_id) = &request.state.id {
            format!("txid={}", order_id.to_string())
        } else {
            return Err(UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(
                "Order ID required for cancellation".to_string(),
            )));
        };

        let _cancel_response: CancelOrderResponse = self
            .make_private_request("CancelOrder", &params, 1)
            .await
            .map_err(|e| UnindexedOrderError::Rejected(UnindexedApiError::OrderRejected(e.to_string())))?;

        Ok(Cancelled {
            id: request.state.id.unwrap_or_else(|| OrderId::default()),
            time_exchange: Utc::now(),
        })
    }

    pub async fn fetch_trades(
        &self,
        time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        let params = format!("start={}", time_since.timestamp());
        
        let trades_response: TradesHistoryResponse = self
            .make_private_request("TradesHistory", &params, 2) // Trades history costs 2
            .await?;

        let parsed_trades = trades_response
            .trades
            .into_iter()
            .filter_map(|(trade_id, trade_info)| {
                self.parse_kraken_trade(&trade_id, trade_info)
            })
            .collect();

        Ok(parsed_trades)
    }

    fn parse_kraken_order(
        &self,
        order_id: &str,
        order_info: KrakenOrderInfo,
    ) -> Option<Order<ExchangeId, InstrumentNameExchange, Open>> {
        let price = Decimal::from_str(&order_info.descr.price).ok()?;
        let quantity = Decimal::from_str(&order_info.vol).ok()?;
        let filled = Decimal::from_str(&order_info.vol_exec).ok()?;
        let time = Utc.timestamp_opt(order_info.opentm as i64, 0).single()?;

        let side = match order_info.descr.type_.as_str() {
            "buy" => Side::Buy,
            "sell" => Side::Sell,
            _ => return None,
        };

        let kind = match order_info.descr.ordertype.as_str() {
            "market" => OrderKind::Market,
            "limit" => OrderKind::Limit,
            "stop-loss" => OrderKind::Stop,
            "stop-loss-limit" => OrderKind::StopLimit,
            _ => OrderKind::Limit,
        };

        Some(Order {
            key: OrderKey {
                exchange: ExchangeId::Kraken,
                instrument: InstrumentNameExchange::new(kraken_symbol_to_standard(&order_info.descr.pair)),
                strategy: crate::order::id::StrategyId::unknown(),
                cid: order_info.userref.map(|id| ClientOrderId::new(&id.to_string())).unwrap_or_default(),
            },
            side,
            price,
            quantity,
            kind,
            time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
            state: Open {
                id: OrderId::new(order_id),
                time_exchange: time,
                filled_quantity: filled,
            },
        })
    }

    fn parse_kraken_trade(
        &self,
        trade_id: &str,
        trade_info: KrakenTradeInfo,
    ) -> Option<Trade<QuoteAsset, InstrumentNameExchange>> {
        let price = Decimal::from_str(&trade_info.price).ok()?;
        let quantity = Decimal::from_str(&trade_info.vol).ok()?;
        let fee = Decimal::from_str(&trade_info.fee).ok()?;
        let time = Utc.timestamp_opt(trade_info.time as i64, 0).single()?;

        let side = match trade_info.type_.as_str() {
            "buy" => Side::Buy,
            "sell" => Side::Sell,
            _ => return None,
        };

        Some(Trade {
            id: TradeId::new(trade_id.to_string()),
            order_id: OrderId::new(&trade_info.ordertxid),
            instrument: InstrumentNameExchange::new(kraken_symbol_to_standard(&trade_info.pair)),
            strategy: crate::order::id::StrategyId::unknown(),
            time_exchange: time,
            side,
            price,
            quantity,
            fees: AssetFees::quote_fees(fee),
        })
    }
}

#[derive(Clone)]
struct KrakenAuth {
    key: String,
    signature: String,
}

// Helper functions for Kraken's unique asset naming
fn kraken_asset_to_standard(kraken_asset: &str) -> String {
    match kraken_asset {
        "XXBT" => "BTC".to_string(),
        "XETH" => "ETH".to_string(),
        "ZUSD" => "USD".to_string(),
        "ZEUR" => "EUR".to_string(),
        "XLTC" => "LTC".to_string(),
        "XXRP" => "XRP".to_string(),
        "XZEC" => "ZEC".to_string(),
        "XXLM" => "XLM".to_string(),
        "XXMR" => "XMR".to_string(),
        asset if asset.starts_with('X') && asset.len() == 4 => asset[1..].to_string(),
        asset if asset.starts_with('Z') && asset.len() == 4 => asset[1..].to_string(),
        _ => kraken_asset.to_string(),
    }
}

fn standard_symbol_to_kraken(symbol: &str) -> String {
    // Common Kraken pairs mapping
    match symbol {
        "BTCUSD" => "XBTUSD".to_string(),
        "ETHUSD" => "ETHUSD".to_string(),
        "BTCEUR" => "XBTEUR".to_string(),
        "ETHEUR" => "ETHEUR".to_string(),
        _ => symbol.to_string(),
    }
}

fn kraken_symbol_to_standard(kraken_symbol: &str) -> String {
    match kraken_symbol {
        "XBTUSD" => "BTCUSD".to_string(),
        "XBTEUR" => "BTCEUR".to_string(),
        "ETHUSD" => "ETHUSD".to_string(),
        "ETHEUR" => "ETHEUR".to_string(),
        _ => kraken_symbol.to_string(),
    }
}

// Request/Response types
#[derive(Debug, Deserialize)]
struct KrakenResponse<T> {
    error: Vec<String>,
    result: Option<T>,
}

type BalanceResponse = HashMap<String, String>;

#[derive(Debug, Deserialize)]
struct OpenOrdersResponse {
    open: HashMap<String, KrakenOrderInfo>,
}

#[derive(Debug, Deserialize)]
struct KrakenOrderInfo {
    descr: KrakenOrderDescr,
    vol: String,
    vol_exec: String,
    opentm: f64,
    userref: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct KrakenOrderDescr {
    pair: String,
    #[serde(rename = "type")]
    type_: String,
    ordertype: String,
    price: String,
}

#[derive(Debug, Deserialize)]
struct AddOrderResponse {
    txid: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CancelOrderResponse {
    count: u32,
}

#[derive(Debug, Deserialize)]
struct TradesHistoryResponse {
    trades: HashMap<String, KrakenTradeInfo>,
}

#[derive(Debug, Deserialize)]
struct KrakenTradeInfo {
    ordertxid: String,
    pair: String,
    time: f64,
    #[serde(rename = "type")]
    type_: String,
    price: String,
    vol: String,
    fee: String,
}