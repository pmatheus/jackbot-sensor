//! KuCoin WebSocket client implementation.

use super::types::*;
use crate::{
    balance::{AssetBalance, Balance},
    order::{
        id::{ClientOrderId, OrderId, StrategyId},
        state::{ActiveOrderState, InactiveOrderState, Open, OrderState, Cancelled},
        Order, OrderKey, OrderKind, TimeInForce,
    },
    AccountEvent, AccountEventKind, UnindexedAccountEvent,
};
use chrono::{TimeZone, Utc};
use futures_util::{SinkExt, StreamExt};
use jackbot_data::exchange::kucoin::rate_limit::KucoinRateLimit;
use jackbot_instrument::{
    asset::name::AssetNameExchange,
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
    Side,
};
use jackbot_integration::{
    circuit_breaker::CircuitBreaker,
    error::SocketError,
    protocol::websocket::{connect, WebSocket},
    rate_limit::Priority,
    snapshot::Snapshot,
};
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, warn};
use uuid::Uuid;

/// Create an account stream for KuCoin.
pub async fn create_account_stream(
    config: &KuCoinConfig,
    assets: &[AssetNameExchange],
    instruments: &[InstrumentNameExchange],
) -> Result<UnboundedReceiverStream<UnindexedAccountEvent>, crate::error::UnindexedClientError> {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut config = config.clone();
    let assets = assets.to_vec();
    let instruments = instruments.to_vec();

    // Get WebSocket connection info
    let rest_client = super::rest::KuCoinRestClient::new(config.clone());
    let ws_info = rest_client.get_ws_connection_info().await
        .map_err(|e| crate::error::UnindexedClientError::Other(e.to_string()))?;

    // Use the first instance server
    let server = ws_info.instance_servers.first()
        .ok_or_else(|| crate::error::UnindexedClientError::Other("No WebSocket servers available".to_string()))?;

    let ws_url = format!("{}?token={}", server.endpoint, ws_info.token);
    config.ws_url = Some(url::Url::parse(&ws_url)
        .map_err(|e| crate::error::UnindexedClientError::Other(e.to_string()))?);

    let ping_interval_ms = server.ping_interval;

    tokio::spawn(async move {
        let mut breaker = CircuitBreaker::new(5, Duration::from_secs(5));
        let rate_limiter = KucoinRateLimit::new();

        loop {
            if breaker.is_open() {
                if let Some(wait) = breaker.remaining() {
                    warn!(?wait, "Circuit breaker open, waiting before reconnect");
                    tokio::time::sleep(wait).await;
                    continue;
                }
            }

            rate_limiter.acquire_ws(Priority::Normal).await;

            match connect(config.ws_url.as_ref().unwrap().clone()).await {
                Ok(ws) => {
                    breaker.reset();
                    let result = run_connection(
                        ws, 
                        &tx, 
                        &config, 
                        &assets, 
                        &instruments,
                        ping_interval_ms,
                    ).await;
                    if result.is_err() {
                        breaker.record_failure();
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    } else {
                        break;
                    }
                }
                Err(err) => {
                    breaker.record_failure();
                    error!(?err, "Failed to connect to KuCoin WebSocket");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    });

    Ok(UnboundedReceiverStream::new(rx))
}

/// Run the WebSocket connection.
async fn run_connection(
    mut ws: WebSocket,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
    config: &KuCoinConfig,
    assets: &[AssetNameExchange],
    instruments: &[InstrumentNameExchange],
    ping_interval_ms: i64,
) -> Result<(), ()> {
    // Subscribe to channels
    if let Err(e) = subscribe_channels(&mut ws, assets, instruments).await {
        error!(?e, "Failed to subscribe to channels");
        return Err(());
    }

    // Start ping task
    let mut ping_interval = interval(Duration::from_millis(ping_interval_ms as u64));

    // Main message loop
    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                let ping_msg = serde_json::json!({
                    "id": Uuid::new_v4().to_string(),
                    "type": "ping"
                });
                
                if let Err(e) = ws.send(WsMessage::Text(ping_msg.to_string().into())).await {
                    error!(?e, "Failed to send ping");
                    return Err(());
                }
            }
            msg = ws.next() => {
                match msg {
                    Some(Ok(msg)) => {
                        if let Err(e) = handle_message(msg, tx).await {
                            error!(?e, "Error handling WebSocket message");
                        }
                    }
                    Some(Err(e)) => {
                        error!(?e, "WebSocket error");
                        return Err(());
                    }
                    None => {
                        warn!("WebSocket connection closed");
                        return Err(());
                    }
                }
            }
        }
    }
}

/// Subscribe to WebSocket channels.
async fn subscribe_channels(
    ws: &mut WebSocket,
    assets: &[AssetNameExchange],
    instruments: &[InstrumentNameExchange],
) -> Result<(), SocketError> {
    // Subscribe to account balance changes
    let balance_sub = KuCoinWsSubscribe {
        id: Uuid::new_v4().to_string(),
        r#type: "subscribe".to_string(),
        topic: "/account/balance".to_string(),
        private_channel: true,
        response: true,
    };

    ws.send(WsMessage::Text(
        serde_json::to_string(&balance_sub).map_err(|e| SocketError::Other(e.to_string()))?.into(),
    ))
    .await
    .map_err(|e| SocketError::Other(e.to_string()))?;

    // Subscribe to order changes
    let order_sub = KuCoinWsSubscribe {
        id: Uuid::new_v4().to_string(),
        r#type: "subscribe".to_string(),
        topic: "/spotMarket/tradeOrders".to_string(),
        private_channel: true,
        response: true,
    };

    ws.send(WsMessage::Text(
        serde_json::to_string(&order_sub).map_err(|e| SocketError::Other(e.to_string()))?.into(),
    ))
    .await
    .map_err(|e| SocketError::Other(e.to_string()))?;

    Ok(())
}

/// Handle incoming WebSocket messages.
async fn handle_message(
    msg: WsMessage,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    match msg {
        WsMessage::Text(text) => {
            let ws_msg: KuCoinWsMessage = serde_json::from_str(&text)?;

            match ws_msg.r#type.as_str() {
                "message" => {
                    if let Some(topic) = ws_msg.topic {
                        if let Some(data) = ws_msg.data {
                            match topic.as_str() {
                                "/account/balance" => handle_balance_update(data, tx)?,
                                "/spotMarket/tradeOrders" => handle_order_update(data, tx)?,
                                _ => debug!("Unhandled topic: {}", topic),
                            }
                        }
                    }
                }
                "pong" => {
                    // Pong received
                    debug!("Received pong");
                }
                "welcome" => {
                    debug!("WebSocket connection established");
                }
                "ack" => {
                    if let Some(id) = ws_msg.id {
                        debug!("Subscription acknowledged: {}", id);
                    }
                }
                "error" => {
                    error!("WebSocket error: {:?}", ws_msg.data);
                }
                _ => {
                    debug!("Unhandled message type: {}", ws_msg.r#type);
                }
            }
        }
        WsMessage::Ping(data) => {
            debug!("Received ping");
        }
        WsMessage::Pong(_) => {
            // Pong received
        }
        WsMessage::Close(_) => {
            warn!("Received close frame from server");
            return Err("Connection closed".into());
        }
        _ => {}
    }

    Ok(())
}

/// Handle account balance updates.
fn handle_balance_update(
    data: serde_json::Value,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let update: KuCoinAccountUpdate = serde_json::from_value(data)?;
    
    let total = Decimal::from_str(&update.total)?;
    let available = Decimal::from_str(&update.available)?;
    let time = update.time.parse::<i64>()
        .ok()
        .and_then(|ts| Utc.timestamp_millis_opt(ts).single())
        .unwrap_or_else(Utc::now);

    let balance = AssetBalance {
        asset: AssetNameExchange::new(update.currency),
        balance: Balance {
            total,
            free: available,
        },
        time_exchange: time,
    };

    let event = AccountEvent::new(
        ExchangeId::Kucoin,
        AccountEventKind::BalanceSnapshot(Snapshot(balance)),
    );

    let _ = tx.send(event);

    Ok(())
}

/// Handle order updates.
fn handle_order_update(
    data: serde_json::Value,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let update: KuCoinOrderChange = serde_json::from_value(data)?;
    
    let side = match update.side.as_str() {
        "buy" => Side::Buy,
        "sell" => Side::Sell,
        _ => return Ok(()),
    };

    let kind = match update.order_type.as_str() {
        "limit" => OrderKind::Limit,
        "market" => OrderKind::Market,
        _ => return Ok(()),
    };

    let price = Decimal::from_str(&update.price)?;
    let quantity = Decimal::from_str(&update.size)?;
    let filled_quantity = Decimal::from_str(&update.filled_size)?;
    let time = Utc.timestamp_millis_opt(update.ts).single()
        .unwrap_or_else(Utc::now);

    let state: OrderState<AssetNameExchange, InstrumentNameExchange> = match update.status.as_str() {
        "open" | "match" => OrderState::Active(ActiveOrderState::Open(Open {
            id: OrderId::new(&update.order_id),
            time_exchange: time,
            filled_quantity,
        })),
        "done" => {
            if filled_quantity == quantity {
                OrderState::Active(ActiveOrderState::Open(Open {
                    id: OrderId::new(&update.order_id),
                    time_exchange: time,
                    filled_quantity: quantity,
                }))
            } else {
                OrderState::Inactive(InactiveOrderState::Cancelled(Cancelled {
                    id: OrderId::new(&update.order_id),
                    time_exchange: time,
                }))
            }
        }
        "cancel" => OrderState::Inactive(InactiveOrderState::Cancelled(Cancelled {
            id: OrderId::new(&update.order_id),
            time_exchange: time,
        })),
        _ => return Ok(()),
    };

    let order = Order {
        key: OrderKey {
            exchange: ExchangeId::Kucoin,
            instrument: InstrumentNameExchange::new(update.symbol),
            strategy: StrategyId::unknown(),
            cid: ClientOrderId::new(update.client_oid),
        },
        side,
        price,
        quantity,
        kind,
        time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
        state,
    };

    let event = AccountEvent::new(
        ExchangeId::Kucoin,
        AccountEventKind::OrderSnapshot(Snapshot(order)),
    );

    let _ = tx.send(event);

    Ok(())
}