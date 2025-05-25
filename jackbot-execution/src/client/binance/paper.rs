//! Binance paper trading client.
//!
//! `BinancePaperClient` wraps [`PaperEngine`] to emulate Binance Spot trading
//! while exposing the [`ExecutionClient`] interface. This allows switching
//! between live and paper modes by swapping the client implementation in a
//! strategy's configuration.
use crate::{
    client::ExecutionClient,
    exchange::paper::{PaperBook, PaperEngine},
    UnindexedAccountEvent, UnindexedAccountSnapshot,
    balance::AssetBalance,
    order::{
        Order,
        OrderKey,
        request::{OrderRequestCancel, OrderRequestOpen, UnindexedOrderResponseCancel},
        state::Open,
    },
    trade::Trade,
    error::{UnindexedClientError, UnindexedOrderError},
    indexer::{AccountEventKind, AccountEvent},
};
use jackbot_instrument::{
    asset::{QuoteAsset, name::AssetNameExchange},
    exchange::ExchangeId,
    instrument::{Instrument, name::InstrumentNameExchange},
};
use chrono::{DateTime, Utc};
use fnv::FnvHashMap;
use rust_decimal::Decimal;
use std::sync::{Arc, Mutex};
use futures::{Stream, stream::BoxStream, StreamExt};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use std::future::Future;

#[derive(Debug, Clone)]
pub struct BinancePaperConfig {
    pub books: FnvHashMap<InstrumentNameExchange, PaperBook>,
    pub instruments: FnvHashMap<InstrumentNameExchange, Instrument<ExchangeId, AssetNameExchange>>,
    pub snapshot: UnindexedAccountSnapshot,
    pub fees_percent: Decimal,
}

pub struct BinancePaperClient {
    engine: Arc<Mutex<PaperEngine>>,
    event_tx: broadcast::Sender<UnindexedAccountEvent>,
}

impl ExecutionClient for BinancePaperClient {
    const EXCHANGE: ExchangeId = ExchangeId::BinanceSpot;
    type Config = BinancePaperConfig;
    type AccountStream = BoxStream<'static, UnindexedAccountEvent>;

    fn new(config: Self::Config) -> Self {
        let engine = PaperEngine::new(
            Self::EXCHANGE,
            config.fees_percent,
            config.instruments,
            config.books,
            config.snapshot,
        );
        let (tx, _rx) = broadcast::channel(32);
        Self {
            engine: Arc::new(Mutex::new(engine)),
            event_tx: tx,
        }
    }

    fn account_snapshot(
        &self,
        _assets: &[AssetNameExchange],
        _instruments: &[InstrumentNameExchange],
    ) -> impl Future<Output = Result<UnindexedAccountSnapshot, UnindexedClientError>> + Send {
        let engine = self.engine.clone();
        async move {
            let engine = engine.lock().unwrap();
            Ok(engine.account_snapshot())
        }
    }

    fn account_stream(
        &self,
        _assets: &[AssetNameExchange],
        _instruments: &[InstrumentNameExchange],
    ) -> impl Future<Output = Result<Self::AccountStream, UnindexedClientError>> + Send {
        let rx = self.event_tx.subscribe();
        async move { Ok(Box::pin(BroadcastStream::new(rx).map_while(|r| r.ok()))) }
    }

    fn cancel_order(
        &self,
        _request: OrderRequestCancel<ExchangeId, &InstrumentNameExchange>,
    ) -> impl Future<Output = UnindexedOrderResponseCancel> + Send {
        async { unimplemented!("Binance paper cancel_order") }
    }

    fn open_order(
        &self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
    ) -> impl Future<Output = Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> + Send {
        let engine = self.engine.clone();
        let tx = self.event_tx.clone();
        let request_owned = OrderRequestOpen {
            key: OrderKey {
                exchange: request.key.exchange,
                instrument: request.key.instrument.clone(),
                strategy: request.key.strategy,
                cid: request.key.cid.clone(),
            },
            state: request.state.clone(),
        };
        async move {
            let mut engine = engine.lock().unwrap();
            let (order, notifications) = engine.open_order(request_owned);
            if let Some(notifs) = notifications {
                engine.account.ack_trade(notifs.trade.clone());
                let _ = tx.send(AccountEvent::<ExchangeId, AssetNameExchange, InstrumentNameExchange> {
                    exchange: Self::EXCHANGE,
                    kind: AccountEventKind::BalanceSnapshot(notifs.balance),
                });
                let _ = tx.send(AccountEvent::<ExchangeId, AssetNameExchange, InstrumentNameExchange> {
                    exchange: Self::EXCHANGE,
                    kind: AccountEventKind::Trade(notifs.trade),
                });
            }
            order
        }
    }

    fn fetch_balances(&self) -> impl Future<Output = Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError>> + Send {
        let engine = self.engine.clone();
        async move {
            let engine = engine.lock().unwrap();
            Ok(engine.account_snapshot().balances)
        }
    }

    fn fetch_open_orders(
        &self,
    ) -> impl Future<Output = Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError>> + Send {
        async { Ok(Vec::new()) }
    }

    fn fetch_trades(
        &self,
        _time_since: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError>> + Send {
        async { Ok(Vec::new()) }
    }
}
