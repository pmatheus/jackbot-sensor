# Smart Trade Abstraction

Smart trade helpers such as trailing take profit or multi-level stop loss share common behaviour. This document introduces a small trait to unify these components and encourage modular composition.

## Goals
- Provide a single interface for evaluating smart trade strategies.
- Keep implementations lightweight and exchange agnostic.
- Enable easy combination of multiple strategies during order management.

## `SmartTradeStrategy`
```rust
use rust_decimal::Decimal;
use jackbot::smart_trade::SmartTradeSignal;

/// Unified interface for smart trade helpers.
pub trait SmartTradeStrategy {
    /// Evaluate the strategy with the latest price and optionally emit a signal.
    fn evaluate(&mut self, price: Decimal) -> Option<SmartTradeSignal>;
}
```

Implementations may track internal state and return a `SmartTradeSignal` when the configured conditions are met.

## Initial Implementations
- `TrailingTakeProfit`
- `TrailingStop`
- `ProfitTarget`
- `MultiLevelStop`
- `MultiLevelTakeProfit`

These helpers now implement `SmartTradeStrategy` and can be composed together by higher level order managers.
