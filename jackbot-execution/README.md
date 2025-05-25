# Jackbot-Execution
Stream private account data from financial venues, and execute (live or mock) orders. Also provides
a feature rich MockExchange and MockExecutionClient to assist with backtesting and paper-trading.

**It is:**
* **Easy**: ExecutionClient trait provides a unified and simple language for interacting with exchanges.
* **Normalised**: Allow your strategy to communicate with every real or MockExchange using the same interface.
* **Extensible**: Jackbot-Execution is highly extensible, making it easy to contribute by adding new exchange integrations!

## Overview
High-performance and normalised trading interface capable of executing across many financial venues. Also provides
a feature rich simulated exchange to assist with backtesting and dry-trading. Communicate with an exchange by
initialising it's associated `ExecutionClient` instance.

## Live vs. Paper Trading

Jackbot-Execution exposes the same `ExecutionClient` trait for both modes.

- **Live trading** clients connect to real exchange APIs and place actual
  orders. Account updates stream directly from the venue.
- **Paper trading** uses the in-memory `PaperEngine` and associated clients
  (for example `BinancePaperClient`) to simulate fills using order book
  snapshots. This is ideal for dry runs or integration testing without
  risking capital.

Switch between modes by selecting the appropriate client implementation in your
strategy configuration.
