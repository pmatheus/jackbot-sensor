# Real Exchange Integration Guide

## Overview

This guide explains how to use the new real exchange connectivity in jackbot-sensor, which replaces the localhost:8082 mock service with actual WebSocket connections to 8 major cryptocurrency exchanges.

## Quick Start

### 1. Basic Connection

```rust
use jackbot_sensor::exchange_websocket_config::ExchangeWebSocketConfig;
use jackbot_sensor::websocket_connection_pool::WebSocketConnectionPool;

// Create production configuration
let config = ExchangeWebSocketConfig::production();

// Create connection pool
let pool = WebSocketConnectionPool::new(config);

// Initialize connections to exchanges
let exchanges = vec!["binance", "coinbase", "bybit"];
pool.initialize(exchanges).await?;

// Subscribe to market data
pool.subscribe("binance", vec!["btcusdt@ticker".to_string()]).await?;
```

### 2. Using Resilient Connections

```rust
use jackbot_sensor::network_resilience::ResilientWebSocketConnection;

// Create resilient connection with failover
let endpoints = vec![
    "wss://stream.binance.com:9443/ws".to_string(),
    "wss://stream1.binance.com:9443/ws".to_string(),
];

let resilient_conn = ResilientWebSocketConnection::new(
    "binance".to_string(),
    endpoints,
);

// Connect with automatic retry and failover
resilient_conn.connect().await?;
```

## Configuration

### Production Endpoints

All exchange WebSocket URLs are configured in `exchange_websocket_config.rs`:

| Exchange    | Production URL                              | Testnet Available |
|------------|---------------------------------------------|-------------------|
| Binance    | wss://stream.binance.com:9443/ws          | ✅ Yes            |
| Coinbase   | wss://ws-feed.exchange.coinbase.com       | ✅ Yes            |
| Bybit      | wss://stream.bybit.com/v5/public/spot     | ✅ Yes            |
| Bitget     | wss://ws.bitget.com/v2/ws/public          | ❌ No             |
| Hyperliquid| wss://api.hyperliquid.xyz/ws              | ❌ No             |
| KuCoin     | wss://ws-api-spot.kucoin.com              | ✅ Yes            |
| Kraken     | wss://ws.kraken.com                        | ❌ No             |
| OKX        | wss://ws.okx.com:8443/ws/v5/public        | ✅ Yes            |

### Using Testnet

```rust
// Use testnet configuration for development
let config = ExchangeWebSocketConfig::testnet();
```

## API Credentials

For authenticated endpoints (private data streams), set environment variables:

```bash
export BINANCE_API_KEY=your_api_key
export BINANCE_SECRET=your_secret_key
export COINBASE_API_KEY=your_api_key
export COINBASE_SECRET=your_secret_key
# ... etc for other exchanges
```

## Subscription Formats

Each exchange has different subscription message formats:

### Binance
```rust
// Ticker: symbol@ticker
// Trades: symbol@trade
// Order Book: symbol@depth@100ms
pool.subscribe("binance", vec!["btcusdt@ticker".to_string()]).await?;
```

### Coinbase
```rust
// Subscribe with channel names
let subscribe_msg = json!({
    "type": "subscribe",
    "channels": ["ticker"],
    "product_ids": ["BTC-USD"]
});
```

### Bybit
```rust
// Topic-based subscription
let subscribe_msg = json!({
    "op": "subscribe",
    "args": ["spot.ticker.BTCUSDT"]
});
```

## Performance Monitoring

### Check Latency Statistics
```rust
let stats = pool.get_latency_stats().await;
for (endpoint, avg_latency) in stats {
    println!("{}: {:.2}ms average", endpoint, avg_latency);
}
```

### Monitor Connection Health
```rust
let health_status = resilient_conn.failover.get_health_status().await;
for endpoint in health_status {
    println!("{} - {} (latency: {:.2}ms)", 
        endpoint.url,
        if endpoint.is_healthy { "Healthy" } else { "Unhealthy" },
        endpoint.average_latency_ms
    );
}
```

## Error Handling

The system includes multiple layers of error handling:

1. **Circuit Breaker**: Prevents cascade failures
2. **Exponential Backoff**: Smart retry with jitter
3. **Automatic Failover**: Switches to backup endpoints
4. **Connection Pool**: Maintains multiple connections

## Testing

Run integration tests to validate connectivity:

```bash
# Test individual exchanges
cargo test test_binance_real_connection_latency

# Test all exchanges in parallel
cargo test test_all_exchanges_parallel_connection

# Run extended load test
cargo test test_extended_load_performance -- --ignored
```

## Migration from Localhost Mock

### Before (Mock Service)
```rust
const WEBSOCKET_URL: &str = "ws://localhost:8082/ws";
```

### After (Real Exchange)
```rust
let config = ExchangeWebSocketConfig::production();
let url = config.get_optimal_url("binance", Some("us")).await?;
// Returns: "wss://stream.binance.us:9443/ws"
```

## Troubleshooting

### High Latency
- Check regional endpoints: `config.get_optimal_url("binance", Some("us"))`
- Monitor network health: `pool.get_latency_stats()`
- Consider using connection pooling

### Connection Failures
- Check circuit breaker state
- Verify API credentials for private endpoints
- Ensure firewall allows WebSocket connections
- Check exchange status pages

### Rate Limiting
- Connection pools respect per-exchange rate limits
- Automatic throttling based on exchange configuration
- Monitor with `ConnectionMetrics`

## Best Practices

1. **Always use connection pools** for production
2. **Enable resilient connections** for critical systems
3. **Monitor latency continuously**
4. **Use regional endpoints** when available
5. **Implement proper error handling**
6. **Test with real exchanges** before production

## Example: Complete Setup

```rust
use jackbot_sensor::{
    exchange_websocket_config::ExchangeWebSocketConfig,
    websocket_connection_pool::WebSocketConnectionPool,
    streaming::StreamingManager,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize
    let config = ExchangeWebSocketConfig::production();
    let pool = WebSocketConnectionPool::new(config);
    let streaming = StreamingManager::new();
    
    // Connect to exchanges
    let exchanges = vec!["binance", "coinbase", "bybit"];
    pool.initialize(exchanges).await?;
    
    // Subscribe to data
    for exchange in &exchanges {
        pool.subscribe(exchange, vec![
            format!("btcusdt@ticker"),
            format!("btcusdt@trade"),
        ]).await?;
    }
    
    // Process real-time data
    let mut receiver = streaming.subscribe_all().await?;
    while let Ok(event) = receiver.recv().await {
        // Handle market data with <10ms latency
        process_market_event(event);
    }
    
    Ok(())
}
```

## Performance Targets

- **Connection Latency**: <10ms for major exchanges
- **Message Processing**: <1ms per message
- **Failover Time**: <1 second
- **Throughput**: >1000 messages/second
- **Uptime**: 99.9% with resilience patterns

---

*For more details, see the implementation files in `src/` and integration tests in `tests/`*