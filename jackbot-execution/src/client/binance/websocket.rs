use crate::error::UnindexedClientError;
use chrono::{TimeZone, Utc};
use futures_util::{SinkExt, StreamExt};
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
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, info, warn};
use url::Url;

const BINANCE_SPOT_WS: &str = "wss://stream.binance.com:9443/ws";
const BINANCE_FUTURES_WS: &str = "wss://fstream.binance.com/ws";

#[derive(Clone)]
pub struct BinanceWsManager {
    spot_url: Url,
    futures_url: Url,
    connections: Arc<RwLock<HashMap<String, WebSocket>>>,
}

impl BinanceWsManager {
    pub fn new() -> Self {
        Self {
            spot_url: Url::parse(BINANCE_SPOT_WS).expect("Valid URL"),
            futures_url: Url::parse(BINANCE_FUTURES_WS).expect("Valid URL"),
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Subscribe to order book updates for a symbol
    pub async fn subscribe_order_book(
        &self,
        symbol: &str,
        depth: u32,
        is_futures: bool,
    ) -> Result<mpsc::UnboundedReceiver<MarketEvent<InstrumentNameExchange, DataKind>>, UnindexedClientError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let stream_name = format!("{}@depth{}@100ms", symbol.to_lowercase(), depth);
        let url = if is_futures {
            self.futures_url.clone()
        } else {
            self.spot_url.clone()
        };

        let url_with_stream = format!("{}/{}", url.as_str().trim_end_matches("/ws"), stream_name);
        let ws_url = Url::parse(&url_with_stream)
            .map_err(|e| UnindexedClientError::AccountStream(e.to_string()))?;

        self.spawn_connection(ws_url, tx, ConnectionType::OrderBook).await?;
        Ok(rx)
    }

    /// Subscribe to trade updates for a symbol
    pub async fn subscribe_trades(
        &self,
        symbol: &str,
        is_futures: bool,
    ) -> Result<mpsc::UnboundedReceiver<MarketEvent<InstrumentNameExchange, DataKind>>, UnindexedClientError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let stream_name = format!("{}@trade", symbol.to_lowercase());
        let url = if is_futures {
            self.futures_url.clone()
        } else {
            self.spot_url.clone()
        };

        let url_with_stream = format!("{}/{}", url.as_str().trim_end_matches("/ws"), stream_name);
        let ws_url = Url::parse(&url_with_stream)
            .map_err(|e| UnindexedClientError::AccountStream(e.to_string()))?;

        self.spawn_connection(ws_url, tx, ConnectionType::Trades).await?;
        Ok(rx)
    }

    /// Subscribe to multiple streams at once
    pub async fn subscribe_combined(
        &self,
        streams: Vec<String>,
        is_futures: bool,
    ) -> Result<mpsc::UnboundedReceiver<MarketEvent<InstrumentNameExchange, DataKind>>, UnindexedClientError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let combined_stream = streams.join("/");
        let url = if is_futures {
            self.futures_url.clone()
        } else {
            self.spot_url.clone()
        };

        let url_with_stream = format!("{}/stream?streams={}", url.as_str().trim_end_matches("/ws"), combined_stream);
        let ws_url = Url::parse(&url_with_stream)
            .map_err(|e| UnindexedClientError::AccountStream(e.to_string()))?;

        self.spawn_connection(ws_url, tx, ConnectionType::Combined).await?;
        Ok(rx)
    }

    async fn spawn_connection(
        &self,
        url: Url,
        tx: mpsc::UnboundedSender<MarketEvent<InstrumentNameExchange, DataKind>>,
        conn_type: ConnectionType,
    ) -> Result<(), UnindexedClientError> {
        let connections = self.connections.clone();
        let connection_id = url.to_string();

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
                        info!("Connected to Binance WebSocket: {}", url);
                        
                        // Store connection
                        {
                            let mut conns = connections.write().await;
                            conns.insert(connection_id.clone(), ws);
                        }

                        // Run the connection handler
                        let run_result = run_market_connection(&connections, &connection_id, &tx, conn_type).await;
                        
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

async fn run_market_connection(
    connections: &Arc<RwLock<HashMap<String, WebSocket>>>,
    connection_id: &str,
    tx: &mpsc::UnboundedSender<MarketEvent<InstrumentNameExchange, DataKind>>,
    conn_type: ConnectionType,
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
                debug!("Received message: {}", text);
                match conn_type {
                    ConnectionType::OrderBook => {
                        if let Ok(depth) = serde_json::from_str::<DepthUpdate>(&text) {
                            if let Some(event) = convert_depth_to_event(depth) {
                                let _ = tx.send(event);
                            }
                        }
                    }
                    ConnectionType::Trades => {
                        if let Ok(trade) = serde_json::from_str::<TradeUpdate>(&text) {
                            if let Some(event) = convert_trade_to_event(trade) {
                                let _ = tx.send(event);
                            }
                        }
                    }
                    ConnectionType::Combined => {
                        if let Ok(combined) = serde_json::from_str::<CombinedStream>(&text) {
                            if let Some(event) = process_combined_stream(combined) {
                                let _ = tx.send(event);
                            }
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

fn convert_depth_to_event(depth: DepthUpdate) -> Option<MarketEvent<InstrumentNameExchange, DataKind>> {
    let time = Utc.timestamp_millis_opt(depth.event_time).single()?;
    
    // Convert to OrderBook
    let mut bids = Vec::new();
    let mut asks = Vec::new();
    
    for bid in depth.bids {
        if bid.len() >= 2 {
            let price = Decimal::from_str_exact(&bid[0]).ok()?;
            let amount = Decimal::from_str_exact(&bid[1]).ok()?;
            bids.push(Level::new(price, amount));
        }
    }
    
    for ask in depth.asks {
        if ask.len() >= 2 {
            let price = Decimal::from_str_exact(&ask[0]).ok()?;
            let amount = Decimal::from_str_exact(&ask[1]).ok()?;
            asks.push(Level::new(price, amount));
        }
    }
    
    let orderbook = OrderBook::new(
        depth.final_update_id,
        Some(time),
        bids,
        asks,
    );
    
    Some(MarketEvent {
        time_exchange: time,
        time_received: Utc::now(),
        exchange: ExchangeId::BinanceSpot,
        instrument: InstrumentNameExchange::new(&depth.symbol),
        kind: DataKind::OrderBook(OrderBookEvent::Snapshot(orderbook)),
    })
}

fn convert_trade_to_event(trade: TradeUpdate) -> Option<MarketEvent<InstrumentNameExchange, DataKind>> {
    let price = trade.price.parse::<f64>().ok()?;
    let amount = trade.quantity.parse::<f64>().ok()?;
    let time = Utc.timestamp_millis_opt(trade.time).single()?;

    let public_trade = PublicTrade {
        id: trade.trade_id.to_string(),
        price,
        amount,
        side: if trade.is_buyer_maker { Side::Sell } else { Side::Buy },
    };

    Some(MarketEvent {
        time_exchange: time,
        time_received: Utc::now(),
        exchange: ExchangeId::BinanceSpot,
        instrument: InstrumentNameExchange::new(&trade.symbol),
        kind: DataKind::Trade(public_trade),
    })
}

fn process_combined_stream(combined: CombinedStream) -> Option<MarketEvent<InstrumentNameExchange, DataKind>> {
    match combined.data {
        CombinedData::DepthUpdate(depth) => convert_depth_to_event(depth),
        CombinedData::Trade(trade) => convert_trade_to_event(trade),
    }
}

// Message types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DepthUpdate {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "E")]
    event_time: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "U")]
    first_update_id: u64,
    #[serde(rename = "u")]
    final_update_id: u64,
    #[serde(rename = "b")]
    bids: Vec<Vec<String>>,
    #[serde(rename = "a")]
    asks: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TradeUpdate {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "E")]
    event_time: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "t")]
    trade_id: i64,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "b")]
    buyer_order_id: i64,
    #[serde(rename = "a")]
    seller_order_id: i64,
    #[serde(rename = "T")]
    time: i64,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

#[derive(Debug, Deserialize)]
struct CombinedStream {
    stream: String,
    data: CombinedData,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CombinedData {
    DepthUpdate(DepthUpdate),
    Trade(TradeUpdate),
}