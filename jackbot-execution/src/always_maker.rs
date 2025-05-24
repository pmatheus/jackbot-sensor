use crate::{
    client::ExecutionClient,
    order::{
        id::ClientOrderId,
        request::{OrderRequestCancel, OrderRequestOpen, RequestCancel},
        Order,
        state::Open,
    },
    error::UnindexedOrderError,
};
use jackbot_data::books::aggregator::OrderBookAggregator;
use jackbot_instrument::{
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
    Side,
};
use rust_decimal::Decimal;
use tokio::time::{sleep, Duration};
use crate::advanced::OrderExecutionStrategy;
use async_trait::async_trait;

/// Simple always maker execution that reposts top-of-book orders until filled.
#[derive(Debug, Clone)]
pub struct AlwaysMaker<C>
where
    C: ExecutionClient + Clone,
{
    /// Client used to place and cancel orders.
    pub client: C,
    /// Aggregated order book view used for price discovery.
    pub aggregator: OrderBookAggregator,
}

/// Parameters controlling always maker behaviour.
#[derive(Debug, Clone, Copy)]
pub struct AlwaysMakerConfig {
    /// Time to wait before cancelling and reposting if not filled.
    pub cancel_after: Duration,
}

impl<C> AlwaysMaker<C>
where
    C: ExecutionClient + Clone,
{
    /// Create a new always maker helper.
    pub fn new(client: C, aggregator: OrderBookAggregator) -> Self {
        Self { client, aggregator }
    }

    /// Execute the provided order request, reposting until filled.
    pub async fn execute(
        &mut self,
        mut request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        cancel_after: Duration,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> {
        let mut remaining = request.state.quantity;
        let mut results = Vec::new();

        while remaining > Decimal::ZERO {
            let price = match request.state.side {
                Side::Buy => self.aggregator.best_bid().map(|(_, p)| p),
                Side::Sell => self.aggregator.best_ask().map(|(_, p)| p),
            };
            let Some(price) = price else { break };

            request.key.cid = ClientOrderId::random();
            request.state.price = price;
            request.state.quantity = remaining;

            let order = self.client.clone().open_order(request.clone()).await;
            let filled = match &order.state {
                Ok(open) => open.filled_quantity,
                Err(_) => Decimal::ZERO,
            };
            let order_id = match &order.state {
                Ok(open) => Some(open.id.clone()),
                Err(_) => None,
            };
            results.push(order.clone());

            if remaining <= filled {
                break;
            }
            remaining -= filled;

            if let Some(id) = order_id {
                sleep(cancel_after).await;
                let cancel = OrderRequestCancel {
                    key: order.key.clone(),
                    state: RequestCancel { id: Some(id) },
                };
                let _ = self.client.clone().cancel_order(cancel).await;
            } else {
                break;
            }
        }

        results
    }
}

#[async_trait]
impl<C> OrderExecutionStrategy for AlwaysMaker<C>
where
    C: ExecutionClient + Clone + Send + Sync,
{
    type Config = AlwaysMakerConfig;

    async fn execute(
        &mut self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        config: Self::Config,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> {
        self.execute(request, config.cancel_after).await
    }
}
