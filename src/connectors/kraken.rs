//! Kraken exchange connector implementation
//!
//! Real implementation of Kraken REST API and WebSocket integration

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
use hmac::{Hmac, Mac};
use sha2::{Sha256, Sha512};
use base64::{Engine as _, engine::general_purpose};
use uuid::Uuid;

use crate::connector::{
    Balance, Connection, Exchange, MarketData, MarketDataStream, Order, OrderId, OrderResult,
};

#[derive(Debug, Clone, Deserialize)]
struct KrakenTickerResponse {
    error: Vec<String>,
    result: Option<HashMap<String, KrakenTicker>>,
}

#[derive(Debug, Clone, Deserialize)]
struct KrakenTicker {
    #[serde(rename = "c")]
    close: Vec<String>, // [price, lot volume]
    #[serde(rename = "b")]
    bid: Vec<String>, // [price, whole lot volume, lot volume]
    #[serde(rename = "a")]
    ask: Vec<String>, // [price, whole lot volume, lot volume]
    #[serde(rename = "v")]
    volume: Vec<String>, // [today, last 24 hours]
    #[serde(rename = "p")]
    vwap: Vec<String>, // [today, last 24 hours]
    #[serde(rename = "h")]
    high: Vec<String>, // [today, last 24 hours]
    #[serde(rename = "l")]
    low: Vec<String>, // [today, last 24 hours]
}

#[derive(Debug, Clone, Serialize)]
struct KrakenOrderRequest {
    pair: String,
    #[serde(rename = "type")]
    order_type: String, // buy or sell
    ordertype: String, // market, limit, etc.
    volume: String,
    price: Option<String>,
    #[serde(rename = "userref")]
    user_ref: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct KrakenOrderResponse {
    error: Vec<String>,
    result: Option<KrakenOrderResult>,
}

#[derive(Debug, Clone, Deserialize)]
struct KrakenOrderResult {
    descr: KrakenOrderDescription,
    txid: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct KrakenOrderDescription {
    order: String,
}

#[derive(Debug, Clone, Deserialize)]
struct KrakenBalanceResponse {
    error: Vec<String>,
    result: Option<HashMap<String, String>>,
}

/// Kraken connector implementing the Exchange trait
#[derive(Clone)]
pub struct KrakenConnector {
    client: Client,
    api_key: Option<String>,
    api_secret: Option<String>,
    base_url: String,
    sandbox: bool,
}

impl KrakenConnector {
    pub fn new(
        api_key: Option<String>,
        api_secret: Option<String>,
        sandbox: bool,
    ) -> Result<Self> {
        // Kraken doesn't have a separate sandbox URL, but we can track the flag
        let base_url = "https://api.kraken.com".to_string();

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        info!("🔗 Initializing Kraken connector (sandbox mode: {})", sandbox);
        if sandbox {
            warn!("⚠️ Kraken doesn't have a separate sandbox environment");
        }

        Ok(Self {
            client,
            api_key,
            api_secret,
            base_url,
            sandbox,
        })
    }

    /// Get ticker data for a symbol
    async fn get_ticker_data(&self, symbol: &str) -> Result<crate::api::TickerData> {
        let kraken_symbol = self.to_kraken_symbol(symbol);
        let url = format!("{}/0/public/Ticker?pair={}", self.base_url, kraken_symbol);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch ticker data")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Kraken API error: {}", response.status()));
        }

        let ticker_response: KrakenTickerResponse = response
            .json()
            .await
            .context("Failed to parse ticker response")?;

        if !ticker_response.error.is_empty() {
            return Err(anyhow::anyhow!("Kraken API error: {:?}", ticker_response.error));
        }

        let result = ticker_response.result.context("Missing ticker data")?;
        let (symbol_key, ticker) = result.into_iter().next()
            .context("No ticker data found")?;

        // Calculate 24h change percentage
        let current_price: f64 = ticker.close.get(0)
            .and_then(|p| p.parse().ok())
            .unwrap_or(0.0);
        let high_24h: f64 = ticker.high.get(1)
            .and_then(|p| p.parse().ok())
            .unwrap_or(current_price);
        let low_24h: f64 = ticker.low.get(1)
            .and_then(|p| p.parse().ok())
            .unwrap_or(current_price);
        
        // Estimate 24h change (Kraken doesn't provide direct percentage)
        let change_24h = if low_24h > 0.0 {
            ((current_price - low_24h) / low_24h) * 100.0
        } else {
            0.0
        };

        Ok(crate::api::TickerData {
            symbol: self.normalize_symbol(&symbol_key),
            exchange: "kraken".to_string(),
            price: current_price,
            bid: ticker.bid.get(0).and_then(|p| p.parse().ok()).unwrap_or(0.0),
            ask: ticker.ask.get(0).and_then(|p| p.parse().ok()).unwrap_or(0.0),
            volume_24h: ticker.volume.get(1).and_then(|p| p.parse().ok()).unwrap_or(0.0),
            change_24h,
            high_24h,
            low_24h,
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }

    /// Normalize symbol format (Kraken uses XXBTZUSD, we use BTC/USDT)
    fn normalize_symbol(&self, symbol: &str) -> String {
        // Common Kraken symbol mappings
        match symbol {
            "XXBTZUSD" => "BTC/USD".to_string(),
            "XETHZUSD" => "ETH/USD".to_string(),
            "XXBTZEUR" => "BTC/EUR".to_string(),
            "XETHZEUR" => "ETH/EUR".to_string(),
            _ => {
                // Try to parse generic format like ABCDEFG -> ABC/DEFG
                if symbol.len() >= 6 {
                    let mid = symbol.len() / 2;
                    format!("{}/{}", &symbol[..mid], &symbol[mid..])
                } else {
                    symbol.to_string()
                }
            }
        }
    }

    /// Convert our symbol format to Kraken's format
    fn to_kraken_symbol(&self, symbol: &str) -> String {
        match symbol {
            "BTC/USD" => "XXBTZUSD".to_string(),
            "ETH/USD" => "XETHZUSD".to_string(),
            "BTC/EUR" => "XXBTZEUR".to_string(),
            "ETH/EUR" => "XETHZEUR".to_string(),
            "BTC/USDT" => "XXBTZUSD".to_string(), // Map USDT to USD for Kraken
            "ETH/USDT" => "XETHZUSD".to_string(),
            _ => {
                // Generic conversion: remove / and uppercase
                symbol.replace('/', "").to_uppercase()
            }
        }
    }

    /// Generate authenticated headers for private endpoints
    fn generate_auth_headers(&self, endpoint: &str, nonce: u64, data: &str) -> Result<reqwest::header::HeaderMap> {
        let api_key = self.api_key.as_ref().context("API key required")?;
        let api_secret = self.api_secret.as_ref().context("API secret required")?;

        // Decode base64 secret
        let secret = general_purpose::STANDARD.decode(api_secret)
            .context("Invalid API secret format")?;

        // Create message to sign
        let nonce_data = format!("nonce={}&{}", nonce, data);
        let hash_digest = {
            use sha2::Digest;
            let mut hasher = Sha256::new();
            hasher.update(nonce_data.as_bytes());
            hasher.finalize()
        };

        // Create HMAC-SHA512 signature
        let mut mac = Hmac::<Sha512>::new_from_slice(&secret)
            .map_err(|_| anyhow::anyhow!("Invalid API secret"))?;
        mac.update(endpoint.as_bytes());
        mac.update(&hash_digest);
        let signature = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("API-Key", api_key.parse()?);
        headers.insert("API-Sign", signature.parse()?);
        headers.insert("Content-Type", "application/x-www-form-urlencoded".parse()?);

        Ok(headers)
    }
}

#[async_trait]
impl Exchange for KrakenConnector {
    async fn connect(&self) -> Result<Connection> {
        info!("🔗 Connecting to Kraken exchange");
        
        // Test connection by fetching server time
        let url = format!("{}/0/public/Time", self.base_url);
        let response = self.client.get(&url).send().await
            .context("Failed to connect to Kraken")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Kraken connection failed: {}", response.status()));
        }

        info!("✅ Successfully connected to Kraken");
        Ok(Arc::new(()) as Connection)
    }
    
    async fn subscribe_market_data(&self, symbols: Vec<String>) -> Result<MarketDataStream> {
        info!("📊 Subscribing to Kraken market data for {} symbols", symbols.len());
        
        let symbols_clone = symbols.clone();
        let connector = self.clone();
        
        let stream = async_stream::stream! {
            for symbol in symbols_clone {
                // Get real ticker data from Kraken
                match connector.get_ticker_data(&symbol).await {
                    Ok(ticker_data) => {
                        debug!("📈 Got Kraken ticker data for {}: ${}", symbol, ticker_data.price);
                        yield MarketData::Ticker(ticker_data);
                    }
                    Err(e) => {
                        error!("❌ Failed to get Kraken ticker for {}: {}", symbol, e);
                    }
                }
                
                // Add delay to avoid rate limiting (Kraken has strict limits)
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        };
        
        Ok(Box::pin(stream) as MarketDataStream)
    }
    
    async fn place_order(&self, order: Order) -> Result<OrderResult> {
        info!("📝 Placing Kraken order: {} {} {}", order.side, order.quantity, order.symbol);
        
        if self.api_key.is_none() || self.api_secret.is_none() {
            return Err(anyhow::anyhow!("API credentials required for placing orders"));
        }

        let kraken_symbol = self.to_kraken_symbol(&order.symbol);
        let nonce = chrono::Utc::now().timestamp_nanos() as u64 / 1000; // microseconds
        
        let order_data = format!(
            "pair={}&type={}&ordertype={}&volume={}{}",
            kraken_symbol,
            match order.side {
                crate::connector::OrderSide::Buy => "buy",
                crate::connector::OrderSide::Sell => "sell",
            },
            match order.order_type {
                crate::connector::OrderType::Market => "market",
                crate::connector::OrderType::Limit => "limit",
                _ => "limit", // Default to limit
            },
            order.quantity,
            if let Some(price) = order.price {
                format!("&price={}", price)
            } else {
                String::new()
            }
        );

        let endpoint = "/0/private/AddOrder";
        let full_data = format!("nonce={}&{}", nonce, order_data);
        let headers = self.generate_auth_headers(endpoint, nonce, &order_data)?;
        
        let url = format!("{}{}", self.base_url, endpoint);
        let response = self.client
            .post(&url)
            .headers(headers)
            .body(full_data)
            .send()
            .await
            .context("Failed to place order")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Kraken order failed: {}", error_text));
        }

        let order_response: KrakenOrderResponse = response
            .json()
            .await
            .context("Failed to parse order response")?;

        if !order_response.error.is_empty() {
            return Err(anyhow::anyhow!("Kraken order error: {:?}", order_response.error));
        }

        let result = order_response.result.context("Missing order result")?;
        let order_id = result.txid.get(0)
            .context("Missing transaction ID")?
            .clone();
        
        info!("✅ Kraken order placed successfully: {}", order_id);

        Ok(OrderResult {
            order_id,
            status: crate::connector::OrderStatus::New,
            filled_quantity: 0.0,
            remaining_quantity: order.quantity,
            average_price: 0.0,
            commission: 0.0,
            commission_asset: "USD".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }
    
    async fn cancel_order(&self, order_id: OrderId) -> Result<()> {
        info!("❌ Cancelling Kraken order: {}", order_id);
        
        if self.api_key.is_none() || self.api_secret.is_none() {
            return Err(anyhow::anyhow!("API credentials required for cancelling orders"));
        }

        let nonce = chrono::Utc::now().timestamp_nanos() as u64 / 1000;
        let cancel_data = format!("txid={}", order_id);
        
        let endpoint = "/0/private/CancelOrder";
        let full_data = format!("nonce={}&{}", nonce, cancel_data);
        let headers = self.generate_auth_headers(endpoint, nonce, &cancel_data)?;
        
        let url = format!("{}{}", self.base_url, endpoint);
        let response = self.client
            .post(&url)
            .headers(headers)
            .body(full_data)
            .send()
            .await
            .context("Failed to cancel order")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Kraken cancel failed: {}", error_text));
        }

        let cancel_response: Value = response
            .json()
            .await
            .context("Failed to parse cancel response")?;

        if let Some(errors) = cancel_response["error"].as_array() {
            if !errors.is_empty() {
                return Err(anyhow::anyhow!("Kraken cancel error: {:?}", errors));
            }
        }

        info!("✅ Kraken order cancelled: {}", order_id);
        Ok(())
    }
    
    async fn get_balance(&self) -> Result<Vec<Balance>> {
        debug!("💰 Getting Kraken account balances");
        
        if self.api_key.is_none() || self.api_secret.is_none() {
            return Err(anyhow::anyhow!("API credentials required for balance retrieval"));
        }

        let nonce = chrono::Utc::now().timestamp_nanos() as u64 / 1000;
        let balance_data = String::new(); // Empty for balance request
        
        let endpoint = "/0/private/Balance";
        let full_data = format!("nonce={}", nonce);
        let headers = self.generate_auth_headers(endpoint, nonce, &balance_data)?;
        
        let url = format!("{}{}", self.base_url, endpoint);
        let response = self.client
            .post(&url)
            .headers(headers)
            .body(full_data)
            .send()
            .await
            .context("Failed to get balances")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Kraken balance request failed: {}", error_text));
        }

        let balance_response: KrakenBalanceResponse = response
            .json()
            .await
            .context("Failed to parse balance response")?;

        if !balance_response.error.is_empty() {
            return Err(anyhow::anyhow!("Kraken balance error: {:?}", balance_response.error));
        }

        let balances = balance_response.result.context("Missing balance data")?;
        let mut result = Vec::new();
        
        for (asset, balance_str) in balances {
            let total: f64 = balance_str.parse().unwrap_or(0.0);
            
            if total > 0.0 {
                // Kraken doesn't separate free/locked in balance endpoint
                // You'd need to call OpenOrders to get locked amounts
                result.push(Balance {
                    asset: asset.replace("X", "").replace("Z", ""), // Remove Kraken prefixes
                    free: total, // Assume all is free for now
                    locked: 0.0,
                    total,
                });
            }
        }

        debug!("💰 Retrieved {} Kraken balances", result.len());
        Ok(result)
    }
}
