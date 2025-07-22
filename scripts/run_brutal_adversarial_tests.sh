#!/bin/bash

# BRUTAL ADVERSARIAL TEST RUNNER
# Performance torture testing for jackbot-sensor
# Zero tolerance for weakness!

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
REPORT_FILE="$PROJECT_ROOT/BRUTAL_ADVERSARIAL_TEST_REPORT_$(date +%Y%m%d_%H%M%S).md"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${PURPLE}🔥 JACKBOT SENSOR BRUTAL ADVERSARIAL TEST SUITE 🔥${NC}"
echo -e "${PURPLE}=================================================${NC}"
echo -e "${YELLOW}Testing ALL 11 exchanges with ZERO TOLERANCE!${NC}"
echo ""

# Start report
cat > "$REPORT_FILE" << EOF
# BRUTAL ADVERSARIAL TEST REPORT
Generated: $(date)

## Performance Requirements
- <10ms order book processing (P99)
- <10ms arbitrage detection
- 1M messages/second throughput
- Zero data loss
- <100MB memory usage
- All 11 exchanges connected

## Exchanges Under Test
1. Binance
2. Coinbase
3. Bybit
4. Bitget
5. Hyperliquid
6. KuCoin
7. Kraken
8. OKX
9. Gate.io (NEW)
10. MEXC (NEW)
11. BingX (NEW)

## Test Results

EOF

# Function to run a test and capture results
run_test() {
    local test_name=$1
    local test_function=$2
    
    echo -e "${CYAN}Running: $test_name${NC}"
    echo "### $test_name" >> "$REPORT_FILE"
    echo "" >> "$REPORT_FILE"
    
    # Run test with timeout
    if timeout 600 cargo test --test brutal_11_exchange_adversarial_tests "$test_function" -- --nocapture 2>&1 | tee -a test_output.tmp; then
        echo -e "${GREEN}✅ PASSED${NC}"
        echo "**Status**: ✅ PASSED" >> "$REPORT_FILE"
    else
        echo -e "${RED}❌ FAILED${NC}"
        echo "**Status**: ❌ FAILED" >> "$REPORT_FILE"
    fi
    
    # Extract key metrics from output
    grep -E "(P99|latency|Success Rate|Throughput|Memory)" test_output.tmp | tail -20 >> "$REPORT_FILE" || true
    echo "" >> "$REPORT_FILE"
    
    rm -f test_output.tmp
}

# Check if we're in release mode
echo -e "${BLUE}Building in release mode for maximum performance...${NC}"
cd "$PROJECT_ROOT"
cargo build --release --tests

echo ""
echo -e "${YELLOW}Starting brutal performance tests...${NC}"
echo ""

# Run each attack vector
run_test "ATTACK 1: Catastrophic Network Failure" "test_catastrophic_network_failure"
sleep 2

run_test "ATTACK 2: Million Messages Bombardment" "test_million_messages_bombardment"
sleep 2

run_test "ATTACK 3: Memory Leak Hunting" "test_memory_leak_hunting"
sleep 2

run_test "ATTACK 4: CPU Hot Spot Detection" "test_cpu_hotspot_detection"
sleep 2

run_test "ATTACK 5: Data Corruption Detection" "test_data_corruption_detection"
sleep 2

run_test "ATTACK 6: Order Book Aggregator Stress" "test_order_book_aggregator_stress"
sleep 2

run_test "ATTACK 7: Arbitrage Detection Accuracy" "test_arbitrage_detection_accuracy"
sleep 2

# Summary
echo ""
echo -e "${PURPLE}=================================================${NC}"
echo -e "${PURPLE}TEST SUITE COMPLETE${NC}"
echo -e "${PURPLE}=================================================${NC}"

# Generate summary
cat >> "$REPORT_FILE" << EOF

## Summary

All 11 exchanges have been subjected to:
- Network failure simulation
- 1M messages/second bombardment
- Memory leak detection
- CPU hot spot analysis
- Data corruption attempts
- Order book aggregation stress
- Arbitrage detection under load

### Performance Validation
- P99 Latency: Must be <10ms
- Throughput: Must exceed 1M msgs/sec
- Memory Usage: Must stay under 100MB
- Success Rate: Must be >99.9%

### Recommendations
1. Monitor Gate.io, MEXC, and BingX closely as new additions
2. Implement circuit breakers for network failures
3. Add memory pooling for high-frequency allocations
4. Profile CPU hot spots in production
5. Validate all external data inputs

EOF

echo ""
echo -e "${GREEN}Report saved to: $REPORT_FILE${NC}"
echo ""

# Run quick connectivity test for new exchanges
echo -e "${YELLOW}Running quick connectivity test for new exchanges...${NC}"
cargo test --test new_exchanges_integration_test test_all_11_exchanges_parallel_connection -- --nocapture

echo ""
echo -e "${PURPLE}🔥 BRUTAL TESTING COMPLETE! 🔥${NC}"
echo -e "${YELLOW}If the system survived this, it can handle ANYTHING!${NC}"

# Optional: Run 24-hour test if requested
if [[ "${1:-}" == "--endurance" ]]; then
    echo ""
    echo -e "${RED}💀 STARTING 24-HOUR ENDURANCE TEST! 💀${NC}"
    echo -e "${RED}This will run for 24 HOURS with maximum stress!${NC}"
    read -p "Are you ABSOLUTELY sure? (yes/no): " confirm
    
    if [[ "$confirm" == "yes" ]]; then
        cargo test --release --test brutal_11_exchange_adversarial_tests test_24_hour_endurance -- --ignored --nocapture
    fi
fi