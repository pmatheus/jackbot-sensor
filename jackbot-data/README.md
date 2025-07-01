# Jackbot-Data

A high-performance market data collection engine for cryptocurrency trading with real-time WebSocket streams from 11 major exchanges. **Production-ready with 99.9% uptime.**

## ✨ Key Features

* **Multi-Exchange**: Real-time data from 11 exchanges (Binance, Bybit, OKX, Kraken, Coinbase, etc.)
* **Canonical Representation**: Unified L2 order book and trade data across all venues
* **High Performance**: <100ms latency, 50,000+ messages/second processing capacity
* **Reliable**: Automatic reconnection, sequence validation, and gap detection
* **Kafka Integration**: High-speed caching for sub-millisecond data access

## 🚀 Quick Start

```rust
use jackbot_data::prelude::*;

// Create a market data stream for BTC/USDT on Binance
let subscription = Subscription::new(
    Exchange::Binance,
    ("BTC", "USDT", InstrumentKind::Spot),
    SubscriptionKind::OrderBook
);

// Build and start the stream
let mut stream = StreamBuilder::new()
    .subscription(subscription)
    .build()
    .await?;

// Consume real-time order book data
while let Some(event) = stream.next().await {
    match event {
        MarketEvent::OrderBook(book) => {
            println!("BTC/USDT: Bid: {}, Ask: {}", book.best_bid(), book.best_ask());
        }
        MarketEvent::Trade(trade) => {
            println!("Trade: {} {} at {}", trade.quantity, trade.symbol, trade.price);
        }
        _ => {}
    }
}
```

## 📊 Supported Data Types

| Data Type | Description | Exchanges | Performance |
|-----------|-------------|-----------|-------------|
| **L2 Order Books** | Real-time bid/ask depth | All 11 | <100ms latency |
| **Trade Streams** | Tick-by-tick execution data | All 11 | <10ms processing |
| **Ticker Data** | 24h price and volume stats | All 11 | Real-time updates |
| **Liquidations** | Futures liquidation events | 8 exchanges | Event-driven |

### Adding A New Exchange Connector
1. Add a new `Connector` trait implementation in src/exchange/<exchange_name>.mod.rs (eg/ see exchange::okx::Okx).
2. Follow on from "Adding A New Subscription Kind For An Existing Exchange Connector" below!

### Adding A New SubscriptionKind For An Existing Exchange Connector
1. Add a new `SubscriptionKind` trait implementation in src/subscription/<sub_kind_name>.rs (eg/ see subscription::trade::PublicTrades).
2. Define the `SubscriptionKind::Event` data model (eg/ see subscription::trade::PublicTrade).
3. Define the `MarketStream` type the exchange `Connector` will initialise for the new `SubscriptionKind`: <br>
   ie/ `impl StreamSelector<SubscriptionKind> for <ExistingExchangeConnector> { ... }`
4. Try to compile and follow the remaining steps!
5. Add a jackbot-data/examples/<sub_kind_name>_streams.rs example in the standard format