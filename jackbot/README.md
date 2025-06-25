# Jackbot
Jackbot core is a Rust framework for building high-performance live-trading, paper-trading and back-testing systems.
* **Fast**: Written in native Rust. Minimal allocations. Data-oriented state management system with direct index lookups.
* **Robust**: Strongly typed. Thread safe. Extensive test coverage.
* **Customisable**: Plug and play Strategy and RiskManager components that facilitates most trading strategies (MarketMaking, StatArb, HFT, etc.).
* **Scalable**: Multithreaded architecture with modular design. Leverages Tokio for I/O. Memory efficient data structures.
* **Market Making**: Built-in two-sided quoting with inventory skew, spread optimisation, toxic flow detection, and adaptive quote refresh.

## Overview
Jackbot core is a Rust framework for building professional grade live-trading, paper-trading and back-testing systems. The
central Engine facilitates executing on many exchanges simultaneously, and offers the flexibility to run most types of
trading strategies. It allows turning algorithmic order generation on/off and can action Commands issued from external
processes (eg/ CloseAllPositions, OpenOrders, CancelOrders, etc.)

At a high-level, it provides a few major components:
* `SystemBuilder` for constructing and initialising a full trading `System`.
* `Engine` with plug and play `Strategy` and `RiskManager` components.
* Centralised cache friendly `EngineState` management with O(1) constant lookups using indexed data structures.  
* `Strategy` interfaces for customising Engine behavior (AlgoStrategy, ClosePositionsStrategy, OnDisconnectStrategy, etc.).
* `RiskManager` interface for defining custom risk logic which checking generated algorithmic orders.
* Event-driven system that allows for Commands to be issued from external processes (eg/ CloseAllPositions, OpenOrders, CancelOrders, etc.),
  as well as turning algorithmic trading on/off.
* Comprehensive statistics package that provides a summary of key performance metrics (PnL, Sharpe, Sortino, Drawdown, etc.).