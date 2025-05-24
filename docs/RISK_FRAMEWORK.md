# Jackbot Risk Management Framework

Jackbot provides a modular risk framework used by all trading components. The
`jackbot-risk` crate exposes utilities for tracking exposure, monitoring
drawdowns, enforcing correlation limits and scaling position sizes.

## Components
- **PositionTracker** – tracks net positions per `(ExchangeId, Instrument)` pair.
- **DrawdownTracker** – records realised and unrealised PnL to compute drawdowns.
- **CorrelationMatrix** – enforces correlation based exposure limits between
  instrument pairs.
- **VolatilityScaler** – adjusts position sizes and risk limits according to
  observed volatility.
- **ExposureRiskManager** – integrates all of the above with the trading engine
  and can generate mitigation commands when limits are breached.
- **Stress Testing Utilities** – evaluate PnL impact under simulated price
  shocks.

## Exchange Support
All exchanges share the same risk management layer. Integration tests verify
consistent behaviour for spot and futures markets across the supported
exchanges.

| Exchange | Spot Risk | Futures Risk |
|----------|-----------|--------------|
| Binance | ✓ | ✓ |
| Bitget | ✓ | ✓ |
| Bybit | ✓ | ✓ |
| Coinbase | ✓ | ✓ |
| Hyperliquid | ✓ | ✓ |
| Kraken | ✓ | ✓ |
| MEXC | ✓ | ✓ |
| Kucoin | ✓ | ✓ |
| Gate.io | ✓ | ✓ |
| Crypto.com | ✓ | ✓ |
| OKX | ✓ | ✓ |

These features are enforced in both live and paper trading environments.
