//! OKX requires algorithmic order endpoints for trailing orders and other
//! advanced features. These smart trade behaviours are stubbed until those
//! endpoints are integrated.
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
    trade::{AssetFees, Trade, TradeId},
};
use chrono::{DateTime, TimeZone, Utc};
use futures::{SinkExt, Stream, StreamExt, stream};
use jackbot_instrument::{
    Side,
    asset::{QuoteAsset, name::AssetNameExchange},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
};
use jackbot_integration::snapshot::Snapshot;
use jackbot_integration::{circuit_breaker::CircuitBreaker, protocol::websocket::WebSocket};
use rust_decimal::Decimal;
use std::future::Future;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{error, warn};
use url::Url;

#[derive(Clone, Debug)]
pub struct OkxWsConfig {
    pub url: Url,
    pub auth_payload: String,
}

#[derive(Clone, Debug)]
pub struct OkxWsClient {
    config: OkxWsConfig,
}

impl ExecutionClient for OkxWsClient {
    const EXCHANGE: ExchangeId = ExchangeId::Okx;
    type Config = OkxWsConfig;
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
                match jackbot_integration::protocol::websocket::connect(url.clone()).await {
                    Ok(ws) => {
                        breaker.reset();
                        if run_connection(ws, &tx, &auth).await.is_err() {
                            breaker.record_failure();
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            continue;
                        } else {
                            break;
                        }
                    }
                    Err(err) => {
                        breaker.record_failure();
                        warn!(?err, "failed to connect to WebSocket");
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
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
                if let Ok(event) = serde_json::from_str::<OkxEvent>(&text) {
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
enum OkxEvent {
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
        instrument: String,
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
        instrument: String,
        client_order_id: Option<String>,
        order_id: String,
        side: String,
        price: String,
        size: String,
    },
}

fn to_account_event(event: OkxEvent) -> Option<UnindexedAccountEvent> {
    match event {
        OkxEvent::Balance {
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
                ExchangeId::Okx,
                AccountEventKind::BalanceSnapshot(Snapshot::new(asset_balance)),
            ))
        }
        OkxEvent::Order {
            time,
            instrument,
            side,
            price,
            size,
            order_id,
            status,
        } => {
            let instrument_name = InstrumentNameExchange::new(instrument);
            let order_id_str = order_id.clone();
            let id = OrderId::new(order_id_str.clone());
            let timestamp = Utc
                .timestamp_opt((time / 1000) as i64, (time % 1000 * 1_000_000) as u32)
                .single()?;
            let parsed_side = match side.as_str() {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => return None,
            };
            let parsed_price = Decimal::from_str(&price).ok()?;
            let parsed_size = Decimal::from_str(&size).ok()?;

            let order_key = OrderKey {
                exchange: ExchangeId::Okx,
                instrument: instrument_name,
                strategy: StrategyId::unknown(),
                cid: ClientOrderId::default(), // OKX may not provide CID in all order updates
            };

            let order_state: OrderState<AssetNameExchange, InstrumentNameExchange> =
                match status.as_str() {
                    "live" | "partially_filled" => OrderState::Active(ActiveOrderState::Open(
                        Open::new(id.clone(), timestamp, Decimal::ZERO), // Assuming 0 filled for new/open
                    )),
                    "filled" => OrderState::Inactive(InactiveOrderState::FullyFilled),
                    "canceled" => OrderState::Inactive(InactiveOrderState::Cancelled(
                        Cancelled::new(id.clone(), timestamp),
                    )),
                    _ => return None, // Other statuses like "pending_cancel" might need handling
                };

            let order = Order {
                key: order_key,
                side: parsed_side,
                price: parsed_price,
                quantity: parsed_size,
                kind: OrderKind::Limit, // Assuming limit orders
                time_in_force: TimeInForce::GoodUntilCancelled { post_only: false }, // Assuming GTC for OKX
                state: order_state,
            };
            Some(UnindexedAccountEvent::new(
                ExchangeId::Okx,
                AccountEventKind::OrderSnapshot(Snapshot::new(order)),
            ))
        }
        OkxEvent::Trade {
            time,
            trade_id,
            instrument,
            client_order_id,
            order_id,
            side,
            price,
            size,
        } => {
            let instrument_name = InstrumentNameExchange::new(instrument);
            let order_id_str = order_id.clone();
            let parsed_order_id = OrderId::new(order_id_str);
            let _parsed_client_id = client_order_id.map(ClientOrderId::new); // Not directly used in Trade struct
            let timestamp = Utc
                .timestamp_opt((time / 1000) as i64, (time % 1000 * 1_000_000) as u32)
                .single()?;
            let parsed_side = match side.as_str() {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => return None,
            };
            let parsed_price = Decimal::from_str(&price).ok()?;
            let parsed_size = Decimal::from_str(&size).ok()?;

            let trade_data = Trade {
                id: TradeId::new(trade_id.to_string()),
                order_id: parsed_order_id,
                instrument: instrument_name,
                strategy: StrategyId::unknown(), // OKX doesn't provide strategy in trade events
                time_exchange: timestamp,
                side: parsed_side,
                price: parsed_price,
                quantity: parsed_size,
                fees: AssetFees::quote_fees(Decimal::ZERO), // Assuming no fee info in OKX trade events
            };
            Some(UnindexedAccountEvent::new(
                ExchangeId::Okx,
                AccountEventKind::Trade(trade_data),
            ))
        }
    }
}
