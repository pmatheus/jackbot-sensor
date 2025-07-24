# Jackbot Sensor Setup Guide

## Prerequisites

1. **Rust & Cargo** (1.70+)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Docker & Docker Compose**
   - Install Docker Desktop from https://www.docker.com/products/docker-desktop

3. **Exchange API Keys** (for production)
   - Create testnet/sandbox accounts for development
   - Store credentials securely in environment variables

## Quick Start

### 1. Start Infrastructure

```bash
cd infrastructure
./start-sensor.sh
```

This starts:
- Kafka (message broker) on port 9092
- Redis (cache) on port 6380
- PostgreSQL (state storage) on port 5434
- Prometheus (metrics) on port 9091
- Grafana (dashboards) on port 3001
- Kafka UI on port 8081

### 2. Build the Sensor

```bash
# From the jackbot-sensor directory
cargo build --release
```

### 3. Run the Sensor

#### Development Mode (Testnet)
```bash
# Start with default exchanges (binance, coinbase, bybit)
cargo run -- start

# Or specify exchanges
cargo run -- start --exchanges binance,coinbase

# With debug logging
cargo run -- start --debug
```

#### Production Mode
```bash
# Set environment variables first
export BINANCE_API_KEY="your-api-key"
export BINANCE_API_SECRET="your-api-secret"
export COINBASE_API_KEY="your-api-key"
export COINBASE_API_SECRET="your-api-secret"
export COINBASE_PASSPHRASE="your-passphrase"

# Run in production mode
cargo run --release -- start --production
```

#### MVP Mode (Simple Kafka Testing)
```bash
cargo run -- mvp
```

## Configuration

Edit `sensor-config.toml` to:
- Enable/disable exchanges
- Configure symbols to monitor
- Adjust rate limits
- Set risk parameters

## Environment Variables

### Exchange Credentials
```bash
# Binance
BINANCE_API_KEY=
BINANCE_API_SECRET=

# Coinbase
COINBASE_API_KEY=
COINBASE_API_SECRET=
COINBASE_PASSPHRASE=

# Bybit
BYBIT_API_KEY=
BYBIT_API_SECRET=

# Add more as needed...
```

### Infrastructure
```bash
KAFKA_BROKERS=localhost:9092
REDIS_URL=redis://:jackbot_sensor_redis_password@localhost:6380
DATABASE_URL=postgresql://sensor:sensor_password@localhost:5434/jackbot_sensor
```

## Monitoring

### Grafana Dashboards
1. Open http://localhost:3001
2. Login with admin/admin
3. Import dashboards from `infrastructure/grafana/dashboards/`

### Kafka UI
1. Open http://localhost:8081
2. View topics, messages, and consumer groups

### Prometheus Metrics
1. Open http://localhost:9091
2. Query sensor metrics

## Testing

### Unit Tests
```bash
cargo test
```

### Integration Tests
```bash
# Start infrastructure first
cd infrastructure && ./start-sensor.sh

# Run integration tests
cargo test --features integration
```

### Performance Tests
```bash
cargo bench
```

## Troubleshooting

### Common Issues

1. **Port conflicts**
   - Check if ports are already in use
   - Modify port mappings in `docker-compose.yml`

2. **WebSocket connection errors**
   - Verify exchange URLs in config
   - Check firewall/proxy settings
   - Ensure API credentials are valid

3. **Kafka connection issues**
   - Verify Kafka is running: `docker ps`
   - Check logs: `docker logs jackbot-sensor-kafka`

4. **High memory usage**
   - Adjust buffer sizes in config
   - Reduce number of symbols monitored
   - Check for memory leaks with `valgrind`

### Debug Commands

```bash
# View all logs
docker-compose logs -f

# Check specific service
docker logs jackbot-sensor-kafka

# Test Kafka connectivity
docker exec -it jackbot-sensor-kafka kafka-topics --list --bootstrap-server localhost:9092

# Test Redis connectivity
docker exec -it jackbot-sensor-redis redis-cli -a jackbot_sensor_redis_password ping

# Test PostgreSQL connectivity
docker exec -it jackbot-sensor-postgres psql -U sensor -d jackbot_sensor -c "SELECT 1"
```

## Development Workflow

1. **Make changes** to Rust code
2. **Run tests**: `cargo test`
3. **Build**: `cargo build`
4. **Run locally**: `cargo run -- start`
5. **Check metrics**: http://localhost:9091
6. **View dashboards**: http://localhost:3001

## Production Deployment

See [DEPLOYMENT.md](docs/DEPLOYMENT.md) for production deployment instructions.

## Support

- GitHub Issues: https://github.com/pmatheus/jackbot-sensor/issues
- Documentation: [docs/](docs/)