pub mod futures;

use url::Url;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::str::FromStr;
use crate::{
    client::ExecutionClient,
    UnindexedAccountEvent, UnindexedAccountSnapshot,
    balance::{AssetBalance, Balance},
    error::{UnindexedClientError, UnindexedOrderError},
    order::{
        Order, OrderKey, OrderKind, TimeInForce,
        id::{ClientOrderId, OrderId, StrategyId},
        request::{OrderRequestCancel, OrderRequestOpen, UnindexedOrderResponseCancel},
        state::{Open, Cancelled, OrderState},
    },
    trade::{Trade, AssetFees, TradeId},
};
use jackbot_instrument::{
    Side,
    asset::{name::AssetNameExchange, QuoteAsset},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
};
use jackbot_integration::protocol::websocket::{connect, WebSocket};
use jackbot_integration::snapshot::Snapshot;
use tokio::time::Duration;

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
            loop {
                match connect(url.clone()).await {
                    Ok(ws) => {
                        if run_connection(ws, &tx, &auth).await.is_err() {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            continue;
                        } else {
                            break;
                        }
                    }
                    Err(_) => {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
            }
        });
        Ok(UnboundedReceiverStream::new(rx))
    }

    async fn cancel_order(
        &self,
        request: OrderRequestCancel<ExchangeId, &InstrumentNameExchange>,
    ) -> UnindexedOrderResponseCancel {
        UnindexedOrderResponseCancel {
            key: OrderKey {
                exchange: ExchangeId::BinanceSpot,
                instrument: request.key.instrument.clone(),
                strategy: request.key.strategy,
                cid: request.key.cid.clone(),
            },
            state: Ok(Cancelled {
                id: request.state.id.unwrap_or(OrderId(String::new())),
                time_exchange: Utc::now(),
            }),
        }
    }

    async fn open_order(
        &self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
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
                id: OrderId(Utc::now().timestamp_millis().to_string()),
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
    if ws.send(WsMessage::Text(auth.to_string())).await.is_err() {
        return Err(());
    }
    while let Some(msg) = ws.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(_) => return Err(()),
        };
        match msg {
            WsMessage::Text(text) => {
                if let Ok(event) = serde_json::from_str::<BinanceEvent>(&text) {
                    if let Some(evt) = to_account_event(event) {
                        let _ = tx.send(evt);
                    }
                }
            }
            WsMessage::Close(_) => return Err(()),
            _ => {}
        }
    }
    Err(())
}

#[derive(Deserialize)]
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
        BinanceEvent::Balance { time, asset, free, total } => {
            let time = Utc.timestamp_millis_opt(time as i64).single()?;
            let free = Decimal::from_str(&free).ok()?;
            let total = Decimal::from_str(&total).ok()?;
            let balance = AssetBalance {
                asset: AssetNameExchange(asset),
                balance: Balance { total, free },
                time_exchange: time,
            };
            Some(AccountEvent::new(
                ExchangeId::BinanceSpot,
                AccountEventKind::BalanceSnapshot(Snapshot(balance)),
            ))
        }
        BinanceEvent::Order { time, symbol, side, price, quantity, order_id, .. } => {
            let time = Utc.timestamp_millis_opt(time as i64).single()?;
            let side = match side.as_str() {
                "BUY" => Side::Buy,
                "SELL" => Side::Sell,
                _ => return None,
            };
            let price = Decimal::from_str(&price).ok()?;
            let quantity = Decimal::from_str(&quantity).ok()?;
            let order = Order {
                key: OrderKey {
                    exchange: ExchangeId::BinanceSpot,
                    instrument: InstrumentNameExchange(symbol),
                    strategy: StrategyId::unknown(),
                    cid: ClientOrderId::default(),
                },
                side,
                price,
                quantity,
                kind: OrderKind::Market,
                time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
                state: OrderState::active(Open {
                    id: OrderId(order_id.to_string()),
                    time_exchange: time,
                    filled_quantity: quantity,
                }),
            };
            Some(AccountEvent::new(
                ExchangeId::BinanceSpot,
                AccountEventKind::OrderSnapshot(Snapshot(order)),
            ))
        }
    }
}
