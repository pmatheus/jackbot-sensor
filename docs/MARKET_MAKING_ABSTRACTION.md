# Market Making Abstraction

Market making strategies often share common behaviour but implementations vary by exchange. This document introduces a simple trait for building market makers in a consistent way.

## Goals
- Provide a single trait to drive quoting algorithms.
- Keep implementations exchange agnostic by building on `ExecutionClient`.
- Allow strategies to expose custom configuration parameters.

## `MarketMakingStrategy`
```rust
use async_trait::async_trait;
use jackbot_execution::{
    client::ExecutionClient,
    error::UnindexedClientError,
    market_making::Quote,
};

#[async_trait]
pub trait MarketMakingStrategy<C>
where
    C: ExecutionClient + Clone + Send + Sync,
{
    /// Additional configuration required by the strategy.
    type Config: Send + Sync;

    /// Maintain markets using the provided execution client and configuration.
    async fn maintain_market(
        &mut self,
        client: &C,
        config: Self::Config,
    ) -> Result<Quote, UnindexedClientError>;
}
```

Implementations may place, cancel or modify orders via `ExecutionClient` while retaining their own quoting logic. The `Config` associated type allows strategies to declare parameters such as target spread, inventory limits or refresh intervals.

## Initial Implementation
`InventorySkewQuoter` and related components can be adapted to this trait. Future exchange-specific strategies can reuse the same interface for consistent behaviour across markets.
