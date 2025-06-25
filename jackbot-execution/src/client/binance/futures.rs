//! Binance futures API does not natively expose trailing stop or multi-level
//! order types. Smart trade behaviour will be emulated through order
//! management in a future implementation.
use crate::{
    balance::AssetBalance,
    client::ExecutionClient,
    error::{UnindexedClientError, UnindexedOrderError},
    order::{
        request::{OrderRequestCancel, OrderRequestOpen, UnindexedOrderResponseCancel},
        state::Open,
        Order,
    },
    trade::Trade,
    UnindexedAccountEvent, UnindexedAccountSnapshot,
};
use chrono::{DateTime, Utc};
use futures::stream;
use jackbot_instrument::{
    asset::{name::AssetNameExchange, QuoteAsset},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
};

#[derive(Debug, Clone, Default)]
pub struct BinanceFuturesUsd;

#[derive(Debug, Clone, Default)]
pub struct BinanceFuturesUsdConfig;

impl ExecutionClient for BinanceFuturesUsd {
    const EXCHANGE: ExchangeId = ExchangeId::BinanceFuturesUsd;
    type Config = BinanceFuturesUsdConfig;
    type AccountStream = stream::Empty<UnindexedAccountEvent>;

    fn new(_config: Self::Config) -> Self {
        Self
    }

    async fn account_snapshot(
        &self,
        _assets: &[AssetNameExchange],
        _instruments: &[InstrumentNameExchange],
    ) -> Result<UnindexedAccountSnapshot, UnindexedClientError> {
        unimplemented!("Binance futures account_snapshot")
    }

    async fn account_stream(
        &self,
        _assets: &[AssetNameExchange],
        _instruments: &[InstrumentNameExchange],
    ) -> Result<Self::AccountStream, UnindexedClientError> {
        Ok(stream::empty())
    }

    async fn cancel_order(
        &self,
        _request: OrderRequestCancel<ExchangeId, InstrumentNameExchange>,
    ) -> UnindexedOrderResponseCancel {
        unimplemented!("Binance futures cancel_order")
    }

    async fn open_order(
        &self,
        _request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>> {
        unimplemented!("Binance futures open_order")
    }

    async fn fetch_balances(
        &self,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        unimplemented!("Binance futures fetch_balances")
    }

    async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError> {
        unimplemented!("Binance futures fetch_open_orders")
    }

    async fn fetch_trades(
        &self,
        _time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        unimplemented!("Binance futures fetch_trades")
    }
}
