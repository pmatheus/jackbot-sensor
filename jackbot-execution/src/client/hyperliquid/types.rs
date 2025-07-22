//! Hyperliquid API types and configuration.

use serde::{Deserialize, Serialize};
use url::Url;

/// Hyperliquid client configuration.
#[derive(Clone, Debug)]
pub struct HyperliquidConfig {
    /// REST API base URL.
    pub rest_url: Url,
    /// WebSocket base URL.
    pub ws_url: Url,
    /// Web3 RPC URL (for on-chain interactions).
    pub web3_rpc_url: Url,
    /// Private key for signing transactions (hex encoded).
    pub private_key: String,
    /// Account address.
    pub account_address: String,
    /// API key for REST/WebSocket authentication.
    pub api_key: Option<String>,
    /// Chain ID (Arbitrum = 42161).
    pub chain_id: u64,
}

impl HyperliquidConfig {
    /// Create a new Hyperliquid configuration.
    pub fn new(
        private_key: String,
        account_address: String,
        api_key: Option<String>,
    ) -> Self {
        Self {
            rest_url: Url::parse("https://api.hyperliquid.xyz").expect("valid URL"),
            ws_url: Url::parse("wss://api.hyperliquid.xyz/ws").expect("valid URL"),
            web3_rpc_url: Url::parse("https://arb1.arbitrum.io/rpc").expect("valid URL"),
            private_key,
            account_address,
            api_key,
            chain_id: 42161, // Arbitrum mainnet
        }
    }

    /// Create a configuration for testnet.
    pub fn new_testnet(
        private_key: String,
        account_address: String,
        api_key: Option<String>,
    ) -> Self {
        Self {
            rest_url: Url::parse("https://api.hyperliquid-testnet.xyz").expect("valid URL"),
            ws_url: Url::parse("wss://api.hyperliquid-testnet.xyz/ws").expect("valid URL"),
            web3_rpc_url: Url::parse("https://arb-goerli.g.alchemy.com/v2/demo").expect("valid URL"),
            private_key,
            account_address,
            api_key,
            chain_id: 421613, // Arbitrum Goerli testnet
        }
    }
}

/// Hyperliquid API response wrapper.
#[derive(Debug, Deserialize)]
pub struct HyperliquidResponse<T> {
    pub status: String,
    pub data: Option<T>,
    pub error: Option<String>,
}

/// Hyperliquid account state.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperliquidAccountState {
    pub user: String,
    pub asset_positions: Vec<AssetPosition>,
    pub margin_summary: MarginSummary,
    pub cross_margin_summary: CrossMarginSummary,
}

/// Asset position on Hyperliquid.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetPosition {
    pub position: Position,
    pub funding: Funding,
}

/// Position details.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub coin: String,
    pub szi: String, // Size (signed, negative for short)
    pub entry_px: Option<String>,
    pub pos_value: String,
    pub unrealized_pnl: String,
    pub realized_pnl: String,
}

/// Funding details.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Funding {
    pub since_change: String,
    pub since_open: String,
}

/// Margin summary.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginSummary {
    pub account_value: String,
    pub total_ntl_pos: String,
    pub total_raw_usd: String,
    pub total_margin_used: String,
    pub available_margin: String,
}

/// Cross margin summary.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossMarginSummary {
    pub account_value: String,
    pub total_ntl_pos: String,
    pub total_raw_usd: String,
    pub total_margin_used: String,
    pub available_margin: String,
}

/// Hyperliquid order request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperliquidOrderRequest {
    pub coin: String,
    pub is_buy: bool,
    pub sz: String,
    pub limit_px: Option<String>,
    pub order_type: OrderType,
    pub reduce_only: bool,
    pub ioc: bool,
    pub post_only: bool,
    pub client_order_id: u64,
}

/// Order type for Hyperliquid.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OrderType {
    Limit,
    Market,
    Stop,
    StopLimit,
}

/// Hyperliquid order response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperliquidOrderResponse {
    pub status: String,
    pub response: OrderResponseData,
}

/// Order response data.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponseData {
    pub type_: String,
    pub data: Option<OrderStatusData>,
}

/// Order status data.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderStatusData {
    pub statuses: Vec<OrderStatus>,
}

/// Order status.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderStatus {
    pub oid: String,
    pub cloid: Option<String>,
}

/// Hyperliquid order info.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperliquidOrderInfo {
    pub oid: String,
    pub coin: String,
    pub side: String,
    pub order_type: String,
    pub sz: String,
    pub limit_px: String,
    pub timestamp: i64,
    pub orig_sz: String,
    pub cloid: Option<String>,
}

/// Hyperliquid fill/trade.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperliquidFill {
    pub coin: String,
    pub px: String,
    pub sz: String,
    pub side: String,
    pub time: i64,
    pub start_position: String,
    pub dir: String,
    pub closed_pnl: String,
    pub hash: String,
    pub oid: String,
    pub crossed: bool,
    pub fee: String,
    pub tid: u64,
}

/// Hyperliquid WebSocket subscription.
#[derive(Debug, Serialize)]
pub struct HyperliquidWsSubscribe {
    pub method: String,
    pub subscription: HyperliquidSubscription,
}

/// Subscription details.
#[derive(Debug, Serialize)]
pub struct HyperliquidSubscription {
    #[serde(rename = "type")]
    pub sub_type: String,
    pub user: Option<String>,
    pub coin: Option<String>,
}

/// Hyperliquid WebSocket message.
#[derive(Debug, Deserialize)]
#[serde(tag = "channel")]
pub enum HyperliquidWsMessage {
    #[serde(rename = "allMids")]
    AllMids { data: serde_json::Value },
    #[serde(rename = "notification")]
    Notification { data: NotificationData },
    #[serde(rename = "webData2")]
    WebData2 { data: WebData2 },
    #[serde(rename = "user")]
    User { data: UserData },
    #[serde(rename = "orderUpdates")]
    OrderUpdates { data: Vec<OrderUpdate> },
    #[serde(rename = "userFills")]
    UserFills { data: Vec<HyperliquidFill> },
    #[serde(rename = "userFundings")]
    UserFundings { data: Vec<FundingUpdate> },
    #[serde(rename = "userNonFundingLedgerUpdates")]
    UserNonFundingLedgerUpdates { data: Vec<LedgerUpdate> },
}

/// Notification data.
#[derive(Debug, Deserialize)]
pub struct NotificationData {
    pub notification: String,
}

/// WebData2 structure.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebData2 {
    pub user_state: Option<HyperliquidAccountState>,
    pub open_orders: Option<Vec<HyperliquidOrderInfo>>,
}

/// User data.
#[derive(Debug, Deserialize)]
pub struct UserData {
    pub user: String,
    pub data: serde_json::Value,
}

/// Order update.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderUpdate {
    pub order: HyperliquidOrderInfo,
    pub status: String,
    pub status_timestamp: i64,
}

/// Funding update.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingUpdate {
    pub time: i64,
    pub coin: String,
    pub usdc: String,
    pub szi: String,
    pub funding_rate: String,
}

/// Ledger update.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerUpdate {
    pub time: i64,
    pub coin: String,
    pub usdc: String,
    pub hash: String,
    pub delta: LedgerDelta,
}

/// Ledger delta.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerDelta {
    #[serde(rename = "type")]
    pub delta_type: String,
    pub amount: String,
}

/// L2 order book snapshot/update.
#[derive(Debug, Deserialize)]
pub struct HyperliquidL2Book {
    pub coin: String,
    pub time: i64,
    pub levels: Vec<Vec<L2Level>>,
}

/// L2 price level.
#[derive(Debug, Deserialize)]
pub struct L2Level {
    pub px: String,
    pub sz: String,
    pub n: u32, // Number of orders
}

/// Liquidation event.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperliquidLiquidation {
    pub liquidation_id: String,
    pub account: String,
    pub coin: String,
    pub side: String,
    pub px: String,
    pub sz: String,
    pub time: i64,
    pub liquidation_type: String,
}