//! Bitget API types and configuration.

use serde::{Deserialize, Serialize};
use url::Url;

/// Bitget client configuration.
#[derive(Clone, Debug)]
pub struct BitgetConfig {
    /// REST API base URL.
    pub rest_url: Url,
    /// WebSocket base URL.
    pub ws_url: Url,
    /// API key.
    pub api_key: String,
    /// API secret.
    pub api_secret: String,
    /// API passphrase.
    pub passphrase: String,
    /// Trading mode (spot or futures).
    pub trading_mode: TradingMode,
}

impl BitgetConfig {
    /// Create a new Bitget configuration for spot trading.
    pub fn new_spot(api_key: String, api_secret: String, passphrase: String) -> Self {
        Self {
            rest_url: Url::parse("https://api.bitget.com").expect("valid URL"),
            ws_url: Url::parse("wss://ws.bitget.com/spot/v1/stream").expect("valid URL"),
            api_key,
            api_secret,
            passphrase,
            trading_mode: TradingMode::Spot,
        }
    }

    /// Create a new Bitget configuration for futures trading.
    pub fn new_futures(api_key: String, api_secret: String, passphrase: String) -> Self {
        Self {
            rest_url: Url::parse("https://api.bitget.com").expect("valid URL"),
            ws_url: Url::parse("wss://ws.bitget.com/mix/v1/stream").expect("valid URL"),
            api_key,
            api_secret,
            passphrase,
            trading_mode: TradingMode::Futures,
        }
    }
}

/// Trading mode for Bitget.
#[derive(Clone, Debug, PartialEq)]
pub enum TradingMode {
    Spot,
    Futures,
}

/// Bitget API response wrapper.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitgetResponse<T> {
    pub code: String,
    pub msg: String,
    pub request_time: i64,
    pub data: Option<T>,
}

/// Bitget spot account balance.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitgetSpotBalance {
    pub coin_id: String,
    pub coin_name: String,
    pub available: String,
    pub frozen: String,
    pub lock: String,
    pub utime: String,
}

/// Bitget futures account balance.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitgetFuturesBalance {
    pub margin_coin: String,
    pub locked: String,
    pub available: String,
    pub cross_max_available: String,
    pub fixed_max_available: String,
    pub unrealized_pl: String,
    pub equity: String,
    pub utime: String,
}

/// Bitget order request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BitgetOrderRequest {
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub size: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    pub client_oid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<String>,
}

/// Bitget order response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitgetOrderResponse {
    pub order_id: String,
    pub client_oid: String,
}

/// Bitget order info.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitgetOrderInfo {
    pub order_id: String,
    pub client_oid: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: String,
    pub size: String,
    pub filled_qty: String,
    pub filled_amount: String,
    pub status: String,
    pub create_time: String,
    pub update_time: String,
}

/// Bitget trade history.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitgetTradeHistory {
    pub order_id: String,
    pub trade_id: String,
    pub symbol: String,
    pub side: String,
    pub price: String,
    pub size: String,
    pub fee: String,
    pub fee_coin: String,
    pub timestamp: String,
}

/// Bitget WebSocket subscription message.
#[derive(Debug, Serialize)]
pub struct BitgetWsSubscribe {
    pub op: String,
    pub args: Vec<BitgetWsChannel>,
}

/// Bitget WebSocket channel.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitgetWsChannel {
    pub inst_type: String,
    pub channel: String,
    pub inst_id: String,
}

/// Bitget WebSocket message types.
#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum BitgetWsMessage {
    #[serde(rename = "snapshot")]
    Snapshot { arg: BitgetWsChannel, data: serde_json::Value },
    #[serde(rename = "update")]
    Update { arg: BitgetWsChannel, data: serde_json::Value },
}

/// Bitget order book entry.
#[derive(Debug, Deserialize)]
pub struct BitgetOrderBookEntry {
    pub price: String,
    pub size: String,
}

/// Bitget order book data.
#[derive(Debug, Deserialize)]
pub struct BitgetOrderBook {
    pub asks: Vec<BitgetOrderBookEntry>,
    pub bids: Vec<BitgetOrderBookEntry>,
    pub timestamp: String,
}

/// Bitget trade data.
#[derive(Debug, Deserialize)]
pub struct BitgetTrade {
    pub trade_id: String,
    pub price: String,
    pub size: String,
    pub side: String,
    pub timestamp: String,
}

/// Bitget account update.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitgetAccountUpdate {
    pub coin_id: String,
    pub available: String,
    pub frozen: String,
    pub lock: String,
    pub utime: String,
}

/// Bitget order update.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitgetOrderUpdate {
    pub inst_id: String,
    pub order_id: String,
    pub client_oid: String,
    pub side: String,
    pub order_type: String,
    pub price: String,
    pub size: String,
    pub filled_qty: String,
    pub status: String,
    pub utime: String,
}