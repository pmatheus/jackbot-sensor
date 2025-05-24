# Advanced Order Type Support

This document summarises the current support for advanced order types across all exchanges integrated with Jackbot. Where a venue lacks native support, Jackbot emulates the behaviour via the execution client layer.

## Features Considered

- **Always Maker** – post-only orders placed at the top of book and automatically cancelled or reposted when the price moves.
- **Advanced TWAP** – time-weighted execution using non-linear schedules and order book blending.
- **Advanced VWAP** – volume-weighted execution based on observed market volume patterns.
- **Smart Trades** – trailing take profit, fixed profit targets, trailing stop loss and multi-level stop loss.
- **Prophetic Orders** – capture out-of-book limit orders and place them once the market trades within range.
- **Jackpot Orders** – high leverage bets with strictly controlled ticket loss.

## Exchange Matrix

| Exchange | Always Maker | TWAP/VWAP | Smart Trades | Prophetic Orders | Jackpot Orders | Notes |
|---------|--------------|-----------|--------------|------------------|----------------|-------|
| Binance | Yes | Yes | Yes | Yes | Yes | |
| Bitget | Yes | Yes | Yes | Yes | Yes | |
| Bybit | Yes | Yes | Yes | Yes | Yes | |
| Coinbase | Yes | Yes | Yes | Yes | N/A | Spot only |
| Hyperliquid | Yes | Yes | Yes | Yes | Yes | |
| Kraken | Yes | Yes | Yes | Yes | Yes | |
| MEXC | Yes | Yes | Yes | Yes | Yes | |
| Kucoin | Yes | Yes | Yes | Yes | Yes | |
| Gate.io | Yes | Yes | Yes | Yes | Yes | |
| Crypto.com | Yes | Yes | Yes | Yes | Yes | |
| OKX | Yes | Yes | Yes | Yes | Yes | |

All exchanges expose the same trait-based interface in `jackbot-execution`. Stubs indicate planned integration where the exchange API does not yet offer an equivalent feature.

## Maker-Only (Post-Only) Support

Some advanced execution algorithms rely on placing maker-only orders. The table below summarises native post-only capabilities across supported venues. Where a venue lacks a direct API flag, Jackbot emulates the behaviour by cancelling and reposting orders when they would otherwise match as takers.

| Exchange | Spot Post-Only | Futures Post-Only | Notes |
|----------|----------------|-------------------|-------|
| Binance | Yes | Yes | |
| Bitget | Yes | Yes | |
| Bybit | Yes | Yes | |
| Coinbase | Yes | N/A | Spot only venue |
| Hyperliquid | N/A | Yes | Perpetual markets only |
| Kraken | Yes | Yes | |
| MEXC | Yes | Yes | |
| Kucoin | Yes | Yes | |
| Gate.io | Yes | Yes | |
| Crypto.com | Yes | Yes | |
| OKX | Yes | Yes | |

Refer to `docs/IMPLEMENTATION_STATUS.md` for ongoing implementation details and per-exchange notes.

## Unified Abstraction Design

Advanced order types share common behaviours: splitting orders, scheduling placements and monitoring state. To encourage consistency each exchange implementation should provide an `AdvancedOrderExecutor` built on top of `ExecutionClient`:

```rust
use async_trait::async_trait;
use jackbot_execution::order::{request::OrderRequestOpen, state::Open, Order};
use jackbot_execution::error::UnindexedOrderError;
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange};

#[async_trait]
pub trait AdvancedOrderExecutor {
    async fn always_maker(&mut self, req: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>)
        -> Result<Order<ExchangeId, InstrumentNameExchange, Open>, UnindexedOrderError>;

    async fn twap(&mut self, req: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>, config: twap::TwapConfig)
        -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>>;

    async fn vwap(&mut self, req: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>, config: vwap::VwapConfig)
        -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>>;
}
```

`PropheticOrderManager`, `JackpotOrderManager` and the various smart trade helpers plug into this executor to provide exchange‑agnostic advanced orders. Each exchange adaptor is free to optimize the underlying calls while keeping the surface consistent.

