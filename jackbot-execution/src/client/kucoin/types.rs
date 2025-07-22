//! KuCoin API types and configuration.

use serde::{Deserialize, Serialize};
use url::Url;

/// KuCoin client configuration.
#[derive(Clone, Debug)]
pub struct KuCoinConfig {
    /// REST API base URL.
    pub rest_url: Url,
    /// WebSocket base URL (obtained dynamically).
    pub ws_url: Option<Url>,
    /// API key.
    pub api_key: String,
    /// API secret.
    pub api_secret: String,
    /// API passphrase.
    pub passphrase: String,
    /// API version.
    pub api_version: String,
}

impl KuCoinConfig {
    /// Create a new KuCoin configuration.
    pub fn new(api_key: String, api_secret: String, passphrase: String) -> Self {
        Self {
            rest_url: Url::parse("https://api.kucoin.com").expect("valid URL"),
            ws_url: None, // Will be fetched dynamically
            api_key,
            api_secret,
            passphrase,
            api_version: "2".to_string(),
        }
    }

    /// Create a sandbox configuration for testing.
    pub fn new_sandbox(api_key: String, api_secret: String, passphrase: String) -> Self {
        Self {
            rest_url: Url::parse("https://openapi-sandbox.kucoin.com").expect("valid URL"),
            ws_url: None,
            api_key,
            api_secret,
            passphrase,
            api_version: "2".to_string(),
        }
    }
}

/// KuCoin API response wrapper.
#[derive(Debug, Deserialize)]
pub struct KuCoinResponse<T> {
    pub code: String,
    pub msg: Option<String>,
    pub data: Option<T>,
}

/// KuCoin paginated response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinPaginatedResponse<T> {
    pub current_page: i32,
    pub page_size: i32,
    pub total_num: i64,
    pub total_page: i32,
    pub items: Vec<T>,
}

/// KuCoin account balance.
#[derive(Debug, Deserialize)]
pub struct KuCoinBalance {
    pub id: String,
    pub currency: String,
    pub r#type: String,
    pub balance: String,
    pub available: String,
    pub holds: String,
}

/// KuCoin order request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinOrderRequest {
    pub client_oid: String,
    pub side: String,
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub funds: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iceberg: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible_size: Option<String>,
}

/// KuCoin order response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinOrderResponse {
    pub order_id: String,
}

/// KuCoin order info.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinOrderInfo {
    pub id: String,
    pub symbol: String,
    pub op_type: String,
    pub r#type: String,
    pub side: String,
    pub price: String,
    pub size: String,
    pub funds: String,
    pub deal_funds: String,
    pub deal_size: String,
    pub fee: String,
    pub fee_currency: String,
    pub stop: String,
    pub stop_triggered: bool,
    pub stop_price: String,
    pub time_in_force: String,
    pub post_only: bool,
    pub hidden: bool,
    pub iceberg: bool,
    pub visible_size: String,
    pub cancel_after: i64,
    pub channel: String,
    pub client_oid: String,
    pub remark: Option<String>,
    pub tags: Option<String>,
    pub is_active: bool,
    pub cancel_exist: bool,
    pub created_at: i64,
    pub trade_type: String,
}

/// KuCoin trade fill.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinFill {
    pub id: String,
    pub symbol: String,
    pub trade_id: String,
    pub order_id: String,
    pub counter_order_id: String,
    pub side: String,
    pub liquidity: String,
    pub price: String,
    pub size: String,
    pub funds: String,
    pub fee: String,
    pub fee_rate: String,
    pub fee_currency: String,
    pub stop: String,
    pub r#type: String,
    pub created_at: i64,
    pub trade_type: String,
}

/// WebSocket connection info.
#[derive(Debug, Deserialize)]
pub struct KuCoinWsConnection {
    pub token: String,
    #[serde(rename = "instanceServers")]
    pub instance_servers: Vec<KuCoinWsServer>,
}

/// WebSocket server info.
#[derive(Debug, Deserialize)]
pub struct KuCoinWsServer {
    pub endpoint: String,
    pub protocol: String,
    pub encrypt: bool,
    #[serde(rename = "pingInterval")]
    pub ping_interval: i64,
    #[serde(rename = "pingTimeout")]
    pub ping_timeout: i64,
}

/// WebSocket subscription message.
#[derive(Debug, Serialize)]
pub struct KuCoinWsSubscribe {
    pub id: String,
    pub r#type: String,
    pub topic: String,
    #[serde(rename = "privateChannel")]
    pub private_channel: bool,
    pub response: bool,
}

/// WebSocket message wrapper.
#[derive(Debug, Deserialize)]
pub struct KuCoinWsMessage {
    pub id: Option<String>,
    pub r#type: String,
    pub topic: Option<String>,
    pub subject: Option<String>,
    pub data: Option<serde_json::Value>,
}

/// WebSocket account balance update.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinAccountUpdate {
    pub account_id: String,
    pub currency: String,
    pub total: String,
    pub available: String,
    pub hold: String,
    pub relation_event: String,
    pub relation_event_id: String,
    pub time: String,
}

/// WebSocket order change event.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinOrderChange {
    pub symbol: String,
    pub order_type: String,
    pub side: String,
    pub order_id: String,
    pub r#type: String,
    pub order_time: i64,
    pub size: String,
    pub filled_size: String,
    pub price: String,
    pub client_oid: String,
    pub remain_size: String,
    pub status: String,
    pub ts: i64,
}

/// Sub-account info.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinSubAccount {
    pub user_id: String,
    pub sub_name: String,
    pub r#type: i32,
    pub created_at: i64,
    pub remarks: Option<String>,
}

/// Sub-account creation request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinSubAccountCreate {
    pub sub_name: String,
    pub password: String,
    pub remarks: Option<String>,
}

/// Transfer request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinTransferRequest {
    pub currency: String,
    pub amount: String,
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_user_id: Option<String>,
}

/// Transfer response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinTransferResponse {
    pub order_id: String,
}

/// Margin account info.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinMarginAccount {
    pub debt_ratio: String,
    pub total_asset_of_quote: String,
    pub total_liability_of_quote: String,
    pub accounts: Vec<KuCoinMarginAsset>,
}

/// Margin asset info.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinMarginAsset {
    pub currency: String,
    pub total_balance: String,
    pub available: String,
    pub hold: String,
    pub borrowed: String,
    pub max_borrow_size: String,
    pub borrowed_amount: String,
    pub locked_amount: String,
    pub accrued_interest: String,
}

/// Borrow request.
#[derive(Debug, Serialize)]
pub struct KuCoinBorrowRequest {
    pub currency: String,
    pub size: String,
    pub r#type: String, // "FOK" or "IOC"
}

/// Borrow response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinBorrowResponse {
    pub order_id: String,
    pub currency: String,
    pub actual_size: String,
}

/// Repay request.
#[derive(Debug, Serialize)]
pub struct KuCoinRepayRequest {
    pub currency: String,
    pub size: String,
}

/// Repay response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinRepayResponse {
    pub order_id: String,
    pub currency: String,
    pub actual_size: String,
}

/// Borrow record.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinBorrowRecord {
    pub order_id: String,
    pub currency: String,
    pub size: String,
    pub actual_size: String,
    pub status: String,
    pub created_at: i64,
}

/// WebSocket Level 2 order book update.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinL2Update {
    pub symbol: String,
    pub changes: KuCoinL2Changes,
    pub sequence_start: i64,
    pub sequence_end: i64,
}

/// Order book changes.
#[derive(Debug, Deserialize)]
pub struct KuCoinL2Changes {
    pub asks: Vec<[String; 3]>, // [price, size, sequence]
    pub bids: Vec<[String; 3]>, // [price, size, sequence]
}

/// WebSocket ticker update.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinTickerUpdate {
    pub symbol: String,
    pub best_ask: String,
    pub best_ask_size: String,
    pub best_bid: String,
    pub best_bid_size: String,
    pub change: String,
    pub change_rate: String,
    pub high: String,
    pub last: String,
    pub low: String,
    pub open: String,
    pub quote_volume: String,
    pub volume: String,
    pub time: i64,
}

/// WebSocket trade update.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuCoinTradeUpdate {
    pub symbol: String,
    pub sequence: String,
    pub side: String,
    pub size: String,
    pub price: String,
    pub time: i64,
}