# Jackbot Sensor Deployment

This directory contains deployment configurations and scripts for the Jackbot sensor system.

## Quick Start

### Local Development

1. **Start infrastructure**:
```bash
docker-compose -f infrastructure/docker-compose.yml up -d
```

2. **Build and run sensor**:
```bash
docker-compose -f docker-compose.sensor.yml up --build
```

3. **Test deployment**:
```bash
./scripts/test-sensor-local.sh
```

### Production Deployment

1. **Configure AWS credentials**:
```bash
export AWS_REGION=us-east-1
export AWS_ACCESS_KEY_ID=your_key_id
export AWS_SECRET_ACCESS_KEY=your_secret_key
```

2. **Deploy sensor image**:
```bash
./scripts/deploy-sensor.sh --env prod --push --registry your-ecr-registry
```

3. **Deploy management Lambda**:
```bash
cd ../jackbot-backend/lambdas/sensor-management
cargo lambda build --release --arm64
# Deploy using SAM or terraform
```

## Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   Management    │────▶│   ECS Fargate    │────▶│    Exchange     │
│     Lambda      │     │     Sensors      │     │   WebSockets    │
└─────────────────┘     └──────────────────┘     └─────────────────┘
         │                       │
         │                       ▼
         │              ┌──────────────────┐
         │              │      Kafka       │
         │              │    Streaming     │
         │              └──────────────────┘
         │                       │
         ▼                       ▼
┌─────────────────┐     ┌──────────────────┐
│   CloudWatch    │     │     Backend      │
│    Metrics      │     │    Services      │
└─────────────────┘     └──────────────────┘
```

## Key Features

- **Multi-exchange support**: Binance, Coinbase, Bybit, OKX, Kraken, etc.
- **High performance**: <10ms latency, 10K+ messages/second
- **Horizontal scaling**: Deploy multiple sensors per exchange
- **Health monitoring**: Real-time health checks and metrics
- **Auto-recovery**: Circuit breakers and automatic reconnection

## Configuration

See `sensor-config-prod.toml` for production configuration options.

Key environment variables:
- `KAFKA_BROKERS`: Kafka broker addresses
- `REDIS_URL`: Redis connection URL
- `EXCHANGE`: Target exchange name
- `SYMBOLS`: Comma-separated trading symbols

## Monitoring

Access monitoring dashboards:
- **Grafana**: http://localhost:3001 (local)
- **Prometheus**: http://localhost:9091 (local)
- **CloudWatch**: AWS Console (production)

## Troubleshooting

See the [Sensor Deployment Guide](../../docs/layer-3-specs/sensor-deployment.md) for detailed troubleshooting steps.