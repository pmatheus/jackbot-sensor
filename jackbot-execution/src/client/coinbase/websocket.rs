use crate::error::UnindexedClientError;
use chrono::{DateTime, TimeZone, Utc};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use jackbot_data::{
    books::{OrderBook, Level},
    event::{MarketEvent, DataKind},
    subscription::{
        book::OrderBookEvent,
        trade::PublicTrade,
    },
};
use jackbot_instrument::{
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
    Side,
};
use jackbot_integration::{
    circuit_breaker::CircuitBreaker,
    protocol::websocket::{connect, WebSocket},
};
use rust_decimal::{Decimal, prelude::FromStr};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, info, warn};
use url::Url;

const COINBASE_WS: &str = "wss://ws-feed.exchange.coinbase.com";
const COINBASE_SANDBOX_WS: &str = "wss://ws-feed-public.sandbox.exchange.coinbase.com";
const COINBASE_AUTH_WS: &str = "wss://ws-feed.exchange.coinbase.com";
const COINBASE_SANDBOX_AUTH_WS: &str = "wss://ws-feed-public.sandbox.exchange.coinbase.com";

#[derive(Clone)]
pub struct CoinbaseWsManager {
    url: Url,
    auth_url: Url,
    connections: Arc<RwLock<HashMap<String, WebSocket>>>,
    auth_credentials: Option<CoinbaseAuth>,
}

#[derive(Clone)]
struct CoinbaseAuth {
    api_key: String,
    api_secret: String,
    api_passphrase: String,
}

impl CoinbaseWsManager {
    pub fn new(sandbox: bool) -> Self {
        let url = if sandbox {
            Url::parse(COINBASE_SANDBOX_WS).expect("Valid URL")
        } else {
            Url::parse(COINBASE_WS).expect("Valid URL")
        };
        
        let auth_url = if sandbox {
            Url::parse(COINBASE_SANDBOX_AUTH_WS).expect("Valid URL")
        } else {
            Url::parse(COINBASE_AUTH_WS).expect("Valid URL")
        };

        Self {
            url,
            auth_url,
            connections: Arc::new(RwLock::new(HashMap::new())),
            auth_credentials: None,
        }
    }
    
    /// Create a new manager with authentication credentials
    pub fn with_auth(sandbox: bool, api_key: String, api_secret: String, api_passphrase: String) -> Self {
        let mut manager = Self::new(sandbox);
        manager.auth_credentials = Some(CoinbaseAuth {
            api_key,
            api_secret,
            api_passphrase,
        });
        manager
    }

    /// Subscribe to order book updates (level2)
    pub async fn subscribe_order_book(
        &self,
        product_ids: Vec<String>,
    ) -> Result<mpsc::UnboundedReceiver<MarketEvent<InstrumentNameExchange, DataKind>>, UnindexedClientError> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.spawn_connection(
            product_ids,
            vec!["level2_batch".to_string()],
            tx,
            ConnectionType::OrderBook,
        ).await?;
        Ok(rx)
    }

    /// Subscribe to trade updates
    pub async fn subscribe_trades(
        &self,
        product_ids: Vec<String>,
    ) -> Result<mpsc::UnboundedReceiver<MarketEvent<InstrumentNameExchange, DataKind>>, UnindexedClientError> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.spawn_connection(
            product_ids,
            vec!["matches".to_string()],
            tx,
            ConnectionType::Trades,
        ).await?;
        Ok(rx)
    }

    /// Subscribe to multiple channels
    pub async fn subscribe_combined(
        &self,
        product_ids: Vec<String>,
        channels: Vec<String>,
    ) -> Result<mpsc::UnboundedReceiver<MarketEvent<InstrumentNameExchange, DataKind>>, UnindexedClientError> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.spawn_connection(product_ids, channels, tx, ConnectionType::Combined).await?;
        Ok(rx)
    }
    
    /// Subscribe to authenticated user data streams
    pub async fn subscribe_user_data(
        &self,
        product_ids: Vec<String>,
    ) -> Result<mpsc::UnboundedReceiver<UserDataEvent>, UnindexedClientError> {
        if self.auth_credentials.is_none() {
            return Err(UnindexedClientError::AccountStream(
                "Authentication credentials not provided".to_string()
            ));
        }
        
        let (tx, rx) = mpsc::unbounded_channel();
        self.spawn_auth_connection(product_ids, tx).await?;
        Ok(rx)
    }

    async fn spawn_connection(
        &self,
        product_ids: Vec<String>,
        channels: Vec<String>,
        tx: mpsc::UnboundedSender<MarketEvent<InstrumentNameExchange, DataKind>>,
        conn_type: ConnectionType,
    ) -> Result<(), UnindexedClientError> {
        let connections = self.connections.clone();
        let url = self.url.clone();
        let connection_id = format!("{:?}-{:?}", product_ids, channels);

        tokio::spawn(async move {
            let mut breaker = CircuitBreaker::new(5, Duration::from_secs(30));
            
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
                        info!("Connected to Coinbase WebSocket");
                        
                        // Store connection
                        {
                            let mut conns = connections.write().await;
                            conns.insert(connection_id.clone(), ws);
                        }

                        // Subscribe to channels
                        let subscribe_msg = SubscribeMessage {
                            type_: "subscribe".to_string(),
                            product_ids: product_ids.clone(),
                            channels: channels.clone(),
                        };

                        if let Ok(msg) = serde_json::to_string(&subscribe_msg) {
                            let mut conns = connections.write().await;
                            if let Some(ws) = conns.get_mut(&connection_id) {
                                if ws.send(WsMessage::Text(msg.into())).await.is_err() {
                                    error!("Failed to send subscribe message");
                                    continue;
                                }
                            }
                        }

                        // Run the connection handler
                        let run_result = run_market_connection(
                            &connections,
                            &connection_id,
                            &tx,
                            conn_type,
                        ).await;
                        
                        if run_result.is_err() {
                            breaker.record_failure();
                            warn!("WebSocket connection failed, will retry");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                    Err(e) => {
                        breaker.record_failure();
                        error!("Failed to connect to WebSocket: {}", e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Ok(())
    }
    
    async fn spawn_auth_connection(
        &self,
        product_ids: Vec<String>,
        tx: mpsc::UnboundedSender<UserDataEvent>,
    ) -> Result<(), UnindexedClientError> {
        let connections = self.connections.clone();
        let url = self.auth_url.clone();
        let connection_id = format!("auth-{:?}", product_ids);
        let auth = self.auth_credentials.clone();
        
        tokio::spawn(async move {
            let mut breaker = CircuitBreaker::new(5, Duration::from_secs(30));
            
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
                        info!("Connected to Coinbase authenticated WebSocket");
                        
                        // Store connection
                        {
                            let mut conns = connections.write().await;
                            conns.insert(connection_id.clone(), ws);
                        }

                        // Send authentication message
                        if let Some(auth_creds) = &auth {
                            let auth_msg = create_auth_message(auth_creds, &product_ids);
                            let mut conns = connections.write().await;
                            if let Some(ws) = conns.get_mut(&connection_id) {
                                if ws.send(WsMessage::Text(auth_msg.into())).await.is_err() {
                                    error!("Failed to send auth message");
                                    continue;
                                }
                            }
                        }

                        // Subscribe to user channels
                        let subscribe_msg = SubscribeMessage {
                            type_: "subscribe".to_string(),
                            product_ids: product_ids.clone(),
                            channels: vec![
                                "user".to_string(),
                                "full".to_string(),
                            ],
                        };

                        if let Ok(msg) = serde_json::to_string(&subscribe_msg) {
                            let mut conns = connections.write().await;
                            if let Some(ws) = conns.get_mut(&connection_id) {
                                if ws.send(WsMessage::Text(msg.into())).await.is_err() {
                                    error!("Failed to send subscribe message");
                                    continue;
                                }
                            }
                        }

                        // Run the connection handler
                        let run_result = run_auth_connection(
                            &connections,
                            &connection_id,
                            &tx,
                        ).await;
                        
                        if run_result.is_err() {
                            breaker.record_failure();
                            warn!("WebSocket connection failed, will retry");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                    Err(e) => {
                        breaker.record_failure();
                        error!("Failed to connect to WebSocket: {}", e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum ConnectionType {
    OrderBook,
    Trades,
    Combined,
}

/// User data events from authenticated WebSocket
#[derive(Debug, Clone)]
pub enum UserDataEvent {
    Order {
        order_id: String,
        product_id: String,
        side: Side,
        price: Decimal,
        size: Decimal,
        status: String,
        timestamp: DateTime<Utc>,
    },
    Fill {
        trade_id: String,
        order_id: String,
        product_id: String,
        side: Side,
        price: Decimal,
        size: Decimal,
        fee: Decimal,
        timestamp: DateTime<Utc>,
    },
    Balance {
        currency: String,
        available: Decimal,
        hold: Decimal,
        timestamp: DateTime<Utc>,
    },
}

async fn run_market_connection(
    connections: &Arc<RwLock<HashMap<String, WebSocket>>>,
    connection_id: &str,
    tx: &mpsc::UnboundedSender<MarketEvent<InstrumentNameExchange, DataKind>>,
    conn_type: ConnectionType,
) -> Result<(), ()> {
    let mut orderbooks: HashMap<String, OrderBookAccumulator> = HashMap::new();

    loop {
        let msg = {
            let mut conns = connections.write().await;
            if let Some(ws) = conns.get_mut(connection_id) {
                match ws.next().await {
                    Some(Ok(msg)) => msg,
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        conns.remove(connection_id);
                        return Err(());
                    }
                    None => {
                        warn!("WebSocket connection closed");
                        conns.remove(connection_id);
                        return Err(());
                    }
                }
            } else {
                return Err(());
            }
        };

        match msg {
            WsMessage::Text(text) => {
                debug!("Received message: {}", text);
                if let Ok(msg) = serde_json::from_str::<CoinbaseMessage>(&text) {
                    match msg {
                        CoinbaseMessage::Snapshot(snapshot) => {
                            if let Some(event) = convert_snapshot_to_event(snapshot, &mut orderbooks) {
                                let _ = tx.send(event);
                            }
                        }
                        CoinbaseMessage::L2Update(update) => {
                            if let Some(event) = convert_l2_update_to_event(update, &mut orderbooks) {
                                let _ = tx.send(event);
                            }
                        }
                        CoinbaseMessage::Match(trade) => {
                            if let Some(event) = convert_match_to_event(trade) {
                                let _ = tx.send(event);
                            }
                        }
                        CoinbaseMessage::Heartbeat(_) => {
                            // Ignore heartbeats
                        }
                        CoinbaseMessage::Subscriptions(_) => {
                            info!("Subscription confirmed");
                        }
                        CoinbaseMessage::Error(err) => {
                            error!("Coinbase error: {}", err.message);
                        }
                    }
                }
            }
            WsMessage::Ping(data) => {
                let mut conns = connections.write().await;
                if let Some(ws) = conns.get_mut(connection_id) {
                    if ws.send(WsMessage::Pong(data)).await.is_err() {
                        error!("Failed to send pong");
                        return Err(());
                    }
                }
            }
            WsMessage::Close(_) => {
                warn!("Received close frame");
                return Err(());
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct OrderBookAccumulator {
    bids: HashMap<Decimal, Decimal>,
    asks: HashMap<Decimal, Decimal>,
    sequence: u64,
}

fn convert_snapshot_to_event(
    snapshot: SnapshotMessage,
    orderbooks: &mut HashMap<String, OrderBookAccumulator>,
) -> Option<MarketEvent<InstrumentNameExchange, DataKind>> {
    let mut accumulator = OrderBookAccumulator::default();
    
    // Build initial order book
    for bid in snapshot.bids {
        if bid.len() >= 2 {
            if let (Ok(price), Ok(size)) = (
                Decimal::from_str_exact(&bid[0]),
                Decimal::from_str_exact(&bid[1])
            ) {
                if size > Decimal::ZERO {
                    accumulator.bids.insert(price, size);
                }
            }
        }
    }

    for ask in snapshot.asks {
        if ask.len() >= 2 {
            if let (Ok(price), Ok(size)) = (
                Decimal::from_str_exact(&ask[0]),
                Decimal::from_str_exact(&ask[1])
            ) {
                if size > Decimal::ZERO {
                    accumulator.asks.insert(price, size);
                }
            }
        }
    }

    // Convert to sorted levels
    let mut bid_levels: Vec<Level> = accumulator.bids
        .iter()
        .map(|(&price, &size)| Level::new(price, size))
        .collect();
    bid_levels.sort_by(|a, b| b.price.cmp(&a.price)); // Descending

    let mut ask_levels: Vec<Level> = accumulator.asks
        .iter()
        .map(|(&price, &size)| Level::new(price, size))
        .collect();
    ask_levels.sort_by(|a, b| a.price.cmp(&b.price)); // Ascending

    let orderbook = OrderBook::new(
        0, // Coinbase doesn't provide sequence in snapshot
        Some(Utc::now()),
        bid_levels,
        ask_levels,
    );

    // Store accumulator for future updates
    orderbooks.insert(snapshot.product_id.clone(), accumulator);

    Some(MarketEvent {
        time_exchange: Utc::now(),
        time_received: Utc::now(),
        exchange: ExchangeId::Coinbase,
        instrument: InstrumentNameExchange::new(&snapshot.product_id),
        kind: DataKind::OrderBook(OrderBookEvent::Snapshot(orderbook)),
    })
}

fn convert_l2_update_to_event(
    update: L2UpdateMessage,
    orderbooks: &mut HashMap<String, OrderBookAccumulator>,
) -> Option<MarketEvent<InstrumentNameExchange, DataKind>> {
    let accumulator = orderbooks.get_mut(&update.product_id)?;
    let time = DateTime::parse_from_rfc3339(&update.time).ok()?.with_timezone(&Utc);

    // Apply changes
    for change in update.changes {
        if change.len() >= 3 {
            let side = &change[0];
            if let (Ok(price), Ok(size)) = (
                Decimal::from_str_exact(&change[1]),
                Decimal::from_str_exact(&change[2])
            ) {
                match side.as_str() {
                    "buy" => {
                        if size > Decimal::ZERO {
                            accumulator.bids.insert(price, size);
                        } else {
                            accumulator.bids.remove(&price);
                        }
                    }
                    "sell" => {
                        if size > Decimal::ZERO {
                            accumulator.asks.insert(price, size);
                        } else {
                            accumulator.asks.remove(&price);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Convert to sorted levels
    let mut bid_levels: Vec<Level> = accumulator.bids
        .iter()
        .map(|(&price, &size)| Level::new(price, size))
        .collect();
    bid_levels.sort_by(|a, b| b.price.cmp(&a.price));

    let mut ask_levels: Vec<Level> = accumulator.asks
        .iter()
        .map(|(&price, &size)| Level::new(price, size))
        .collect();
    ask_levels.sort_by(|a, b| a.price.cmp(&b.price));

    let orderbook = OrderBook::new(
        0, // Coinbase doesn't provide sequence in updates
        Some(time),
        bid_levels,
        ask_levels,
    );

    Some(MarketEvent {
        time_exchange: time,
        time_received: Utc::now(),
        exchange: ExchangeId::Coinbase,
        instrument: InstrumentNameExchange::new(&update.product_id),
        kind: DataKind::OrderBook(OrderBookEvent::Update(orderbook)),
    })
}

fn convert_match_to_event(trade: MatchMessage) -> Option<MarketEvent<InstrumentNameExchange, DataKind>> {
    let price = trade.price.parse::<f64>().ok()?;
    let amount = trade.size.parse::<f64>().ok()?;
    let time = DateTime::parse_from_rfc3339(&trade.time).ok()?.with_timezone(&Utc);

    let public_trade = PublicTrade {
        id: trade.trade_id.to_string(),
        price,
        amount,
        side: match trade.side.as_str() {
            "buy" => Side::Buy,
            "sell" => Side::Sell,
            _ => return None,
        },
    };

    Some(MarketEvent {
        time_exchange: time,
        time_received: Utc::now(),
        exchange: ExchangeId::Coinbase,
        instrument: InstrumentNameExchange::new(&trade.product_id),
        kind: DataKind::Trade(public_trade),
    })
}

// Message types
#[derive(Debug, Serialize)]
struct SubscribeMessage {
    #[serde(rename = "type")]
    type_: String,
    product_ids: Vec<String>,
    channels: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum CoinbaseMessage {
    #[serde(rename = "snapshot")]
    Snapshot(SnapshotMessage),
    #[serde(rename = "l2update")]
    L2Update(L2UpdateMessage),
    #[serde(rename = "match")]
    Match(MatchMessage),
    #[serde(rename = "heartbeat")]
    Heartbeat(HeartbeatMessage),
    #[serde(rename = "subscriptions")]
    Subscriptions(SubscriptionsMessage),
    #[serde(rename = "error")]
    Error(ErrorMessage),
}

#[derive(Debug, Deserialize)]
struct SnapshotMessage {
    product_id: String,
    bids: Vec<Vec<String>>,
    asks: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct L2UpdateMessage {
    product_id: String,
    time: String,
    changes: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct MatchMessage {
    trade_id: u64,
    sequence: u64,
    maker_order_id: String,
    taker_order_id: String,
    time: String,
    product_id: String,
    size: String,
    price: String,
    side: String,
}

#[derive(Debug, Deserialize)]
struct HeartbeatMessage {
    sequence: u64,
    last_trade_id: u64,
    product_id: String,
    time: String,
}

#[derive(Debug, Deserialize)]
struct SubscriptionsMessage {
    channels: Vec<Channel>,
}

#[derive(Debug, Deserialize)]
struct Channel {
    name: String,
    product_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorMessage {
    message: String,
}

/// Create authentication message for Coinbase WebSocket
fn create_auth_message(auth: &CoinbaseAuth, product_ids: &[String]) -> String {
    use base64::{Engine as _, engine::general_purpose};
    
    let timestamp = Utc::now().timestamp().to_string();
    let method = "GET";
    let path = "/users/self/verify";
    
    // Create signature
    let message = format!("{}{}{}", timestamp, method, path);
    let mut mac = Hmac::<Sha256>::new_from_slice(
        &general_purpose::STANDARD.decode(&auth.api_secret).unwrap_or_default()
    ).expect("HMAC can take key of any size");
    mac.update(message.as_bytes());
    let signature = general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    
    // Create auth message
    json!({
        "type": "subscribe",
        "product_ids": product_ids,
        "channels": [
            {
                "name": "user",
                "product_ids": product_ids
            },
            {
                "name": "full",
                "product_ids": product_ids
            }
        ],
        "signature": signature,
        "key": auth.api_key,
        "passphrase": auth.api_passphrase,
        "timestamp": timestamp
    }).to_string()
}

/// Run authenticated WebSocket connection
async fn run_auth_connection(
    connections: &Arc<RwLock<HashMap<String, WebSocket>>>,
    connection_id: &str,
    tx: &mpsc::UnboundedSender<UserDataEvent>,
) -> Result<(), ()> {
    loop {
        let msg = {
            let mut conns = connections.write().await;
            if let Some(ws) = conns.get_mut(connection_id) {
                match ws.next().await {
                    Some(Ok(msg)) => msg,
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        conns.remove(connection_id);
                        return Err(());
                    }
                    None => {
                        warn!("WebSocket connection closed");
                        conns.remove(connection_id);
                        return Err(());
                    }
                }
            } else {
                return Err(());
            }
        };

        match msg {
            WsMessage::Text(text) => {
                debug!("Received auth message: {}", text);
                if let Ok(msg) = serde_json::from_str::<CoinbaseAuthMessage>(&text) {
                    if let Some(event) = convert_auth_message_to_event(msg) {
                        let _ = tx.send(event);
                    }
                }
            }
            WsMessage::Ping(data) => {
                let mut conns = connections.write().await;
                if let Some(ws) = conns.get_mut(connection_id) {
                    if ws.send(WsMessage::Pong(data)).await.is_err() {
                        error!("Failed to send pong");
                        return Err(());
                    }
                }
            }
            WsMessage::Close(_) => {
                warn!("Received close frame");
                return Err(());
            }
            _ => {}
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum CoinbaseAuthMessage {
    #[serde(rename = "received")]
    Received {
        order_id: String,
        product_id: String,
        side: String,
        price: String,
        size: String,
        time: String,
    },
    #[serde(rename = "open")]
    Open {
        order_id: String,
        product_id: String,
        side: String,
        price: String,
        remaining_size: String,
        time: String,
    },
    #[serde(rename = "done")]
    Done {
        order_id: String,
        product_id: String,
        side: String,
        reason: String,
        time: String,
    },
    #[serde(rename = "match")]
    Match {
        trade_id: u64,
        order_id: String,
        product_id: String,
        side: String,
        price: String,
        size: String,
        time: String,
        fee: String,
    },
    #[serde(rename = "activate")]
    Activate {
        order_id: String,
        product_id: String,
        time: String,
    },
}

fn convert_auth_message_to_event(msg: CoinbaseAuthMessage) -> Option<UserDataEvent> {
    match msg {
        CoinbaseAuthMessage::Received { order_id, product_id, side, price, size, time } |
        CoinbaseAuthMessage::Open { order_id, product_id, side, price, remaining_size: size, time } => {
            let timestamp = DateTime::parse_from_rfc3339(&time).ok()?.with_timezone(&Utc);
            Some(UserDataEvent::Order {
                order_id,
                product_id,
                side: match side.as_str() {
                    "buy" => Side::Buy,
                    "sell" => Side::Sell,
                    _ => return None,
                },
                price: Decimal::from_str(&price).ok()?,
                size: Decimal::from_str(&size).ok()?,
                status: "open".to_string(),
                timestamp,
            })
        }
        CoinbaseAuthMessage::Done { order_id, product_id, side, reason, time } => {
            let timestamp = DateTime::parse_from_rfc3339(&time).ok()?.with_timezone(&Utc);
            Some(UserDataEvent::Order {
                order_id,
                product_id,
                side: match side.as_str() {
                    "buy" => Side::Buy,
                    "sell" => Side::Sell,
                    _ => return None,
                },
                price: Decimal::ZERO,
                size: Decimal::ZERO,
                status: reason,
                timestamp,
            })
        }
        CoinbaseAuthMessage::Match { trade_id, order_id, product_id, side, price, size, time, fee } => {
            let timestamp = DateTime::parse_from_rfc3339(&time).ok()?.with_timezone(&Utc);
            Some(UserDataEvent::Fill {
                trade_id: trade_id.to_string(),
                order_id,
                product_id,
                side: match side.as_str() {
                    "buy" => Side::Buy,
                    "sell" => Side::Sell,
                    _ => return None,
                },
                price: Decimal::from_str(&price).ok()?,
                size: Decimal::from_str(&size).ok()?,
                fee: Decimal::from_str(&fee).ok()?,
                timestamp,
            })
        }
        _ => None,
    }
}