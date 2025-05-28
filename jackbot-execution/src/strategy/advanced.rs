use crate::{
    client::ExecutionClient,
    error::UnindexedOrderError,
    order::{Order, request::OrderRequestOpen, state::Open},
};
use async_trait::async_trait;
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange};

/// Unified interface for advanced order execution strategies.
///
/// Implementations may schedule or split a single order request into
/// multiple child orders using the provided `Config`.
#[async_trait]
pub trait OrderExecutionStrategy {
    /// Additional configuration required by the strategy.
    type Config: Send + Sync;

    /// Execute the strategy for the given order request and configuration.
    async fn execute(
        &mut self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        config: Self::Config,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>>;
}
