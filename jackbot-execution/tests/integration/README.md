# Jackbot Cross-Component Integration Tests

This directory contains comprehensive integration tests for validating the end-to-end functionality of the Jackbot Bloomberg Terminal killer system.

## Overview

The integration test suite validates the complete data flow and functionality across all three major Jackbot components:

```
jackbot-terminal (Flutter) ←→ jackbot-backend (Rust) ←→ jackbot-sensor (Rust) ←→ Exchanges
```

## Test Categories

### 1. Market Data Flow Tests (`market_data_flow.rs`)
- **Objective**: Validate end-to-end market data pipeline
- **Flow**: Exchange → Sensor → Backend → Terminal
- **Performance Target**: <100ms end-to-end latency
- **Validation**: Data integrity, real-time updates, error handling

### 2. Order Execution Tests (`order_execution_flow.rs`)
- **Objective**: Validate complete order lifecycle
- **Flow**: Terminal → Backend → Sensor → Exchange → Confirmations back
- **Performance Target**: <1000ms total execution time
- **Scenarios**: Market orders, limit orders, smart orders (TWAP/Iceberg), portfolio rebalancing

### 3. Performance Validation Tests (`performance_tests.rs`)
- **Objective**: Validate system performance under various conditions
- **Categories**: Latency, throughput, stress, memory leak, concurrency, stability
- **Targets**:
  - Market data: <100ms latency
  - Order execution: <1000ms
  - WebSocket updates: <50ms
  - Database queries: <50ms
  - Throughput: 100 orders/sec, 1M messages/hour
  - Concurrent connections: 10,000
  - 24-hour stability (shortened to 2 minutes for testing)

### 4. Functional Integration Tests (`functional_tests.rs`)
- **Objective**: Validate cross-component functionality
- **Categories**: Authentication, portfolio sync, error handling, smart orders, risk management, data integrity
- **Success Criteria**: 90-99% accuracy depending on category

### 5. Infrastructure Tests (`infrastructure.rs`)
- **Objective**: Mock services for isolated testing
- **Components**: Mock exchange server, Kafka environment, database setup

### 6. Mock Services (`mock_services.rs`)
- **Objective**: Comprehensive mock implementations
- **Services**: Kafka broker, PostgreSQL database, Redis cache

## Test Architecture

### Mock Exchange Server
```rust
// WebSocket market data streaming
// REST API order operations
// Realistic latencies and responses
// Error scenarios and edge cases
```

### Test Environment
- **Kafka**: Isolated message broker for real-time data
- **PostgreSQL**: Test database with transaction rollback
- **Redis**: Cache testing with TTL and eviction
- **Docker Compose**: Orchestrated test environment

### Performance Monitoring
- Real-time metrics collection
- Memory usage tracking
- CPU utilization monitoring
- Network throughput measurement
- Error rate calculation

## Running Tests

### Prerequisites
1. **Rust**: 1.75 or later
2. **Docker**: For containerized test environment
3. **Docker Compose**: For service orchestration

### Environment Setup

#### Option 1: Docker Compose (Recommended)
```bash
# Start test environment
cd tests/integration
docker-compose -f docker-compose.test.yml up -d

# Wait for services to be ready
docker-compose -f docker-compose.test.yml logs -f integration-tests

# Run tests
docker-compose -f docker-compose.test.yml exec integration-tests cargo test

# Cleanup
docker-compose -f docker-compose.test.yml down -v
```

#### Option 2: Local Services
```bash
# Set environment variables
export TEST_KAFKA_BROKERS="localhost:9092"
export TEST_DATABASE_URL="postgres://test:test@localhost:5433/jackbot_test"
export TEST_REDIS_URL="redis://localhost:6380"
export TEST_SENSOR_ENDPOINT="http://localhost:8081"
export TEST_BACKEND_ENDPOINT="http://localhost:8080"
export TEST_TERMINAL_ENDPOINT="ws://localhost:8082"

# Run specific test categories
cargo test --package jackbot-execution --test integration_test test_market_data_flow_only
cargo test --package jackbot-execution --test integration_test test_order_execution_flow_only
cargo test --package jackbot-execution --test integration_test test_performance_validation_only
cargo test --package jackbot-execution --test integration_test test_functional_integration_only

# Run all integration tests
cargo test --package jackbot-execution --test integration_test run_comprehensive_integration_tests
```

### Test Configuration

Environment variables for customizing test behavior:

```bash
# Service endpoints
TEST_SENSOR_ENDPOINT="http://localhost:8081"
TEST_BACKEND_ENDPOINT="http://localhost:8080"
TEST_TERMINAL_ENDPOINT="ws://localhost:8082"
TEST_KAFKA_BROKERS="localhost:9092"
TEST_DATABASE_URL="postgres://test:test@localhost:5433/jackbot_test"
TEST_REDIS_URL="redis://localhost:6380"

# Performance targets
TEST_MARKET_DATA_LATENCY_TARGET=100      # milliseconds
TEST_ORDER_EXECUTION_LATENCY_TARGET=1000 # milliseconds
TEST_WEBSOCKET_UPDATE_LATENCY_TARGET=50  # milliseconds
TEST_DATABASE_QUERY_LATENCY_TARGET=50    # milliseconds
TEST_THROUGHPUT_ORDERS_TARGET=100        # orders per second
TEST_CONCURRENT_CONNECTIONS_TARGET=10000 # concurrent connections
TEST_MESSAGES_PER_HOUR_TARGET=1000000    # messages per hour

# Test execution
TEST_TIMEOUT_SECONDS=30                  # test timeout
TEST_MOCK_EXCHANGE_PORT=8090            # mock exchange port
```

## Test Results and Reporting

### Output Formats
1. **Console Output**: Real-time test progress and results
2. **JSON Report**: Detailed metrics and performance data (`integration_test_report.json`)
3. **Prometheus Metrics**: Time-series performance data
4. **Grafana Dashboards**: Visual performance analysis

### Success Criteria
- **Market Data Flow**: <100ms latency, >99% data integrity
- **Order Execution**: <1000ms execution time, >99% accuracy
- **Performance**: Meet all latency and throughput targets
- **Functional**: >90% test pass rate (varies by category)
- **Stability**: 24-hour operation without memory leaks or crashes

### Performance Baselines
- **Latency Percentiles**: P50, P90, P95, P99 measurements
- **Throughput Metrics**: Messages/second, orders/second
- **Resource Usage**: Memory, CPU, network utilization
- **Error Rates**: <1% for market data, <0.1% for orders

## Test Scenarios

### Market Data Scenarios
1. **Normal Operation**: Steady market data flow
2. **High Volatility**: Burst market data with price spikes
3. **Network Issues**: Intermittent connectivity problems
4. **Exchange Downtime**: Graceful handling of exchange outages

### Order Execution Scenarios
1. **Simple Orders**: Market and limit orders
2. **Smart Orders**: TWAP, VWAP, Iceberg execution
3. **Error Conditions**: Invalid parameters, insufficient balance
4. **Portfolio Rebalancing**: Multi-asset coordinated trading

### Stress Test Scenarios
1. **Connection Stress**: 10,000 concurrent WebSocket connections
2. **Message Flood**: 1M+ messages per hour processing
3. **Memory Pressure**: Extended operation under memory constraints
4. **CPU Saturation**: High computational load scenarios

## Troubleshooting

### Common Issues

#### Test Environment Setup
```bash
# Check Docker services status
docker-compose -f docker-compose.test.yml ps

# View service logs
docker-compose -f docker-compose.test.yml logs kafka
docker-compose -f docker-compose.test.yml logs postgres-test
docker-compose -f docker-compose.test.yml logs redis-test

# Test service connectivity
docker-compose -f docker-compose.test.yml exec integration-tests nc -z kafka 9092
docker-compose -f docker-compose.test.yml exec integration-tests nc -z postgres-test 5432
```

#### Performance Issues
```bash
# Monitor resource usage
docker stats

# Check for memory leaks
docker-compose -f docker-compose.test.yml exec integration-tests ps aux

# Analyze test results
cat test-results/integration_test_report.json | jq '.[] | select(.success == false)'
```

#### Network Connectivity
```bash
# Test mock exchange connectivity
curl http://localhost:8090/api/v1/exchangeInfo

# Test WebSocket connections
wscat -c ws://localhost:8090

# Verify Kafka topics
docker-compose -f docker-compose.test.yml exec kafka kafka-topics --list --bootstrap-server localhost:9092
```

### Known Limitations
1. **Shortened Stability Test**: 2 minutes instead of 24 hours
2. **Simplified Error Injection**: Limited failure scenarios
3. **Mock Exchange Behavior**: Simplified compared to real exchanges
4. **Network Simulation**: Limited network condition simulation

### Performance Tuning
1. **Increase Docker Resources**: More CPU/memory for containers
2. **Parallel Test Execution**: Use `--test-threads` parameter
3. **Selective Testing**: Run specific test categories
4. **Performance Targets**: Adjust targets via environment variables

## CI/CD Integration

### GitHub Actions Example
```yaml
name: Integration Tests
on: [push, pull_request]

jobs:
  integration-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Build test environment
        run: |
          cd tests/integration
          docker-compose -f docker-compose.test.yml build
      - name: Run integration tests
        run: |
          cd tests/integration
          docker-compose -f docker-compose.test.yml up --exit-code-from integration-tests
      - name: Collect test results
        if: always()
        uses: actions/upload-artifact@v3
        with:
          name: integration-test-results
          path: tests/integration/test-results/
```

### Test Metrics Collection
```bash
# Export test metrics to Prometheus
curl http://localhost:9090/api/v1/query?query=jackbot_test_latency_seconds

# Generate performance report
docker-compose -f docker-compose.test.yml exec grafana-test \
  curl -X POST http://admin:test@localhost:3000/api/dashboards/export/jackbot-performance
```

## Contributing

### Adding New Tests
1. **Create Test Module**: Add new `.rs` file in `tests/integration/`
2. **Update Module Declaration**: Add to `mod.rs`
3. **Implement Test Functions**: Follow existing patterns
4. **Add Documentation**: Update this README
5. **Update Docker Compose**: Add any new service dependencies

### Test Best Practices
1. **Isolation**: Each test should be independent
2. **Cleanup**: Proper resource cleanup after tests
3. **Timing**: Use appropriate timeouts and delays
4. **Logging**: Comprehensive test logging for debugging
5. **Metrics**: Collect relevant performance metrics

### Performance Test Guidelines
1. **Realistic Loads**: Use production-like data volumes
2. **Resource Monitoring**: Track CPU, memory, network usage
3. **Error Handling**: Test failure scenarios
4. **Baseline Measurement**: Establish performance baselines
5. **Regression Detection**: Compare against historical performance

## Future Enhancements

### Planned Improvements
1. **Real Exchange Integration**: Optional real exchange testing
2. **Advanced Network Simulation**: Packet loss, jitter, partitions
3. **Chaos Engineering**: Systematic failure injection
4. **Load Testing Framework**: Scalable load generation
5. **Visual Test Reports**: Enhanced HTML reporting
6. **Automated Performance Regression**: Historical comparison
7. **Cross-Platform Testing**: Windows, macOS, Linux validation

### Monitoring Integration
1. **APM Integration**: New Relic, Datadog integration
2. **Alerting**: Automated alerts for test failures
3. **Dashboards**: Real-time test execution monitoring
4. **Historical Trends**: Long-term performance tracking

---

For questions or issues with the integration tests, please refer to the troubleshooting section above or create an issue in the project repository.