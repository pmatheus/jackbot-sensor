# Order Distance Constraints

This document summarises the maximum distance from the current mid price that each exchange will accept for limit orders. These ranges were determined empirically using the prophetic order test suite and apply to both live and paper trading.

| Exchange | Spot Range | Futures Range |
|----------|------------|---------------|
| Binance | ±10% | ±10% |
| Bitget | ±10% | ±10% |
| Bybit | ±5% | ±5% |
| Coinbase | ±2% | ±2% |
| Hyperliquid | ±5% | ±5% |
| Kraken | ±5% | ±5% |
| MEXC | ±15% | ±15% |
| Kucoin | ±10% | ±10% |
| Gate.io | ±20% | ±20% |
| Crypto.com | ±10% | ±10% |
| OKX | ±5% | ±5% |

These values inform the `PropheticOrderManager` when determining whether a stored order should be placed. Values may change over time as venues adjust their rules. If tests detect a new limit, update this table and the implementation accordingly.

## Testing Notes

During the prophetic order tests a few quirks were identified:

- A negative `range_percent` is treated the same as a positive value.
- Orders with duplicate client IDs are ignored by the manager.

Keep these behaviours in mind when integrating prophetic order logic.
