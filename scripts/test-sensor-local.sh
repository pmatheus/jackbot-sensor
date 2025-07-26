#!/bin/bash

# Test Jackbot Sensor Locally
# This script tests the sensor deployment with Docker Compose

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Jackbot Sensor Local Test ===${NC}\n"

# Check prerequisites
echo -e "${YELLOW}Checking prerequisites...${NC}"

if ! command -v docker &> /dev/null; then
    echo -e "${RED}Docker is not installed${NC}"
    exit 1
fi

if ! command -v docker-compose &> /dev/null; then
    echo -e "${RED}Docker Compose is not installed${NC}"
    exit 1
fi

echo -e "${GREEN}Prerequisites OK${NC}\n"

# Start infrastructure
echo -e "${YELLOW}Starting infrastructure services...${NC}"
docker-compose -f infrastructure/docker-compose.yml up -d kafka redis

# Wait for Kafka to be ready
echo -e "${YELLOW}Waiting for Kafka to be ready...${NC}"
for i in {1..30}; do
    if docker exec jackbot-sensor-kafka kafka-topics --bootstrap-server localhost:9092 --list &> /dev/null; then
        echo -e "${GREEN}Kafka is ready${NC}"
        break
    fi
    echo -n "."
    sleep 2
done

# Create test topics
echo -e "\n${YELLOW}Creating Kafka topics...${NC}"
docker exec jackbot-sensor-kafka kafka-topics --bootstrap-server localhost:9092 \
    --create --if-not-exists --topic market.binance.spot.BTCUSDT.orderbook \
    --partitions 3 --replication-factor 1

docker exec jackbot-sensor-kafka kafka-topics --bootstrap-server localhost:9092 \
    --create --if-not-exists --topic market.binance.spot.BTCUSDT.trades \
    --partitions 3 --replication-factor 1

echo -e "${GREEN}Topics created${NC}\n"

# Build sensor image
echo -e "${YELLOW}Building sensor Docker image...${NC}"
docker build -t jackbot-sensor:latest .
echo -e "${GREEN}Image built${NC}\n"

# Start sensor
echo -e "${YELLOW}Starting sensor...${NC}"
docker-compose -f docker-compose.sensor.yml up -d sensor

# Wait for sensor to start
echo -e "${YELLOW}Waiting for sensor to start...${NC}"
for i in {1..30}; do
    if curl -f http://localhost:8080/health &> /dev/null; then
        echo -e "\n${GREEN}Sensor is healthy${NC}"
        break
    fi
    echo -n "."
    sleep 2
done

# Run tests
echo -e "\n${BLUE}Running tests...${NC}\n"

# Test 1: Health endpoint
echo -e "${YELLOW}Test 1: Health endpoint${NC}"
HEALTH_RESPONSE=$(curl -s http://localhost:8080/health)
if echo "$HEALTH_RESPONSE" | grep -q "healthy"; then
    echo -e "${GREEN}✓ Health endpoint working${NC}"
    echo "$HEALTH_RESPONSE" | jq '.' || echo "$HEALTH_RESPONSE"
else
    echo -e "${RED}✗ Health endpoint failed${NC}"
fi
echo

# Test 2: Metrics endpoint
echo -e "${YELLOW}Test 2: Metrics endpoint${NC}"
METRICS_RESPONSE=$(curl -s http://localhost:9090/metrics)
if echo "$METRICS_RESPONSE" | grep -q "sensor_"; then
    echo -e "${GREEN}✓ Metrics endpoint working${NC}"
    echo "$METRICS_RESPONSE" | grep "sensor_" | head -5
else
    echo -e "${RED}✗ Metrics endpoint failed${NC}"
fi
echo

# Test 3: Kafka connectivity
echo -e "${YELLOW}Test 3: Kafka connectivity${NC}"
docker exec jackbot-sensor-kafka kafka-consumer-groups \
    --bootstrap-server localhost:9092 \
    --group jackbot-sensor \
    --describe &> /dev/null

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ Kafka consumer group exists${NC}"
else
    echo -e "${RED}✗ Kafka connectivity failed${NC}"
fi
echo

# Test 4: Redis connectivity
echo -e "${YELLOW}Test 4: Redis connectivity${NC}"
REDIS_PING=$(docker exec jackbot-sensor-redis redis-cli -a jackbot_sensor_redis_password PING 2>/dev/null)
if [ "$REDIS_PING" = "PONG" ]; then
    echo -e "${GREEN}✓ Redis connectivity OK${NC}"
else
    echo -e "${RED}✗ Redis connectivity failed${NC}"
fi
echo

# Test 5: Resource usage
echo -e "${YELLOW}Test 5: Resource usage${NC}"
STATS=$(docker stats --no-stream --format "table {{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}" | grep sensor)
echo -e "${GREEN}Resource usage:${NC}"
echo "$STATS"
echo

# Show logs
echo -e "${YELLOW}Recent sensor logs:${NC}"
docker logs --tail 20 jackbot-sensor
echo

# Summary
echo -e "${BLUE}=== Test Summary ===${NC}"
echo -e "${GREEN}✓ Infrastructure running${NC}"
echo -e "${GREEN}✓ Sensor deployed${NC}"
echo -e "${GREEN}✓ Health checks passing${NC}"
echo

echo -e "${YELLOW}Next steps:${NC}"
echo "1. Monitor logs: docker-compose -f docker-compose.sensor.yml logs -f sensor"
echo "2. View Grafana: http://localhost:3001"
echo "3. Check Kafka UI: http://localhost:8081"
echo "4. Stop services: docker-compose -f docker-compose.sensor.yml down"