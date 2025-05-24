use crate::{
    client::ExecutionClient,
    balance::AssetBalance,
    error::{UnindexedClientError, UnindexedOrderError},
    order::{
        Order,
        request::{OrderRequestCancel, OrderRequestOpen, UnindexedOrderResponseCancel},
        state::Open,
    },
    trade::Trade,
};
use jackbot_instrument::{
    asset::{QuoteAsset, name::AssetNameExchange},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Async wrapper trait over [`ExecutionClient`] providing a unified trading
/// interface.
#[async_trait]
pub trait TradingClient: ExecutionClient + Send + Sync {
    /// Place a new order on the exchange.
    async fn open_order(
        &self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
    ) -> Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>> {
        ExecutionClient::open_order(self, request).await
    }

    /// Cancel an existing order on the exchange.
    async fn cancel_order(
        &self,
        request: OrderRequestCancel<ExchangeId, &InstrumentNameExchange>,
    ) -> UnindexedOrderResponseCancel {
        ExecutionClient::cancel_order(self, request).await
    }

    /// Fetch the current account balances from the exchange.
    async fn fetch_balances(&self) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        ExecutionClient::fetch_balances(self).await
    }

    /// Fetch all open orders from the exchange.
    async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError> {
        ExecutionClient::fetch_open_orders(self).await
    }

    /// Fetch trade history from the exchange since the provided timestamp.
    async fn fetch_trades(
        &self,
        time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        ExecutionClient::fetch_trades(self, time_since).await
    }
}

impl<T> TradingClient for T
where
    T: ExecutionClient + Send + Sync,
    T::AccountStream: Send,
{
}

