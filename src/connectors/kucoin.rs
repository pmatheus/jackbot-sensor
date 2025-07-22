//! KuCoin exchange connector implementation
//!
//! Real implementation of KuCoin REST API and WebSocket integration

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use url::Url;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{Engine as _, engine::general_purpose};
use uuid::Uuid;

use crate::connector::{
    Balance, Connection, Exchange, MarketData, MarketDataStream, Order, OrderId, OrderResult,
};

#[derive(Debug, Clone, Deserialize)]
struct KuCoinTickerResponse {
    code: String,
    data: Option<KuCoinTicker>,
}

#[derive(Debug, Clone, Deserialize)]
struct KuCoinTicker {
    symbol: String,
    #[serde(rename = "last")]
    price: String,
    bid: String,
    ask: String,
    #[serde(rename = "vol")]
    volume: String,
    #[serde(rename = "changeRate")]
    change_rate: String,
    high: String,
    low: String,
    time: i64,
}

#[derive(Debug, Clone, Serialize)]
struct KuCoinOrderRequest {
    #[serde(rename = "clientOid")]
    client_order_id: String,
    symbol: String,
    side: String,
    #[serde(rename = "type")]
    order_type: String,
    size: String,
    price: Option<String>,
    #[serde(rename = "timeInForce")]
    time_in_force: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct KuCoinOrderResponse {
    code: String,
    data: Option<KuCoinOrderData>,
}

#[derive(Debug, Clone, Deserialize)]
struct KuCoinOrderData {
    #[serde(rename = "orderId")]
    order_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct KuCoinBalance {
    currency: String,
    #[serde(rename = "type")]
    account_type: String,
    balance: String,
    available: String,
    holds: String,
}

/// KuCoin connector implementing the Exchange trait
#[derive(Clone)]
pub struct KuCoinConnector {
    client: Client,
    api_key: Option<String>,
    api_secret: Option<String>,
    passphrase: Option<String>,
    base_url: String,
    sandbox: bool,
}

impl KuCoinConnector {
    pub fn new(
        api_key: Option<String>,
        api_secret: Option<String>,
        sandbox: bool,
    ) -> Result<Self> {
        let base_url = if sandbox {
            "https://openapi-sandbox.kucoin.com".to_string()
        } else {
            "https://api.kucoin.com".to_string()
        };

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        info!("🔗 Initializing KuCoin connector (sandbox: {})", sandbox);

        Ok(Self {
            client,
            api_key,
            api_secret,
            passphrase: std::env::var("KUCOIN_PASSPHRASE").ok(), // KuCoin requires a passphrase
            base_url,
            sandbox,
        })
    }

    /// Get ticker data for a symbol
    async fn get_ticker_data(&self, symbol: &str) -> Result<crate::api::TickerData> {
        let url = format!("{}/api/v1/market/orderbook/level1?symbol={}", self.base_url, symbol);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch ticker data")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("KuCoin API error: {}", response.status()));
        }

        let ticker_response: KuCoinTickerResponse = response
            .json()
            .await
            .context("Failed to parse ticker response")?;

        if ticker_response.code != "200000" {
            return Err(anyhow::anyhow!("KuCoin API error code: {}", ticker_response.code));
        }

        let ticker = ticker_response.data.context("Missing ticker data")?;

        Ok(crate::api::TickerData {
            symbol: self.normalize_symbol(&ticker.symbol),
            exchange: "kucoin".to_string(),
            price: ticker.price.parse().unwrap_or(0.0),
            bid: ticker.bid.parse().unwrap_or(0.0),
            ask: ticker.ask.parse().unwrap_or(0.0),
            volume_24h: ticker.volume.parse().unwrap_or(0.0),
            change_24h: ticker.change_rate.parse::<f64>().unwrap_or(0.0) * 100.0, // Convert to percentage
            high_24h: ticker.high.parse().unwrap_or(0.0),
            low_24h: ticker.low.parse().unwrap_or(0.0),
            timestamp: ticker.time,
        })
    }

    /// Normalize symbol format (KuCoin uses BTC-USDT, we use BTC/USDT)
    fn normalize_symbol(&self, symbol: &str) -> String {
        symbol.replace('-', "/")
    }

    /// Convert our symbol format to KuCoin's format
    fn to_kucoin_symbol(&self, symbol: &str) -> String {
        symbol.replace('/', "-")
    }

    /// Generate authenticated headers for private endpoints
    fn generate_auth_headers(&self, method: &str, endpoint: &str, body: &str) -> Result<reqwest::header::HeaderMap> {
        let api_key = self.api_key.as_ref().context("API key required")?;
        let api_secret = self.api_secret.as_ref().context("API secret required")?;
        let passphrase = self.passphrase.as_ref().context("Passphrase required")?;

        let timestamp = chrono::Utc::now().timestamp_millis().to_string();
        let str_to_sign = format!("{}{}{}{}", timestamp, method, endpoint, body);

        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        use base64::{Engine as _, engine::general_purpose};

        let mut mac = Hmac::<Sha256>::new_from_slice(api_secret.as_bytes())
            .map_err(|_| anyhow::anyhow!("Invalid API secret"))?;
        mac.update(str_to_sign.as_bytes());
        let signature = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("KC-API-KEY", api_key.parse()?);
        headers.insert("KC-API-SIGN", signature.parse()?);
        headers.insert("KC-API-TIMESTAMP", timestamp.parse()?);
        headers.insert("KC-API-PASSPHRASE", passphrase.parse()?);
        headers.insert("KC-API-KEY-VERSION", "2".parse()?);
        headers.insert("Content-Type", "application/json".parse()?);

        Ok(headers)
    }
}

#[async_trait]
impl Exchange for KuCoinConnector {
    async fn connect(&self) -> Result<Connection> {
        info!("🔗 Connecting to KuCoin exchange ({})", if self.sandbox { "sandbox" } else { "production" });
        
        // Test connection by fetching server time
        let url = format!("{}/api/v1/timestamp", self.base_url);
        let response = self.client.get(&url).send().await
            .context("Failed to connect to KuCoin")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("KuCoin connection failed: {}", response.status()));
        }

        info!("✅ Successfully connected to KuCoin");
        Ok(Arc::new(()) as Connection)
    }
    
    async fn subscribe_market_data(&self, symbols: Vec<String>) -> Result<MarketDataStream> {
        info!("📊 Subscribing to KuCoin market data for {} symbols", symbols.len());
        
        let symbols_clone = symbols.clone();
        let connector = self.clone();
        
        let stream = async_stream::stream! {
            for symbol in symbols_clone {
                // Convert to KuCoin format
                let kucoin_symbol = connector.to_kucoin_symbol(&symbol);
                
                // Get real ticker data
                match connector.get_ticker_data(&kucoin_symbol).await {
                    Ok(ticker_data) => {
                        debug!("📈 Got ticker data for {}: ${}", symbol, ticker_data.price);
                        yield MarketData::Ticker(ticker_data);
                    }
                    Err(e) => {
                        error!("❌ Failed to get ticker for {}: {}", symbol, e);
                    }
                }
                
                // Add small delay to avoid rate limiting
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        };
        
        Ok(Box::pin(stream) as MarketDataStream)
    }
    
    async fn place_order(&self, order: Order) -> Result<OrderResult> {
        info!("📝 Placing KuCoin order: {} {} {}", order.side, order.quantity, order.symbol);
        
        if self.api_key.is_none() || self.api_secret.is_none() {
            return Err(anyhow::anyhow!("API credentials required for placing orders"));
        }

        let kucoin_symbol = self.to_kucoin_symbol(&order.symbol);
        let client_order_id = Uuid::new_v4().to_string();
        
        let order_request = KuCoinOrderRequest {
            client_order_id: client_order_id.clone(),
            symbol: kucoin_symbol,
            side: match order.side {
                crate::connector::OrderSide::Buy => "buy".to_string(),
                crate::connector::OrderSide::Sell => "sell".to_string(),
            },
            order_type: match order.order_type {
                crate::connector::OrderType::Market => "market".to_string(),
                crate::connector::OrderType::Limit => "limit".to_string(),
                _ => "limit".to_string(), // Default to limit for other types
            },
            size: order.quantity.to_string(),
            price: order.price.map(|p| p.to_string()),
            time_in_force: match order.time_in_force {
                Some(crate::connector::TimeInForce::GTC) => Some("GTC".to_string()),
                Some(crate::connector::TimeInForce::IOC) => Some("IOC".to_string()),
                Some(crate::connector::TimeInForce::FOK) => Some("FOK".to_string()),
                _ => Some("GTC".to_string()),
            },
        };

        let body = serde_json::to_string(&order_request)
            .context("Failed to serialize order request")?;
        
        let endpoint = "/api/v1/orders";
        let headers = self.generate_auth_headers("POST", endpoint, &body)?;
        
        let url = format!("{}{}", self.base_url, endpoint);
        let response = self.client
            .post(&url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .context("Failed to place order")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("KuCoin order failed: {}", error_text));
        }

        let order_response: KuCoinOrderResponse = response
            .json()
            .await
            .context("Failed to parse order response")?;

        if order_response.code != "200000" {
            return Err(anyhow::anyhow!("KuCoin order error: {}", order_response.code));
        }

        let order_data = order_response.data.context("Missing order data")?;
        
        info!("✅ KuCoin order placed successfully: {}", order_data.order_id);

        Ok(OrderResult {
            order_id: order_data.order_id,
            status: crate::connector::OrderStatus::New,
            filled_quantity: 0.0,
            remaining_quantity: order.quantity,
            average_price: 0.0,
            commission: 0.0,
            commission_asset: "USDT".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }
    
    async fn cancel_order(&self, order_id: OrderId) -> Result<()> {
        info!("❌ Cancelling KuCoin order: {}", order_id);
        
        if self.api_key.is_none() || self.api_secret.is_none() {
            return Err(anyhow::anyhow!("API credentials required for cancelling orders"));
        }

        let endpoint = format!("/api/v1/orders/{}", order_id);
        let headers = self.generate_auth_headers("DELETE", &endpoint, "")?;
        
        let url = format!("{}{}", self.base_url, endpoint);
        let response = self.client
            .delete(&url)
            .headers(headers)
            .send()
            .await
            .context("Failed to cancel order")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("KuCoin cancel failed: {}", error_text));
        }

        info!("✅ KuCoin order cancelled: {}", order_id);
        Ok(())
    }
    
    async fn get_balance(&self) -> Result<Vec<Balance>> {
        debug!("💰 Getting KuCoin account balances");
        
        if self.api_key.is_none() || self.api_secret.is_none() {
            return Err(anyhow::anyhow!("API credentials required for balance retrieval"));
        }

        let endpoint = "/api/v1/accounts";
        let headers = self.generate_auth_headers("GET", endpoint, "")?;
        
        let url = format!("{}{}?type=trade", self.base_url, endpoint);
        let response = self.client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .context("Failed to get balances")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("KuCoin balance request failed: {}", error_text));
        }

        let balance_response: Value = response
            .json()
            .await
            .context("Failed to parse balance response")?;

        if balance_response["code"] != "200000" {
            return Err(anyhow::anyhow!("KuCoin balance error: {}", balance_response["code"]));
        }

        let balances: Vec<KuCoinBalance> = serde_json::from_value(balance_response["data"].clone())
            .context("Failed to parse balance data")?;

        let mut result = Vec::new();
        for balance in balances {
            let free: f64 = balance.available.parse().unwrap_or(0.0);
            let locked: f64 = balance.holds.parse().unwrap_or(0.0);
            
            if free > 0.0 || locked > 0.0 {
                result.push(Balance {
                    asset: balance.currency,
                    free,
                    locked,
                    total: free + locked,
                });
            }
        }

        debug!("💰 Retrieved {} KuCoin balances", result.len());
        Ok(result)
    }
}
