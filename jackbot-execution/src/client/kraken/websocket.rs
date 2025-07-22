use crate::{
    balance::{AssetBalance, Balance},
    error::{UnindexedClientError, UnindexedApiError},
    order::{
        id::{ClientOrderId, OrderId, StrategyId},
        state::{ActiveOrderState, Cancelled, InactiveOrderState, Open, OrderState},
        Order, OrderKey, OrderKind, TimeInForce,
    },
    trade::{AssetFees, Trade, TradeId},
    AccountEventKind, UnindexedAccountEvent,
};
use chrono::{DateTime, TimeZone, Utc};
use futures::{SinkExt, StreamExt};
use jackbot_instrument::{
    asset::{name::AssetNameExchange, QuoteAsset},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
    Side,
};
use jackbot_integration::snapshot::Snapshot;
use jackbot_integration::{
    circuit_breaker::CircuitBreaker,
    protocol::websocket::{connect, WebSocket},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, info, warn};
use url::Url;

const KRAKEN_WS_PUBLIC: &str = "wss://ws.kraken.com/";
const KRAKEN_WS_PRIVATE: &str = "wss://ws-auth.kraken.com/";

#[derive(Clone, Debug)]
pub struct KrakenWsConfig {
    pub public_url: Url,
    pub private_url: Url,
    pub auth_token: Option<String>, // Token from GetWebSocketsToken REST endpoint
}

impl Default for KrakenWsConfig {
    fn default() -> Self {
        Self {
            public_url: Url::parse(KRAKEN_WS_PUBLIC).unwrap(),
            private_url: Url::parse(KRAKEN_WS_PRIVATE).unwrap(),
            auth_token: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct KrakenWsClient {
    config: KrakenWsConfig,
}

impl KrakenWsClient {
    pub fn new(config: KrakenWsConfig) -> Self {
        Self { config }
    }

    pub async fn account_stream(
        &self,
        _assets: &[AssetNameExchange],
        _instruments: &[InstrumentNameExchange],
    ) -> Result<UnboundedReceiverStream<UnindexedAccountEvent>, UnindexedClientError> {
        let (tx, rx) = mpsc::unbounded_channel();
        
        if self.config.auth_token.is_none() {
            return Err(UnindexedClientError::Api(UnindexedApiError::OrderRejected(
                "Authentication token required for private WebSocket feeds".to_string()
            )));
        }

        let url = self.config.private_url.clone();
        let auth_token = self.config.auth_token.clone().unwrap();
        
        tokio::spawn(async move {
            let mut breaker = CircuitBreaker::new(5, std::time::Duration::from_secs(5));
            
            loop {
                if breaker.is_open() {
                    if let Some(wait) = breaker.remaining() {
                        warn!(?wait, "Circuit breaker open, waiting before reconnect");
                        tokio::time::sleep(wait).await;
                        continue;
                    }
                }

                match connect(url.clone()).await {
                    Ok(ws) => {
                        breaker.reset();
                        info!("Connected to Kraken WebSocket");
                        
                        match run_private_connection(ws, &tx, &auth_token).await {
                            Ok(_) => {
                                info!("WebSocket connection ended normally");
                                break;
                            }
                            Err(err) => {
                                error!(?err, "WebSocket connection failed");
                                breaker.record_failure();
                                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                            }
                        }
                    }
                    Err(err) => {
                        breaker.record_failure();
                        warn!(?err, "Failed to connect to Kraken WebSocket");
                        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    }
                }
            }
        });

        Ok(UnboundedReceiverStream::new(rx))
    }

    pub async fn subscribe_public_feeds(
        &self,
        instruments: &[InstrumentNameExchange],
    ) -> Result<UnboundedReceiverStream<KrakenPublicEvent>, UnindexedClientError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let url = self.config.public_url.clone();
        let pairs: Vec<String> = instruments.iter()
            .map(|i| standard_symbol_to_kraken(i.as_str()))
            .collect();

        tokio::spawn(async move {
            let mut breaker = CircuitBreaker::new(5, std::time::Duration::from_secs(5));
            
            loop {
                if breaker.is_open() {
                    if let Some(wait) = breaker.remaining() {
                        warn!(?wait, "Circuit breaker open, waiting before reconnect");
                        tokio::time::sleep(wait).await;
                        continue;
                    }
                }

                match connect(url.clone()).await {
                    Ok(ws) => {
                        breaker.reset();
                        info!("Connected to Kraken public WebSocket");
                        
                        match run_public_connection(ws, &tx, &pairs).await {
                            Ok(_) => {
                                info!("Public WebSocket connection ended normally");
                                break;
                            }
                            Err(err) => {
                                error!(?err, "Public WebSocket connection failed");
                                breaker.record_failure();
                                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                            }
                        }
                    }
                    Err(err) => {
                        breaker.record_failure();
                        warn!(?err, "Failed to connect to Kraken public WebSocket");
                        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    }
                }
            }
        });

        Ok(UnboundedReceiverStream::new(rx))
    }
}

async fn run_private_connection(
    mut ws: WebSocket,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
    auth_token: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Subscribe to private feeds
    let subscriptions = vec![
        KrakenSubscription::new("ownTrades", None, Some(auth_token.to_string())),
        KrakenSubscription::new("openOrders", None, Some(auth_token.to_string())),
    ];

    for subscription in subscriptions {
        let sub_msg = serde_json::to_string(&subscription)?;
        ws.send(WsMessage::Text(sub_msg)).await?;
        debug!("Sent subscription: {:?}", subscription);
    }

    loop {
        match ws.next().await {
            Some(Ok(WsMessage::Text(text))) => {
                debug!("Received message: {}", text);
                
                if let Ok(event) = serde_json::from_str::<KrakenPrivateEvent>(&text) {
                    if let Some(account_event) = process_private_event(event) {
                        if tx.send(account_event).is_err() {
                            warn!("Failed to send account event - receiver closed");
                            break;
                        }
                    }
                } else if let Ok(status) = serde_json::from_str::<KrakenStatusMessage>(&text) {
                    info!("Status: {:?}", status);
                } else {
                    debug!("Unhandled message: {}", text);
                }
            }
            Some(Ok(WsMessage::Close(_))) => {
                warn!("Received close frame from server");
                break;
            }
            Some(Ok(WsMessage::Pong(_))) => {
                debug!("Received pong");
            }
            Some(Ok(_)) => {
                // Other message types
            }
            Some(Err(err)) => {
                error!(?err, "WebSocket stream error");
                return Err(err.into());
            }
            None => {
                warn!("WebSocket stream ended");
                break;
            }
        }
    }

    Ok(())
}

async fn run_public_connection(
    mut ws: WebSocket,
    tx: &mpsc::UnboundedSender<KrakenPublicEvent>,
    pairs: &[String],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Subscribe to public feeds
    let subscriptions = vec![
        KrakenSubscription::new("ticker", Some(pairs.to_vec()), None),
        KrakenSubscription::new("book", Some(pairs.to_vec()), None),
        KrakenSubscription::new("trade", Some(pairs.to_vec()), None),
    ];

    for subscription in subscriptions {
        let sub_msg = serde_json::to_string(&subscription)?;
        ws.send(WsMessage::Text(sub_msg)).await?;
        debug!("Sent public subscription: {:?}", subscription);
    }

    loop {
        match ws.next().await {
            Some(Ok(WsMessage::Text(text))) => {
                debug!("Received public message: {}", text);
                
                if let Ok(event) = serde_json::from_str::<KrakenPublicEvent>(&text) {
                    if tx.send(event).is_err() {
                        warn!("Failed to send public event - receiver closed");
                        break;
                    }
                } else if let Ok(status) = serde_json::from_str::<KrakenStatusMessage>(&text) {
                    info!("Public status: {:?}", status);
                } else {
                    debug!("Unhandled public message: {}", text);
                }
            }
            Some(Ok(WsMessage::Close(_))) => {
                warn!("Received close frame from public server");
                break;
            }
            Some(Ok(WsMessage::Pong(_))) => {
                debug!("Received public pong");
            }
            Some(Ok(_)) => {
                // Other message types
            }
            Some(Err(err)) => {
                error!(?err, "Public WebSocket stream error");
                return Err(err.into());
            }
            None => {
                warn!("Public WebSocket stream ended");
                break;
            }
        }
    }

    Ok(())
}

fn process_private_event(event: KrakenPrivateEvent) -> Option<UnindexedAccountEvent> {
    match event {
        KrakenPrivateEvent::OwnTrades { data } => {
            // Process trade events
            for (trade_id, trade_info) in data {
                if let Some(trade) = parse_kraken_trade(&trade_id, trade_info) {
                    return Some(UnindexedAccountEvent::new(
                        ExchangeId::Kraken,
                        AccountEventKind::Trade(trade),
                    ));
                }
            }
            None
        }
        KrakenPrivateEvent::OpenOrders { data } => {
            // Process order events
            for (order_id, order_info) in data {
                if let Some(order) = parse_kraken_order(&order_id, order_info) {
                    return Some(UnindexedAccountEvent::new(
                        ExchangeId::Kraken,
                        AccountEventKind::OrderSnapshot(Snapshot::new(order)),
                    ));
                }
            }
            None
        }
    }
}

fn parse_kraken_trade(trade_id: &str, trade_info: KrakenWsTradeInfo) -> Option<Trade<QuoteAsset, InstrumentNameExchange>> {
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
        strategy: StrategyId::unknown(),
        time_exchange: time,
        side,
        price,
        quantity,
        fees: AssetFees::quote_fees(fee),
    })
}

fn parse_kraken_order(order_id: &str, order_info: KrakenWsOrderInfo) -> Option<Order<ExchangeId, InstrumentNameExchange, OrderState<AssetNameExchange, InstrumentNameExchange>>> {
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

    let order_state = match order_info.status.as_str() {
        "open" | "pending" => OrderState::Active(ActiveOrderState::Open(
            Open::new(OrderId::new(order_id), time, filled)
        )),
        "closed" => OrderState::Inactive(InactiveOrderState::FullyFilled),
        "canceled" | "expired" => OrderState::Inactive(InactiveOrderState::Cancelled(
            Cancelled::new(OrderId::new(order_id), time)
        )),
        _ => return None,
    };

    Some(Order {
        key: OrderKey {
            exchange: ExchangeId::Kraken,
            instrument: InstrumentNameExchange::new(kraken_symbol_to_standard(&order_info.descr.pair)),
            strategy: StrategyId::unknown(),
            cid: order_info.userref.map(|id| ClientOrderId::new(&id.to_string())).unwrap_or_default(),
        },
        side,
        price,
        quantity,
        kind,
        time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
        state: order_state,
    })
}

// Helper functions for symbol conversion (same as in rest.rs)
fn standard_symbol_to_kraken(symbol: &str) -> String {
    match symbol {
        "BTCUSD" => "XBT/USD".to_string(),
        "ETHUSD" => "ETH/USD".to_string(),
        "BTCEUR" => "XBT/EUR".to_string(),
        "ETHEUR" => "ETH/EUR".to_string(),
        _ => symbol.replace("USD", "/USD").replace("EUR", "/EUR"),
    }
}

fn kraken_symbol_to_standard(kraken_symbol: &str) -> String {
    match kraken_symbol {
        "XBT/USD" => "BTCUSD".to_string(),
        "XBT/EUR" => "BTCEUR".to_string(),
        "ETH/USD" => "ETHUSD".to_string(),
        "ETH/EUR" => "ETHEUR".to_string(),
        _ => kraken_symbol.replace('/', ""),
    }
}

// WebSocket message types
#[derive(Debug, Serialize)]
struct KrakenSubscription {
    event: String,
    pair: Option<Vec<String>>,
    subscription: KrakenSubscriptionDetails,
}

impl KrakenSubscription {
    fn new(name: &str, pair: Option<Vec<String>>, token: Option<String>) -> Self {
        Self {
            event: "subscribe".to_string(),
            pair,
            subscription: KrakenSubscriptionDetails {
                name: name.to_string(),
                token,
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct KrakenSubscriptionDetails {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KrakenStatusMessage {
    event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pair: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum KrakenPrivateEvent {
    OwnTrades {
        #[serde(flatten)]
        data: HashMap<String, KrakenWsTradeInfo>,
    },
    OpenOrders {
        #[serde(flatten)]
        data: HashMap<String, KrakenWsOrderInfo>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum KrakenPublicEvent {
    Ticker {
        #[serde(rename = "1")]
        ticker_data: HashMap<String, KrakenTickerData>,
    },
    Trade {
        #[serde(rename = "1")]
        trade_data: Vec<KrakenPublicTradeData>,
    },
    Book {
        #[serde(rename = "1")]
        book_data: KrakenOrderBookData,
    },
}

#[derive(Debug, Deserialize)]
struct KrakenWsTradeInfo {
    ordertxid: String,
    pair: String,
    time: f64,
    #[serde(rename = "type")]
    type_: String,
    price: String,
    vol: String,
    fee: String,
}

#[derive(Debug, Deserialize)]
struct KrakenWsOrderInfo {
    descr: KrakenWsOrderDescr,
    vol: String,
    vol_exec: String,
    opentm: f64,
    status: String,
    userref: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct KrakenWsOrderDescr {
    pair: String,
    #[serde(rename = "type")]
    type_: String,
    ordertype: String,
    price: String,
}

#[derive(Debug, Deserialize)]
pub struct KrakenTickerData {
    pub a: Vec<String>, // Ask [price, whole_lot_volume, lot_volume]
    pub b: Vec<String>, // Bid [price, whole_lot_volume, lot_volume]
    pub c: Vec<String>, // Last trade closed [price, lot_volume]
    pub v: Vec<String>, // Volume [today, last_24_hours]
    pub p: Vec<String>, // Volume weighted average price [today, last_24_hours]
    pub t: Vec<u64>,    // Number of trades [today, last_24_hours]
    pub l: Vec<String>, // Low [today, last_24_hours]
    pub h: Vec<String>, // High [today, last_24_hours]
    pub o: Vec<String>, // Opening price [today, last_24_hours]
}

#[derive(Debug, Deserialize)]
pub struct KrakenPublicTradeData {
    pub price: String,
    pub volume: String,
    pub time: f64,
    pub side: String, // "b" for buy, "s" for sell
    pub order_type: String, // "l" for limit, "m" for market
}

#[derive(Debug, Deserialize)]
pub struct KrakenOrderBookData {
    pub bids: Vec<Vec<String>>, // [price, volume, timestamp]
    pub asks: Vec<Vec<String>>, // [price, volume, timestamp]
}