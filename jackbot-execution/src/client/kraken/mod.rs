// Kraken's API offers advanced orders but trailing semantics differ from
//! other venues. Further mapping is required before smart trades are fully
//! supported.
use chrono::{DateTime, TimeZone, Utc};
use futures::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use url::Url;

use crate::{
    balance::{AssetBalance, Balance},
    client::ExecutionClient,
    error::{UnindexedClientError, UnindexedOrderError},
    order::{
        id::{ClientOrderId, OrderId, StrategyId},
        request::{OrderRequestCancel, OrderRequestOpen, UnindexedOrderResponseCancel},
        state::{ActiveOrderState, Cancelled, InactiveOrderState, Open, OrderState},
        Order, OrderKey, OrderKind, TimeInForce,
    },
    trade::{AssetFees, Trade, TradeId},
    AccountEventKind, UnindexedAccountEvent, UnindexedAccountSnapshot,
};
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
use tracing::{error, warn};

/// Configuration for [`KrakenWsClient`].
#[derive(Clone, Debug)]
pub struct KrakenWsConfig {
    /// WebSocket endpoint URL.
    pub url: Url,
    /// Authentication payload sent upon connection.
    pub auth_payload: String,
}

/// WebSocket client streaming authenticated account events from Kraken.
#[derive(Clone, Debug)]
pub struct KrakenWsClient {
    config: KrakenWsConfig,
}

impl ExecutionClient for KrakenWsClient {
    const EXCHANGE: ExchangeId = ExchangeId::Kraken;
    type Config = KrakenWsConfig;
    type AccountStream = UnboundedReceiverStream<UnindexedAccountEvent>;

    fn new(config: Self::Config) -> Self {
        Self { config }
    }

    async fn account_snapshot(
        &self,
        _assets: &[AssetNameExchange],
        _instruments: &[InstrumentNameExchange],
    ) -> Result<UnindexedAccountSnapshot, UnindexedClientError> {
        Ok(UnindexedAccountSnapshot {
            exchange: Self::EXCHANGE,
            balances: vec![],
            instruments: vec![],
        })
    }

    async fn account_stream(
        &self,
        _assets: &[AssetNameExchange],
        _instruments: &[InstrumentNameExchange],
    ) -> Result<Self::AccountStream, UnindexedClientError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let url = self.config.url.clone();
        let auth = self.config.auth_payload.clone();
        tokio::spawn(async move {
            let mut breaker = CircuitBreaker::new(5, std::time::Duration::from_secs(5));
            loop {
                if breaker.is_open() {
                    if let Some(wait) = breaker.remaining() {
                        warn!(?wait, "circuit breaker open, waiting before reconnect");
                        tokio::time::sleep(wait).await;
                        continue;
                    }
                }
                let connection_result = connect(url.clone()).await;
                match connection_result {
                    Ok(ws) => {
                        breaker.reset();
                        let run_result = run_connection(ws, &tx, &auth).await;
                        if run_result.is_err() {
                            breaker.record_failure();
                            let sleep_duration = std::time::Duration::from_millis(50);
                            tokio::time::sleep(sleep_duration).await;
                            continue;
                        } else {
                            break;
                        }
                    }
                    Err(err) => {
                        breaker.record_failure();
                        warn!(?err, "failed to connect to WebSocket");
                        let sleep_duration = std::time::Duration::from_millis(50);
                        tokio::time::sleep(sleep_duration).await;
                    }
                }
            }
        });
        Ok(UnboundedReceiverStream::new(rx))
    }

    async fn cancel_order(
        &self,
        _request: OrderRequestCancel<ExchangeId, InstrumentNameExchange>,
    ) -> UnindexedOrderResponseCancel {
        unimplemented!()
    }

    async fn open_order(
        &self,
        _request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>> {
        unimplemented!()
    }

    async fn fetch_balances(
        &self,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        unimplemented!()
    }

    async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError> {
        unimplemented!()
    }

    async fn fetch_trades(
        &self,
        _time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        unimplemented!()
    }
}

async fn run_connection(
    mut ws: WebSocket,
    tx: &mpsc::UnboundedSender<UnindexedAccountEvent>,
    auth: &str,
) -> Result<(), ()> {
    if ws
        .send(WsMessage::Text(auth.to_string().into()))
        .await
        .is_err()
    {
        error!("failed to send auth payload over WebSocket");
        return Err(());
    }
    loop {
        let next_msg = ws.next().await;
        let msg = match next_msg {
            Some(msg_result) => match msg_result {
                Ok(m) => m,
                Err(err) => {
                    error!(?err, "WebSocket stream error");
                    return Err(());
                }
            },
            None => break,
        };
        match msg {
            WsMessage::Text(text) => {
                let text_str = text.to_string();
                if let Ok(event) = serde_json::from_str::<KrakenEvent>(&text_str) {
                    if let Some(evt) = to_account_event(event) {
                        let _ = tx.send(evt);
                    }
                }
            }
            WsMessage::Close(_) => {
                warn!("received close frame from server");
                return Err(());
            }
            _ => {}
        }
    }
    Err(())
}

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
enum KrakenEvent {
    #[serde(rename = "balance")]
    Balance {
        time: u64,
        asset: String,
        free: String,
        total: String,
    },
    #[serde(rename = "order")]
    Order {
        time: u64,
        pair: String,
        side: String,
        price: String,
        size: String,
        order_id: String,
        status: String,
    },
    #[serde(rename = "trade")]
    Trade {
        time: u64,
        trade_id: u64,
        pair: String,
        client_order_id: Option<String>,
        order_id: String,
        side: String,
        price: String,
        size: String,
    },
}

fn to_account_event(event: KrakenEvent) -> Option<UnindexedAccountEvent> {
    match event {
        KrakenEvent::Balance {
            time,
            asset,
            free,
            total,
        } => {
            let asset_name = AssetNameExchange::new(asset);
            let timestamp = Utc
                .timestamp_opt((time / 1000) as i64, (time % 1000 * 1_000_000) as u32)
                .single()?;
            let balance_details = Balance::new(
                Decimal::from_str(&total).ok()?,
                Decimal::from_str(&free).ok()?,
            );
            let asset_balance = AssetBalance::new(asset_name, balance_details, timestamp);
            Some(UnindexedAccountEvent::new(
                ExchangeId::Kraken,
                AccountEventKind::BalanceSnapshot(Snapshot::new(asset_balance)),
            ))
        }
        KrakenEvent::Order {
            time,
            pair,
            side,
            price,
            size,
            order_id,
            status,
        } => {
            let instrument = InstrumentNameExchange::new(pair);
            let order_id = OrderId::new(order_id.clone());
            let timestamp = Utc
                .timestamp_opt((time / 1000) as i64, (time % 1000 * 1_000_000) as u32)
                .single()?;
            let side = match side.as_str() {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => return None,
            };
            let price = Decimal::from_str(&price).ok()?;
            let size = Decimal::from_str(&size).ok()?;

            let order_key = OrderKey {
                exchange: ExchangeId::Kraken,
                instrument,
                strategy: StrategyId::unknown(),
                cid: ClientOrderId::default(), // Kraken does not provide client_id in order updates directly
            };

            let order_state = match status.as_str() {
                "open" | "pending" | "active" => OrderState::Active(ActiveOrderState::Open(
                    Open::new(order_id, timestamp, Decimal::ZERO), // Assuming 0 filled for new/open orders from Kraken
                )),
                "closed" | "filled" => OrderState::Inactive(InactiveOrderState::FullyFilled),
                "canceled" | "expired" => OrderState::Inactive(InactiveOrderState::Cancelled(
                    Cancelled::new(order_id, timestamp),
                )),
                _ => return None,
            };

            let order = Order {
                key: order_key,
                side,
                price,
                quantity: size,
                kind: OrderKind::Limit, // Assuming limit orders from Kraken
                time_in_force: TimeInForce::GoodUntilCancelled { post_only: false }, // Assuming GTC
                state: order_state,
            };

            Some(UnindexedAccountEvent::new(
                ExchangeId::Kraken,
                AccountEventKind::OrderSnapshot(Snapshot::new(order)),
            ))
        }
        KrakenEvent::Trade {
            time,
            trade_id,
            pair,
            client_order_id,
            order_id,
            side,
            price,
            size,
        } => {
            let instrument = InstrumentNameExchange::new(pair);
            let order_id = OrderId::new(order_id.clone());
            let _client_id = client_order_id.map(ClientOrderId::new);
            let timestamp = Utc
                .timestamp_opt((time / 1000) as i64, (time % 1000 * 1_000_000) as u32)
                .single()?;
            let side = match side.as_str() {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => return None,
            };
            let price = Decimal::from_str(&price).ok()?;
            let size = Decimal::from_str(&size).ok()?;

            let trade_data = Trade {
                id: TradeId::new(trade_id.to_string()),
                order_id,
                instrument,
                strategy: StrategyId::unknown(),
                time_exchange: timestamp,
                side,
                price,
                quantity: size,
                fees: AssetFees::quote_fees(Decimal::ZERO),
            };

            Some(UnindexedAccountEvent::new(
                ExchangeId::Kraken,
                AccountEventKind::Trade(trade_data),
            ))
        }
    }
}
