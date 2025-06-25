//! Coinbase spot markets lack native trailing stop orders. Advanced smart trade
//! features will be implemented by client-side order management.
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
use chrono::{DateTime, TimeZone, Utc};
use futures::{SinkExt, StreamExt};
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
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{error, warn};
use url::Url;

#[derive(Clone, Debug)]
pub struct CoinbaseWsConfig {
    pub url: Url,
    pub auth_payload: String,
}

#[derive(Clone, Debug)]
pub struct CoinbaseWsClient {
    config: CoinbaseWsConfig,
}

impl ExecutionClient for CoinbaseWsClient {
    const EXCHANGE: ExchangeId = ExchangeId::Coinbase;
    type Config = CoinbaseWsConfig;
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
                if let Ok(event) = serde_json::from_str::<CoinbaseEvent>(&text_str) {
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
enum CoinbaseEvent {
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
        product_id: String,
        side: String,
        price: String,
        size: String,
        order_id: String,
        status: String,
    },
    #[serde(rename = "fill")]
    Fill {
        time: u64,
        trade_id: u64,
        product_id: String,
        #[allow(dead_code)]
        client_order_id: Option<String>,
        order_id: String,
        side: String,
        price: String,
        size: String,
    },
}

fn to_account_event(event: CoinbaseEvent) -> Option<UnindexedAccountEvent> {
    match event {
        CoinbaseEvent::Balance {
            time,
            asset,
            free,
            total,
        } => {
            let timestamp = Utc
                .timestamp_opt((time / 1000) as i64, (time % 1000 * 1_000_000) as u32)
                .single()?;
            let exchange_asset_name = AssetNameExchange::new(asset);
            let balance_details = Balance::new(
                Decimal::from_str(&total).ok()?,
                Decimal::from_str(&free).ok()?,
            );
            let asset_balance = AssetBalance::new(exchange_asset_name, balance_details, timestamp);
            Some(UnindexedAccountEvent::new(
                ExchangeId::Coinbase,
                AccountEventKind::BalanceSnapshot(Snapshot::new(asset_balance)),
            ))
        }
        CoinbaseEvent::Order {
            time,
            product_id,
            side,
            price,
            size,
            order_id,
            status,
        } => {
            let timestamp = Utc
                .timestamp_opt((time / 1000) as i64, (time % 1000 * 1_000_000) as u32)
                .single()?;
            let instrument = InstrumentNameExchange::new(product_id);
            let order_id_str = order_id.clone();
            let id = OrderId::new(order_id_str.clone());
            let parsed_side = match side.as_str() {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => return None,
            };
            let parsed_price = Decimal::from_str(&price).ok()?;
            let parsed_size = Decimal::from_str(&size).ok()?;

            let order_state: OrderState<AssetNameExchange, InstrumentNameExchange> =
                match status.as_str() {
                    "open" | "pending" | "active" => OrderState::Active(ActiveOrderState::Open(
                        Open::new(id.clone(), timestamp, Decimal::ZERO),
                    )),
                    "done" | "settled" => OrderState::Inactive(InactiveOrderState::FullyFilled),
                    "cancelled" | "rejected" => OrderState::Inactive(
                        InactiveOrderState::Cancelled(Cancelled::new(id.clone(), timestamp)),
                    ),
                    _ => return None,
                };

            let order_key = OrderKey {
                exchange: ExchangeId::Coinbase,
                instrument,
                strategy: StrategyId::unknown(),
                cid: ClientOrderId::default(),
            };

            let order = Order {
                key: order_key,
                side: parsed_side,
                price: parsed_price,
                kind: OrderKind::Limit,
                quantity: parsed_size,
                time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
                state: order_state,
            };

            Some(UnindexedAccountEvent::new(
                ExchangeId::Coinbase,
                AccountEventKind::OrderSnapshot(Snapshot::new(order)),
            ))
        }
        CoinbaseEvent::Fill {
            time,
            trade_id,
            product_id,
            client_order_id: _,
            order_id,
            side,
            price,
            size,
        } => {
            let timestamp = Utc
                .timestamp_opt((time / 1000) as i64, (time % 1000 * 1_000_000) as u32)
                .single()?;
            let instrument = InstrumentNameExchange::new(product_id);
            let order_id_str = order_id.clone();
            let parsed_order_id = OrderId::new(order_id_str);
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
                instrument,
                strategy: StrategyId::unknown(),
                time_exchange: timestamp,
                side: parsed_side,
                price: parsed_price,
                quantity: parsed_size,
                fees: AssetFees::quote_fees(Decimal::ZERO),
            };

            Some(UnindexedAccountEvent::new(
                ExchangeId::Coinbase,
                AccountEventKind::Trade(trade_data),
            ))
        }
    }
}
