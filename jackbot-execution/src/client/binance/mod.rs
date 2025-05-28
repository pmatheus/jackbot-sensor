pub mod futures;
pub mod paper;

use crate::{
    AccountEvent, AccountEventKind, UnindexedAccountEvent, UnindexedAccountSnapshot,
    balance::{AssetBalance, Balance},
    client::ExecutionClient,
    error::{UnindexedClientError, UnindexedOrderError},
    order::{
        Order, OrderKey, OrderKind, TimeInForce,
        id::{ClientOrderId, OrderId, StrategyId},
        request::{OrderRequestCancel, OrderRequestOpen, UnindexedOrderResponseCancel},
        state::{ActiveOrderState, Cancelled, InactiveOrderState, Open, OrderState},
    },
    trade::Trade,
};
use chrono::{DateTime, TimeZone, Utc};
use futures_util::{SinkExt, StreamExt};
use jackbot_instrument::{
    Side,
    asset::{QuoteAsset, name::AssetNameExchange},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
};
use jackbot_integration::snapshot::Snapshot;
use jackbot_integration::{
    circuit_breaker::CircuitBreaker,
    protocol::websocket::{WebSocket, connect},
};
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{error, warn};
use url::Url;

#[derive(Clone, Debug)]
pub struct BinanceWsConfig {
    pub url: Url,
    pub auth_payload: String,
}

#[derive(Clone, Debug)]
pub struct BinanceWsClient {
    config: BinanceWsConfig,
}

impl ExecutionClient for BinanceWsClient {
    const EXCHANGE: ExchangeId = ExchangeId::BinanceSpot;
    type Config = BinanceWsConfig;
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
            let mut breaker = CircuitBreaker::new(5, Duration::from_secs(5));
            loop {
                if breaker.is_open() {
                    if let Some(wait) = breaker.remaining() {
                        warn!(?wait, "circuit breaker open, waiting before reconnect");
                        tokio::time::sleep(wait).await;
                        continue;
                    }
                }
                match connect(url.clone()).await {
                    Ok(ws) => {
                        breaker.reset();
                        if run_connection(ws, &tx, &auth).await.is_err() {
                            breaker.record_failure();
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            continue;
                        } else {
                            break;
                        }
                    }
                    Err(err) => {
                        breaker.record_failure();
                        warn!(?err, "failed to connect to WebSocket");
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
            }
        });
        Ok(UnboundedReceiverStream::new(rx))
    }

    async fn cancel_order(
        &self,
        request: OrderRequestCancel<ExchangeId, InstrumentNameExchange>,
    ) -> UnindexedOrderResponseCancel {
        UnindexedOrderResponseCancel {
            key: OrderKey {
                exchange: ExchangeId::BinanceSpot,
                instrument: request.key.instrument.clone(),
                strategy: request.key.strategy,
                cid: request.key.cid.clone(),
            },
            state: Ok(Cancelled {
                id: request.state.id.unwrap_or(OrderId(String::new().into())),
                time_exchange: Utc::now(),
            }),
        }
    }

    async fn open_order(
        &self,
        request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>> {
        Order {
            key: OrderKey {
                exchange: ExchangeId::BinanceSpot,
                instrument: request.key.instrument.clone(),
                strategy: request.key.strategy,
                cid: request.key.cid.clone(),
            },
            side: request.state.side,
            price: request.state.price,
            quantity: request.state.quantity,
            kind: request.state.kind,
            time_in_force: request.state.time_in_force,
            state: Ok(Open {
                id: OrderId(Utc::now().timestamp_millis().to_string().into()),
                time_exchange: Utc::now(),
                filled_quantity: Decimal::ZERO,
            }),
        }
    }

    async fn fetch_balances(
        &self,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        Ok(Vec::new())
    }

    async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError> {
        Ok(Vec::new())
    }

    async fn fetch_trades(
        &self,
        _time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        Ok(Vec::new())
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
    while let Some(msg) = ws.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(err) => {
                error!(?err, "WebSocket stream error");
                return Err(());
            }
        };
        match msg {
            WsMessage::Text(text) => {
                if let Ok(event) = serde_json::from_str::<BinanceEvent>(&text) {
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
#[serde(tag = "e")]
enum BinanceEvent {
    #[serde(rename = "balance")]
    Balance {
        #[serde(rename = "E")]
        time: u64,
        asset: String,
        free: String,
        total: String,
    },
    #[serde(rename = "order")]
    Order {
        #[serde(rename = "E")]
        time: u64,
        #[serde(rename = "s")]
        symbol: String,
        #[serde(rename = "S")]
        side: String,
        #[serde(rename = "p")]
        price: String,
        #[serde(rename = "q")]
        quantity: String,
        #[serde(rename = "i")]
        order_id: u64,
        #[serde(rename = "X")]
        status: String,
    },
}

fn to_account_event(event: BinanceEvent) -> Option<UnindexedAccountEvent> {
    match event {
        BinanceEvent::Balance {
            time,
            asset,
            free,
            total,
        } => {
            let time = Utc.timestamp_millis_opt(time as i64).single()?;
            let free = Decimal::from_str(&free).ok()?;
            let total = Decimal::from_str(&total).ok()?;
            let balance = AssetBalance {
                asset: AssetNameExchange::new(asset),
                balance: Balance { total, free },
                time_exchange: time,
            };
            Some(AccountEvent::new(
                ExchangeId::BinanceSpot,
                AccountEventKind::BalanceSnapshot(Snapshot(balance)),
            ))
        }
        BinanceEvent::Order {
            time,
            symbol,
            side,
            price,
            quantity,
            order_id,
            status,
        } => {
            let time = Utc.timestamp_millis_opt(time as i64).single()?;
            let instrument = InstrumentNameExchange::new(symbol);
            let side = match side.as_str() {
                "BUY" => Side::Buy,
                "SELL" => Side::Sell,
                _ => return None,
            };
            let parsed_price = Decimal::from_str(&price).ok()?;
            let parsed_quantity = Decimal::from_str(&quantity).ok()?;
            let order_id_str = order_id.to_string();
            let id = OrderId::new(&order_id_str);

            let state: OrderState<AssetNameExchange, InstrumentNameExchange> = match status.as_str()
            {
                "NEW" => OrderState::Active(ActiveOrderState::Open(Open {
                    id: id.clone(),
                    time_exchange: time,
                    filled_quantity: Decimal::ZERO,
                })),
                "FILLED" => OrderState::Active(ActiveOrderState::Open(Open {
                    id: id.clone(),
                    time_exchange: time,
                    filled_quantity: parsed_quantity,
                })),
                "CANCELED" | "EXPIRED" | "REJECTED" => {
                    OrderState::Inactive(InactiveOrderState::Cancelled(Cancelled {
                        id: id.clone(),
                        time_exchange: time,
                    }))
                }
                _ => return None,
            };
            let order = Order {
                key: OrderKey {
                    exchange: ExchangeId::BinanceSpot,
                    instrument,
                    strategy: StrategyId::unknown(),
                    cid: ClientOrderId::default(),
                },
                side,
                price: parsed_price,
                quantity: parsed_quantity,
                kind: OrderKind::Limit,
                time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
                state,
            };
            Some(AccountEvent::new(
                ExchangeId::BinanceSpot,
                AccountEventKind::OrderSnapshot(Snapshot(order)),
            ))
        }
    }
}
