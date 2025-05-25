//! Kraken's API offers advanced orders but trailing semantics differ from
//! other venues. Further mapping is required before smart trades are fully
//! supported.
use url::Url;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::str::FromStr;
use futures::{StreamExt, SinkExt};

use crate::{
    client::ExecutionClient,
    UnindexedAccountEvent, UnindexedAccountSnapshot,
    balance::{AssetBalance, Balance},
    error::{UnindexedClientError, UnindexedOrderError},
    order::{
        Order, OrderKey, OrderKind, TimeInForce,
        id::{ClientOrderId, OrderId, StrategyId, TradeId},
        request::{OrderRequestCancel, OrderRequestOpen, UnindexedOrderResponseCancel},
        state::{Open, OrderState},
    },
    trade::{Trade, AssetFees},
    AccountEvent, AccountEventKind,
};
use jackbot_instrument::{
    Side,
    asset::{name::AssetNameExchange, QuoteAsset},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
};
use jackbot_integration::protocol::websocket::{connect, WebSocket};
use jackbot_integration::snapshot::Snapshot;

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
            loop {
                match connect(url.clone()).await {
                    Ok(ws) => {
                        if run_connection(ws, &tx, &auth).await.is_err() {
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            continue;
                        } else {
                            break;
                        }
                    }
                    Err(_) => {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                }
            }
        });
        Ok(UnboundedReceiverStream::new(rx))
    }

    async fn cancel_order(
        &self,
        _request: OrderRequestCancel<ExchangeId, &InstrumentNameExchange>,
    ) -> UnindexedOrderResponseCancel {
        unimplemented!()
    }

    async fn open_order(
        &self,
        _request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
    ) -> Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>> {
        unimplemented!()
    }

    async fn fetch_balances(&self) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
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
                if let Ok(event) = serde_json::from_str::<KrakenEvent>(&text) {
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
        side: String,
        price: String,
        size: String,
    },
}

fn to_account_event(event: KrakenEvent) -> Option<UnindexedAccountEvent> {
    match event {
        KrakenEvent::Balance { time, asset, free, total } => {
            let time = Utc.timestamp_millis_opt(time as i64).single()?;
            let free = Decimal::from_str(&free).ok()?;
            let total = Decimal::from_str(&total).ok()?;
            let balance = AssetBalance {
                asset: AssetNameExchange(asset),
                balance: Balance { total, free },
                time_exchange: time,
            };
            Some(AccountEvent::new(
                ExchangeId::Kraken,
                AccountEventKind::BalanceSnapshot(Snapshot(balance)),
            ))
        }
        KrakenEvent::Order { time, pair, side, price, size, order_id, .. } => {
            let time = Utc.timestamp_millis_opt(time as i64).single()?;
            let side = match side.to_uppercase().as_str() {
                "BUY" => Side::Buy,
                "SELL" => Side::Sell,
                _ => return None,
            };
            let price = Decimal::from_str(&price).ok()?;
            let quantity = Decimal::from_str(&size).ok()?;
            let order = Order {
                key: OrderKey {
                    exchange: ExchangeId::Kraken,
                    instrument: InstrumentNameExchange(pair),
                    strategy: StrategyId::unknown(),
                    cid: ClientOrderId::default(),
                },
                side,
                price,
                quantity,
                kind: OrderKind::Market,
                time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
                state: OrderState::active(Open {
                    id: OrderId(order_id),
                    time_exchange: time,
                    filled_quantity: quantity,
                }),
            };
            Some(AccountEvent::new(
                ExchangeId::Kraken,
                AccountEventKind::OrderSnapshot(Snapshot(order)),
            ))
        }
        KrakenEvent::Trade { time, trade_id, pair, side, price, size } => {
            let time = Utc.timestamp_millis_opt(time as i64).single()?;
            let side = match side.to_uppercase().as_str() {
                "BUY" => Side::Buy,
                "SELL" => Side::Sell,
                _ => return None,
            };
            let price = Decimal::from_str(&price).ok()?;
            let quantity = Decimal::from_str(&size).ok()?;
            let trade = Trade {
                id: TradeId(trade_id.to_string()),
                order_id: OrderId(String::new()),
                instrument: InstrumentNameExchange(pair),
                strategy: StrategyId::unknown(),
                time_exchange: time,
                side,
                price,
                quantity,
                fees: AssetFees::default(),
            };
            Some(AccountEvent::new(
                ExchangeId::Kraken,
                AccountEventKind::Trade(trade),
            ))
        }
    }
}

