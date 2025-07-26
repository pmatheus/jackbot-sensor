# Phase 4 Testing Quick Start Guide

**Version:** 1.0.0  
**Date:** 2025-07-26  
**Target:** 15% Test Coverage on MVP Critical Paths

## Quick Reference

### Test Execution Commands

```bash
# Run all tests
cargo test --workspace

# Run unit tests only
cargo test --workspace --lib

# Run integration tests
cargo test --workspace --test '*' -- --ignored

# Run security tests
cargo test --workspace --features security

# Run with coverage report
cargo tarpaulin --workspace --out Html

# Run specific test module
cargo test market_data_tests

# Run tests with output
cargo test -- --nocapture
```

### Test Categories & Priorities

| Category | Priority | Target Coverage | Files to Create |
|----------|----------|-----------------|-----------------|
| Market Data Pipeline | P0 | 20% | `tests/market_data_tests.rs` |
| Order Execution | P0 | 30% | `tests/order_tests.rs` |
| Portfolio Calculations | P0 | 25% | `tests/portfolio_tests.rs` |
| Security Tests | P0 | 10% | `tests/security_tests.rs` |
| Integration Tests | P1 | 15% | `tests/integration/*.rs` |

## MVP Critical Paths

### 1. Market Data Display
```
WebSocket → Kafka → Aggregator → Redis → API → Frontend
```

**Key Tests:**
- WebSocket connection resilience
- Order book aggregation accuracy
- Real-time update latency (<10ms)

### 2. Portfolio View
```
Exchange APIs → Balance Aggregation → P&L Calculation → Display
```

**Key Tests:**
- Multi-exchange balance sync
- P&L calculation accuracy
- Real-time portfolio updates

### 3. Order Placement
```
UI → Validation → Risk Check → Exchange → Confirmation → Portfolio Update
```

**Key Tests:**
- Order validation rules
- Risk limit enforcement
- Execution confirmation flow

## Test Structure

```
jackbot-sensor/
├── tests/
│   ├── market_data_tests.rs      # Market data unit tests
│   ├── order_tests.rs            # Order execution tests
│   ├── portfolio_tests.rs        # Portfolio calculation tests
│   ├── security_tests.rs         # Security vulnerability tests
│   └── integration/
│       ├── mod.rs                # Integration test module
│       ├── exchange_integration.rs
│       ├── service_integration.rs
│       └── e2e_scenarios.rs
├── tests/fixtures/               # Test data and mocks
│   ├── market_data.json
│   ├── orders.json
│   └── portfolios.json
└── .github/workflows/
    └── test.yml                  # CI pipeline configuration
```

## Creating Your First Test

### Step 1: Create Test File
```rust
// tests/market_data_tests.rs
use jackbot_sensor::*;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_websocket_connection() {
        // Your test here
    }
}
```

### Step 2: Run Test
```bash
cargo test market_data_tests
```

### Step 3: Check Coverage
```bash
cargo tarpaulin --workspace --out Html
open tarpaulin-report.html
```

## Security Testing Checklist

- [ ] API key encryption validation
- [ ] Input sanitization (SQL injection)
- [ ] Rate limiting enforcement
- [ ] Session management security
- [ ] TLS/SSL configuration
- [ ] Audit logging completeness

## Integration Testing Setup

### 1. Start Test Environment
```bash
docker-compose -f infrastructure/docker-compose.yml up -d
```

### 2. Run Integration Tests
```bash
cargo test --workspace --test '*' -- --ignored
```

### 3. Clean Up
```bash
docker-compose -f infrastructure/docker-compose.yml down -v
```

## Performance Benchmarks

| Operation | Target | Maximum |
|-----------|--------|---------|
| Market Data Update | <10ms | 50ms |
| Order Submission | <100ms | 200ms |
| Portfolio Calculation | <50ms | 100ms |
| WebSocket Message | <5ms | 20ms |

## Common Test Patterns

### Mock Exchange Client
```rust
let mock_exchange = MockExchangeClient::new()
    .with_latency(Duration::milliseconds(50))
    .with_failure_rate(0.001);
```

### Test Database
```rust
let db = Database::new_test().await;
// Automatically cleaned up after test
```

### WebSocket Testing
```rust
let ws_client = MockWebSocketClient::new();
ws_client.emit_message(message);
let received = ws_client.receive_message().await?;
```

## Troubleshooting

### Test Timeouts
```rust
#[tokio::test]
#[timeout(30000)] // 30 seconds
async fn long_running_test() {
    // Test code
}
```

### Flaky Tests
```rust
// Add retry logic for network-dependent tests
for attempt in 0..3 {
    if let Ok(result) = connect_to_exchange().await {
        break;
    }
    tokio::time::sleep(Duration::seconds(1)).await;
}
```

### Test Isolation
```rust
// Use unique identifiers for each test
let test_id = Uuid::new_v4().to_string();
let user_id = format!("test_user_{}", test_id);
```

## CI/CD Integration

```yaml
# .github/workflows/test.yml
name: Test Suite

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Run tests
        run: cargo test --workspace
      - name: Generate coverage
        run: cargo tarpaulin --out Xml
      - name: Upload coverage
        uses: codecov/codecov-action@v3
```

## Next Actions

1. **Create test files** in the `tests/` directory
2. **Implement P0 tests** first (Market Data, Orders, Portfolio)
3. **Run tests locally** before pushing
4. **Monitor coverage** - aim for 15% on critical paths
5. **Document failures** in test comments

## Resources

- [Testing Specification](./testing-specification.md)
- [Security Testing Guide](./security-testing-spec.md)
- [Integration Testing Priorities](./integration-testing-priorities.md)
- [Rust Testing Book](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Tokio Testing](https://tokio.rs/tokio/topics/testing)