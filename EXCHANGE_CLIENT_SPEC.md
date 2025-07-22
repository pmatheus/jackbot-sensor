# JackBot Exchange Client Implementation Specification

This document tracks the implementation requirements for exchange client integrations in the JackBot execution module.

## Overview

Multiple exchange clients have placeholder implementations that need to be completed with actual API integrations. Each exchange requires REST API and WebSocket implementations for trading operations.

## Exchange Clients to Implement

### 1. MEXC Client (`jackbot-execution/src/client/mexc/mod.rs`)

#### REST API Endpoints
- [ ] **account_snapshot** - Fetch account balances and positions
  - Endpoint: `GET /api/v3/account`
  - Authentication: HMAC-SHA256 signature
  
- [ ] **fetch_balances** - Get current balances
  - Endpoint: `GET /api/v3/capital/config/getall`
  - Response: Asset balances with free/locked amounts
  
- [ ] **open_order** - Place new order
  - Endpoint: `POST /api/v3/order`
  - Parameters: symbol, side, type, quantity, price
  
- [ ] **cancel_order** - Cancel existing order
  - Endpoint: `DELETE /api/v3/order`
  - Parameters: orderId or clientOrderId
  
- [ ] **fetch_open_orders** - Get all open orders
  - Endpoint: `GET /api/v3/openOrders`
  - Optional: symbol filter
  
- [ ] **fetch_trades** - Get trade history
  - Endpoint: `GET /api/v3/myTrades`
  - Parameters: symbol, startTime, endTime

#### WebSocket Streams
- [ ] **account_stream** - Real-time account updates
  - Stream: User Data Stream
  - Events: Balance updates, order updates, trade executions

### 2. Gate.io Client (`jackbot-execution/src/client/gateio/mod.rs`)

#### REST API Endpoints
- [ ] **account_snapshot** - Fetch account information
  - Endpoint: `GET /api/v4/spot/accounts`
  - Authentication: API key + signature
  
- [ ] **fetch_balances** - Get spot balances
  - Endpoint: `GET /api/v4/spot/accounts`
  - Response: Currency balances
  
- [ ] **open_order** - Create spot order
  - Endpoint: `POST /api/v4/spot/orders`
  - Parameters: currency_pair, side, type, amount, price
  
- [ ] **cancel_order** - Cancel spot order
  - Endpoint: `DELETE /api/v4/spot/orders/{order_id}`
  - Parameters: order_id, currency_pair
  
- [ ] **fetch_open_orders** - List open orders
  - Endpoint: `GET /api/v4/spot/orders`
  - Parameters: status=open
  
- [ ] **fetch_trades** - Get personal trading history
  - Endpoint: `GET /api/v4/spot/my_trades`
  - Parameters: currency_pair, from, to

#### WebSocket Streams
- [ ] **account_stream** - User data updates
  - Channel: `spot.usertrades`, `spot.orders`, `spot.balances`
  - Authentication: Signed connection

### 3. Crypto.com Client (`jackbot-execution/src/client/cryptocom/mod.rs`)

#### REST API Endpoints
- [ ] **account_snapshot** - Get account summary
  - Endpoint: `POST /v2/private/get-account-summary`
  - Authentication: API key + HMAC
  
- [ ] **fetch_balances** - Get wallet balance
  - Endpoint: `POST /v2/private/get-accounts`
  - Response: Account balances by currency
  
- [ ] **open_order** - Create order
  - Endpoint: `POST /v2/private/create-order`
  - Parameters: instrument_name, side, type, quantity, price
  
- [ ] **cancel_order** - Cancel order
  - Endpoint: `POST /v2/private/cancel-order`
  - Parameters: order_id or client_oid
  
- [ ] **fetch_open_orders** - Get open orders
  - Endpoint: `POST /v2/private/get-open-orders`
  - Optional: instrument_name filter
  
- [ ] **fetch_trades** - Get trades
  - Endpoint: `POST /v2/private/get-trades`
  - Parameters: instrument_name, start_ts, end_ts

#### WebSocket Streams
- [ ] **account_stream** - Private user updates
  - Subscription: `user.balance`, `user.order`, `user.trade`
  - Authentication: Signed WebSocket connection

### 4. KuCoin Client (`jackbot-execution/src/client/kucoin/mod.rs`)

Note: Already partially implemented, needs completion:
- [ ] Add margin/futures position support
- [ ] Implement order fetching in account snapshots
- [ ] Add advanced order types support

### 5. Bitget Client (`jackbot-execution/src/client/bitget/mod.rs`)

Note: Already partially implemented, needs completion:
- [ ] Add futures/swap position support
- [ ] Implement order fetching in account snapshots
- [ ] Add copy trading integration

### 6. Hyperliquid Client (`jackbot-execution/src/client/hyperliquid/mod.rs`)

Note: Custom implementation required:
- [ ] Implement order fetching for account snapshots
- [ ] Add L2 orderbook integration
- [ ] Implement vault operations

## Implementation Guidelines

### Authentication
Each exchange has specific authentication requirements:
- **API Key**: Required for all exchanges
- **Secret Key**: Used for signing requests
- **Passphrase**: Required for some exchanges (KuCoin)
- **Signature Algorithm**: Usually HMAC-SHA256, some use RSA

### Error Handling
```rust
// Convert exchange-specific errors to UnindexedClientError
match exchange_response {
    Ok(data) => process_data(data),
    Err(e) => match e.code {
        "INSUFFICIENT_BALANCE" => Err(UnindexedClientError::InsufficientBalance),
        "INVALID_ORDER" => Err(UnindexedClientError::InvalidOrder),
        _ => Err(UnindexedClientError::Exchange(e.to_string())),
    }
}
```

### Rate Limiting
- Implement exponential backoff for rate limit errors
- Track API weight/credits per exchange
- Use WebSocket for real-time data when possible

### Response Normalization
```rust
// Convert exchange format to standard format
fn normalize_order(exchange_order: ExchangeOrder) -> Order {
    Order {
        key: OrderKey {
            client_order_id: ClientOrderId::from(exchange_order.client_id),
            exchange: ExchangeId::Mexc,
            strategy_id: StrategyId::default(),
        },
        side: parse_side(&exchange_order.side),
        price: Decimal::from_str(&exchange_order.price)?,
        quantity: Decimal::from_str(&exchange_order.quantity)?,
        // ... other fields
    }
}
```

## Testing Requirements

### Unit Tests
- Mock HTTP responses for each endpoint
- Test error handling scenarios
- Verify signature generation

### Integration Tests
- Test with exchange sandboxes
- Verify order lifecycle
- Test reconnection logic

### Common Test Scenarios
1. Place limit order -> Get order status -> Cancel order
2. Fetch balances -> Place market order -> Verify balance change
3. WebSocket connection -> Receive updates -> Handle disconnection
4. Rate limit handling -> Retry logic -> Success

## Security Considerations

1. **API Credentials**
   - Never log sensitive data
   - Use secure storage for keys
   - Implement key rotation support

2. **Network Security**
   - Always use HTTPS
   - Verify SSL certificates
   - Implement request timeouts

3. **Data Validation**
   - Validate all exchange responses
   - Sanitize user inputs
   - Check decimal precision limits

## Performance Requirements

- REST API calls: < 100ms latency (excluding network)
- WebSocket message processing: < 10ms
- Order placement: < 50ms (critical path)
- Concurrent connections: Support multiple symbols

## Dependencies

### Required Crates
- `reqwest`: HTTP client
- `tokio-tungstenite`: WebSocket client
- `hmac` + `sha2`: Authentication
- `serde` + `serde_json`: Serialization

### Exchange-Specific SDKs
Consider using official SDKs where available:
- Some exchanges provide Rust SDKs
- Others have well-documented REST/WS APIs
- Evaluate maintenance and compatibility