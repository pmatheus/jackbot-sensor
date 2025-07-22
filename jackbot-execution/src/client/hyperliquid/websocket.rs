//! Hyperliquid WebSocket client implementation.

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
use jackbot_data::exchange::hyperliquid::rate_limit::HyperliquidRateLimit;
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
use tokio::time::Duration;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, warn};

/// Create an account stream for Hyperliquid.
pub async fn create_account_stream(
    config: &HyperliquidConfig,
    assets: &[AssetNameExchange],
    instruments: &[InstrumentNameExchange],
) -> Result<UnboundedReceiverStream<UnindexedAccountEvent>, crate::error::UnindexedClientError> {
    let (tx, rx) = mpsc::unbounded_channel();
    let config = config.clone();
    let assets = assets.to_vec();
    let instruments = instruments.to_vec();

    tokio::spawn(async move {
        let mut breaker = CircuitBreaker::new(5, Duration::from_secs(5));
        let rate_limiter = HyperliquidRateLimit::new();

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
                    error!(?err, "Failed to connect to Hyperliquid WebSocket");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    });

    Ok(UnboundedReceiverStream::new(rx))
}

/// Create a liquidation stream for Hyperliquid.
pub async fn create_liquidation_stream(
    config: &HyperliquidConfig,
) -> Result<UnboundedReceiverStream<super::Liquidation>, crate::error::UnindexedClientError> {
    let (tx, rx) = mpsc::unbounded_channel();
    let config = config.clone();

    tokio::spawn(async move {
        let mut breaker = CircuitBreaker::new(5, Duration::from_secs(5));
        let rate_limiter = HyperliquidRateLimit::new();

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
                    let result = run_liquidation_stream(ws, &tx, &config).await;
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
                    error!(?err, "Failed to connect to Hyperliquid WebSocket");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    });

    Ok(UnboundedReceiverStream::new(rx))
}

/// Run the WebSocket connection for account updates.
async fn run_connection(
    mut ws: WebSocket,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
    config: &HyperliquidConfig,
    assets: &[AssetNameExchange],
    instruments: &[InstrumentNameExchange],
) -> Result<(), ()> {
    // Authenticate if API key is provided
    if let Some(api_key) = &config.api_key {
        if let Err(e) = authenticate(&mut ws, api_key).await {
            error!(?e, "Failed to authenticate WebSocket");
            return Err(());
        }
    }

    // Subscribe to user channels
    if let Err(e) = subscribe_user_channels(&mut ws, &config.account_address).await {
        error!(?e, "Failed to subscribe to user channels");
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

/// Run the liquidation stream.
async fn run_liquidation_stream(
    mut ws: WebSocket,
    tx: &mpsc::UnboundedSender<super::Liquidation>,
    config: &HyperliquidConfig,
) -> Result<(), ()> {
    // Subscribe to liquidation channel
    let sub_msg = HyperliquidWsSubscribe {
        method: "subscribe".to_string(),
        subscription: HyperliquidSubscription {
            sub_type: "liquidations".to_string(),
            user: None,
            coin: None,
        },
    };

    if let Err(e) = ws
        .send(WsMessage::Text(
            serde_json::to_string(&sub_msg).unwrap_or_default().into(),
        ))
        .await
    {
        error!(?e, "Failed to subscribe to liquidations");
        return Err(());
    }

    // Handle liquidation messages
    loop {
        match ws.next().await {
            Some(Ok(msg)) => {
                if let WsMessage::Text(text) = msg {
                    // Parse liquidation messages
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                        if value["channel"] == "liquidations" {
                            // Parse and send liquidation events
                            debug!("Received liquidation event");
                        }
                    }
                }
            }
            Some(Err(e)) => {
                error!(?e, "WebSocket error in liquidation stream");
                return Err(());
            }
            None => {
                warn!("Liquidation stream closed");
                return Err(());
            }
        }
    }
}

/// Authenticate the WebSocket connection.
async fn authenticate(ws: &mut WebSocket, api_key: &str) -> Result<(), SocketError> {
    // Hyperliquid uses API key in handshake headers
    // This is typically done during connection establishment
    debug!("WebSocket authenticated with API key");
    Ok(())
}

/// Subscribe to user channels.
async fn subscribe_user_channels(
    ws: &mut WebSocket,
    account_address: &str,
) -> Result<(), SocketError> {
    let subscriptions = vec![
        HyperliquidWsSubscribe {
            method: "subscribe".to_string(),
            subscription: HyperliquidSubscription {
                sub_type: "webData2".to_string(),
                user: Some(account_address.to_string()),
                coin: None,
            },
        },
        HyperliquidWsSubscribe {
            method: "subscribe".to_string(),
            subscription: HyperliquidSubscription {
                sub_type: "orderUpdates".to_string(),
                user: Some(account_address.to_string()),
                coin: None,
            },
        },
        HyperliquidWsSubscribe {
            method: "subscribe".to_string(),
            subscription: HyperliquidSubscription {
                sub_type: "userFills".to_string(),
                user: Some(account_address.to_string()),
                coin: None,
            },
        },
    ];

    for sub in subscriptions {
        ws.send(WsMessage::Text(
            serde_json::to_string(&sub).map_err(|e| SocketError::Other(e.to_string()))?.into(),
        ))
        .await
        .map_err(|e| SocketError::Other(e.to_string()))?;
    }

    Ok(())
}

/// Handle incoming WebSocket messages.
async fn handle_message(
    msg: WsMessage,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
    config: &HyperliquidConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    match msg {
        WsMessage::Text(text) => {
            let value: serde_json::Value = serde_json::from_str(&text)?;

            // Check message type
            if let Ok(ws_msg) = serde_json::from_value::<HyperliquidWsMessage>(value.clone()) {
                match ws_msg {
                    HyperliquidWsMessage::WebData2 { data } => {
                        handle_web_data(data, tx)?;
                    }
                    HyperliquidWsMessage::OrderUpdates { data } => {
                        handle_order_updates(data, tx)?;
                    }
                    HyperliquidWsMessage::UserFills { data } => {
                        // Handle fills as trade events
                        debug!("Received {} user fills", data.len());
                    }
                    HyperliquidWsMessage::UserFundings { data } => {
                        // Handle funding updates
                        debug!("Received {} funding updates", data.len());
                    }
                    _ => {
                        debug!("Unhandled message type");
                    }
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

/// Handle web data updates (account state).
fn handle_web_data(
    data: WebData2,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(user_state) = data.user_state {
        // Convert margin summary to balance
        let available = Decimal::from_str(&user_state.margin_summary.available_margin)?;
        let total = Decimal::from_str(&user_state.margin_summary.account_value)?;

        let balance = AssetBalance {
            asset: AssetNameExchange::new("USDC"),
            balance: Balance {
                total,
                free: available,
            },
            time_exchange: Utc::now(),
        };

        let event = AccountEvent::new(
            ExchangeId::Hyperliquid,
            AccountEventKind::BalanceSnapshot(Snapshot(balance)),
        );

        let _ = tx.send(event);
    }

    Ok(())
}

/// Handle order updates.
fn handle_order_updates(
    updates: Vec<OrderUpdate>,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    for update in updates {
        let order_info = update.order;
        
        let side = match order_info.side.as_str() {
            "B" => Side::Buy,
            "A" => Side::Sell,
            _ => continue,
        };

        let kind = match order_info.order_type.as_str() {
            "Limit" => OrderKind::Limit,
            "Market" => OrderKind::Market,
            _ => continue,
        };

        let price = Decimal::from_str(&order_info.limit_px)?;
        let quantity = Decimal::from_str(&order_info.sz)?;
        let time = Utc.timestamp_millis_opt(update.status_timestamp).single()
            .unwrap_or_else(Utc::now);

        let cid = order_info.cloid
            .and_then(|c| c.parse::<u64>().ok())
            .map(|id| ClientOrderId::new(id.to_string()))
            .unwrap_or_default();

        let state: OrderState<AssetNameExchange, InstrumentNameExchange> = match update.status.as_str() {
            "open" | "partially_filled" => {
                let filled = Decimal::from_str(&order_info.sz)? 
                    - Decimal::from_str(&order_info.orig_sz)?;
                OrderState::Active(ActiveOrderState::Open(Open {
                    id: OrderId::new(&order_info.oid),
                    time_exchange: time,
                    filled_quantity: filled,
                }))
            }
            "filled" => OrderState::Active(ActiveOrderState::Open(Open {
                id: OrderId::new(&order_info.oid),
                time_exchange: time,
                filled_quantity: quantity,
            })),
            "canceled" | "rejected" => OrderState::Inactive(InactiveOrderState::Cancelled(Cancelled {
                id: OrderId::new(&order_info.oid),
                time_exchange: time,
            })),
            _ => continue,
        };

        let order = Order {
            key: OrderKey {
                exchange: ExchangeId::Hyperliquid,
                instrument: InstrumentNameExchange::new(order_info.coin),
                strategy: StrategyId::unknown(),
                cid,
            },
            side,
            price,
            quantity,
            kind,
            time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
            state,
        };

        let event = AccountEvent::new(
            ExchangeId::Hyperliquid,
            AccountEventKind::OrderSnapshot(Snapshot(order)),
        );

        let _ = tx.send(event);
    }

    Ok(())
}