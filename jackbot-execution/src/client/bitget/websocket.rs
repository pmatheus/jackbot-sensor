//! Bitget WebSocket client implementation.

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
use hmac::{Hmac, Mac};
use jackbot_data::exchange::bitget::rate_limit::BitgetRateLimit;
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
use sha2::Sha256;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, warn};

type HmacSha256 = Hmac<Sha256>;

/// Create an account stream for Bitget.
pub async fn create_account_stream(
    config: &BitgetConfig,
    assets: &[AssetNameExchange],
    instruments: &[InstrumentNameExchange],
) -> Result<UnboundedReceiverStream<UnindexedAccountEvent>, crate::error::UnindexedClientError> {
    let (tx, rx) = mpsc::unbounded_channel();
    let config = config.clone();
    let assets = assets.to_vec();
    let instruments = instruments.to_vec();

    tokio::spawn(async move {
        let mut breaker = CircuitBreaker::new(5, Duration::from_secs(5));
        let rate_limiter = BitgetRateLimit::new();

        loop {
            if breaker.is_open() {
                if let Some(wait) = breaker.remaining() {
                    warn!(?wait, "Circuit breaker open, waiting before reconnect");
                    tokio::time::sleep(wait).await;
                    continue;
                }
            }

            rate_limiter.acquire_ws(Priority::Normal).await;

            match connect(config.ws_url.clone()).await {
                Ok(ws) => {
                    breaker.reset();
                    let result = run_connection(ws, &tx, &config, &assets, &instruments).await;
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
                    error!(?err, "Failed to connect to Bitget WebSocket");
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
    config: &BitgetConfig,
    assets: &[AssetNameExchange],
    instruments: &[InstrumentNameExchange],
) -> Result<(), ()> {
    // Send authentication
    if let Err(e) = authenticate(&mut ws, config).await {
        error!(?e, "Failed to authenticate WebSocket");
        return Err(());
    }

    // Subscribe to channels
    if let Err(e) = subscribe_channels(&mut ws, config, assets, instruments).await {
        error!(?e, "Failed to subscribe to channels");
        return Err(());
    }

    // Main message loop
    loop {
        match ws.next().await {
            Some(Ok(msg)) => {
                if let Err(e) = handle_message(msg, tx, config).await {
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

/// Authenticate the WebSocket connection.
async fn authenticate(ws: &mut WebSocket, config: &BitgetConfig) -> Result<(), SocketError> {
    let timestamp = Utc::now().timestamp_millis();
    let sign_string = format!("{}GET/user/verify", timestamp);
    
    let mut mac = HmacSha256::new_from_slice(config.api_secret.as_bytes())
        .map_err(|e| SocketError::Other(e.to_string()))?;
    mac.update(sign_string.as_bytes());
    let signature = base64::encode(mac.finalize().into_bytes());

    let auth_msg = serde_json::json!({
        "op": "login",
        "args": [{
            "apiKey": config.api_key,
            "passphrase": config.passphrase,
            "timestamp": timestamp.to_string(),
            "sign": signature
        }]
    });

    ws.send(WsMessage::Text(auth_msg.to_string().into()))
        .await
        .map_err(|e| SocketError::Other(e.to_string()))?;

    // Wait for authentication response
    match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
        Ok(Some(Ok(msg))) => {
            if let WsMessage::Text(text) = msg {
                let response: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|e| SocketError::Other(e.to_string()))?;
                
                if response["event"] == "login" && response["code"] == "0" {
                    debug!("Bitget WebSocket authenticated successfully");
                    Ok(())
                } else {
                    Err(SocketError::Other(format!(
                        "Authentication failed: {:?}",
                        response
                    )))
                }
            } else {
                Err(SocketError::Other("Unexpected message type".to_string()))
            }
        }
        Ok(_) => Err(SocketError::Other("Invalid authentication response".to_string())),
        Err(_) => Err(SocketError::Other("Authentication timeout".to_string())),
    }
}

/// Subscribe to WebSocket channels.
async fn subscribe_channels(
    ws: &mut WebSocket,
    config: &BitgetConfig,
    assets: &[AssetNameExchange],
    instruments: &[InstrumentNameExchange],
) -> Result<(), SocketError> {
    let mut channels = Vec::new();

    // Subscribe to account channels
    let inst_type = match config.trading_mode {
        TradingMode::Spot => "SPOT",
        TradingMode::Futures => "UMCBL",
    };

    // Account balance updates
    channels.push(BitgetWsChannel {
        inst_type: inst_type.to_string(),
        channel: "account".to_string(),
        inst_id: "default".to_string(),
    });

    // Order updates for each instrument
    for instrument in instruments {
        channels.push(BitgetWsChannel {
            inst_type: inst_type.to_string(),
            channel: "orders".to_string(),
            inst_id: instrument.as_ref().to_string(),
        });
    }

    let sub_msg = BitgetWsSubscribe {
        op: "subscribe".to_string(),
        args: channels,
    };

    ws.send(WsMessage::Text(
        serde_json::to_string(&sub_msg).map_err(|e| SocketError::Other(e.to_string()))?.into(),
    ))
    .await
    .map_err(|e| SocketError::Other(e.to_string()))?;

    Ok(())
}

/// Handle incoming WebSocket messages.
async fn handle_message(
    msg: WsMessage,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
    config: &BitgetConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    match msg {
        WsMessage::Text(text) => {
            // Handle ping/pong
            if text == "pong" {
                return Ok(());
            }

            let value: serde_json::Value = serde_json::from_str(&text)?;

            // Check if it's a data message
            if let Some(arg) = value.get("arg") {
                if let Some(data) = value.get("data") {
                    let channel = arg["channel"].as_str().unwrap_or("");
                    
                    match channel {
                        "account" => handle_account_update(data, tx, config)?,
                        "orders" => handle_order_update(data, tx, config)?,
                        _ => debug!("Unhandled channel: {}", channel),
                    }
                }
            }
        }
        WsMessage::Ping(data) => {
            // Bitget may send ping frames
            debug!("Received ping, sending pong");
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
fn handle_account_update(
    data: &serde_json::Value,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
    config: &BitgetConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(array) = data.as_array() {
        for item in array {
            match config.trading_mode {
                TradingMode::Spot => {
                    let update: BitgetAccountUpdate = serde_json::from_value(item.clone())?;
                    let available = Decimal::from_str(&update.available)?;
                    let frozen = Decimal::from_str(&update.frozen)?;
                    let locked = Decimal::from_str(&update.lock)?;
                    let time = update.utime.parse::<i64>()
                        .ok()
                        .and_then(|ts| Utc.timestamp_millis_opt(ts).single())
                        .unwrap_or_else(Utc::now);

                    let balance = AssetBalance {
                        asset: AssetNameExchange::new(update.coin_id),
                        balance: Balance {
                            total: available + frozen + locked,
                            free: available,
                        },
                        time_exchange: time,
                    };

                    let event = AccountEvent::new(
                        ExchangeId::Bitget,
                        AccountEventKind::BalanceSnapshot(Snapshot(balance)),
                    );

                    let _ = tx.send(event);
                }
                TradingMode::Futures => {
                    // Handle futures balance update
                    // Similar to spot but with futures-specific fields
                }
            }
        }
    }

    Ok(())
}

/// Handle order updates.
fn handle_order_update(
    data: &serde_json::Value,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
    config: &BitgetConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(array) = data.as_array() {
        for item in array {
            let update: BitgetOrderUpdate = serde_json::from_value(item.clone())?;
            
            let side = match update.side.as_str() {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => continue,
            };

            let kind = match update.order_type.as_str() {
                "limit" => OrderKind::Limit,
                "market" => OrderKind::Market,
                _ => continue,
            };

            let price = Decimal::from_str(&update.price)?;
            let quantity = Decimal::from_str(&update.size)?;
            let filled_quantity = Decimal::from_str(&update.filled_qty)?;
            let time = update.utime.parse::<i64>()
                .ok()
                .and_then(|ts| Utc.timestamp_millis_opt(ts).single())
                .unwrap_or_else(Utc::now);

            let state: OrderState<AssetNameExchange, InstrumentNameExchange> = match update.status.as_str() {
                "new" | "partially_filled" => OrderState::Active(ActiveOrderState::Open(Open {
                    id: OrderId::new(&update.order_id),
                    time_exchange: time,
                    filled_quantity,
                })),
                "filled" => OrderState::Active(ActiveOrderState::Open(Open {
                    id: OrderId::new(&update.order_id),
                    time_exchange: time,
                    filled_quantity: quantity,
                })),
                "cancelled" => OrderState::Inactive(InactiveOrderState::Cancelled(Cancelled {
                    id: OrderId::new(&update.order_id),
                    time_exchange: time,
                })),
                _ => continue,
            };

            let order = Order {
                key: OrderKey {
                    exchange: ExchangeId::Bitget,
                    instrument: InstrumentNameExchange::new(update.inst_id),
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
                ExchangeId::Bitget,
                AccountEventKind::OrderSnapshot(Snapshot(order)),
            );

            let _ = tx.send(event);
        }
    }

    Ok(())
}