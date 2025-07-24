#!/bin/bash

# Start infrastructure services for Jackbot Sensor

set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

echo "🚀 Starting Jackbot Sensor Infrastructure..."

# Check prerequisites
command -v docker >/dev/null 2>&1 || { echo "❌ Docker is required but not installed. Aborting." >&2; exit 1; }

# Create necessary directories
mkdir -p "$SCRIPT_DIR/grafana/dashboards"
mkdir -p "$SCRIPT_DIR/grafana/datasources"

# Create Grafana datasource configuration
cat > "$SCRIPT_DIR/grafana/datasources/prometheus.yml" << EOF
apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
    isDefault: true
    editable: true
EOF

# Start Docker services
echo "🐳 Starting Docker services..."
cd "$SCRIPT_DIR"
docker-compose up -d

# Wait for services to be ready
echo "⏳ Waiting for services to be ready..."

# Wait for PostgreSQL
until docker exec jackbot-sensor-postgres pg_isready -U sensor >/dev/null 2>&1; do
    echo "   Waiting for PostgreSQL..."
    sleep 2
done
echo "✅ PostgreSQL is ready"

# Wait for Redis
until docker exec jackbot-sensor-redis redis-cli -a jackbot_sensor_redis_password ping >/dev/null 2>&1; do
    echo "   Waiting for Redis..."
    sleep 2
done
echo "✅ Redis is ready"

# Wait for Kafka
echo "   Waiting for Kafka to be ready..."
sleep 10

# Create Kafka topics
echo "📊 Creating Kafka topics..."
docker exec jackbot-sensor-kafka kafka-topics --create --if-not-exists \
    --bootstrap-server localhost:9092 \
    --topic market-data \
    --partitions 10 \
    --replication-factor 1 \
    --config retention.ms=604800000 \
    --config compression.type=zstd || true

docker exec jackbot-sensor-kafka kafka-topics --create --if-not-exists \
    --bootstrap-server localhost:9092 \
    --topic orders \
    --partitions 5 \
    --replication-factor 1 \
    --config retention.ms=2592000000 || true

docker exec jackbot-sensor-kafka kafka-topics --create --if-not-exists \
    --bootstrap-server localhost:9092 \
    --topic trades \
    --partitions 5 \
    --replication-factor 1 \
    --config retention.ms=2592000000 || true

echo "✅ Kafka is ready with topics created"

echo ""
echo "✅ All infrastructure services started successfully!"
echo ""
echo "📊 Service URLs:"
echo "  - Kafka: localhost:9092"
echo "  - Kafka UI: http://localhost:8081"
echo "  - Redis: localhost:6380 (pass: jackbot_sensor_redis_password)"
echo "  - PostgreSQL: localhost:5434 (user: sensor, pass: sensor_password)"
echo "  - Prometheus: http://localhost:9091"
echo "  - Grafana: http://localhost:3001 (admin/admin)"
echo ""
echo "💡 Tips:"
echo "  - Use 'docker-compose logs -f' to view service logs"
echo "  - Run 'cargo run -- start' to start the sensor"
echo "  - Press Ctrl+C to stop all services"
echo ""

# Keep script running
wait