#!/bin/bash

#============================================================================
# JACKBOT SENSOR ADVERSARIAL TEST SUITE
# Comprehensive torture testing and performance validation
#============================================================================

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Performance targets
MARKET_DATA_LATENCY_TARGET_MS=10
ORDER_EXECUTION_LATENCY_TARGET_MS=50
THROUGHPUT_TARGET_MPS=1000000
SUCCESS_RATE_TARGET_PCT=99.9
MEMORY_LEAK_TOLERANCE_MB=100

# Test categories
CATEGORY="${1:-all}"
EXTENDED="${2:-}"
APOCALYPSE="${3:-}"

echo -e "${BLUE}🔥 JACKBOT SENSOR ADVERSARIAL TEST SUITE 🔥${NC}"
echo "Category: $CATEGORY"
echo "Extended: ${EXTENDED:-false}"
echo "Apocalypse: ${APOCALYPSE:-false}"
echo "========================================="

# Change to sensor directory
cd "$(dirname "$0")/.."

# Ensure project builds
echo -e "${YELLOW}📦 Building project...${NC}"
cargo build --release --all-features || {
    echo -e "${RED}❌ Build failed!${NC}"
    exit 1
}

# Function to run test category
run_test_category() {
    local test_name="$1"
    local test_command="$2"
    
    echo -e "\n${BLUE}🏃 Running $test_name...${NC}"
    
    if eval "$test_command"; then
        echo -e "${GREEN}✅ $test_name PASSED${NC}"
        return 0
    else
        echo -e "${RED}❌ $test_name FAILED${NC}"
        return 1
    fi
}

# Function to run performance benchmarks
run_performance_tests() {
    echo -e "\n${BLUE}⚡ PERFORMANCE TORTURE TESTS${NC}"
    
    # Market data processing latency test
    run_test_category "Market Data Latency" "cargo test --release test_market_data_processing_latency --all-features"
    
    # Order execution latency test  
    run_test_category "Order Execution Latency" "cargo test --release test_order_execution_latency --all-features"
    
    # Throughput stress test
    run_test_category "Throughput Stress" "cargo test --release test_throughput_stress --all-features"
    
    # Memory leak detection
    run_test_category "Memory Leak Detection" "cargo test --release test_memory_leak_detection --all-features"
    
    # CPU stress test
    run_test_category "CPU Stress Test" "cargo test --release test_cpu_stress --all-features"
}

# Function to run HFT tests
run_hft_tests() {
    echo -e "\n${BLUE}⚡ HIGH FREQUENCY TRADING TORTURE${NC}"
    
    # Microsecond precision tests
    run_test_category "Microsecond Precision" "cargo test --release test_microsecond_precision --all-features"
    
    # Race condition tests
    run_test_category "Race Condition Hunting" "cargo test --release test_race_conditions --all-features -- --test-threads=1"
    
    # Order book consistency under load
    run_test_category "Order Book Consistency" "cargo test --release test_order_book_consistency_under_load --all-features"
    
    # Smart routing under stress
    run_test_category "Smart Routing Stress" "cargo test --release test_smart_routing_stress --all-features"
}

# Function to run memory tests
run_memory_tests() {
    echo -e "\n${BLUE}🧠 MEMORY TORTURE TESTS${NC}"
    
    # Memory allocation stress
    run_test_category "Memory Allocation Stress" "cargo test --release test_memory_allocation_stress --all-features"
    
    # Ring buffer overflow protection
    run_test_category "Ring Buffer Overflow" "cargo test --release test_ring_buffer_overflow --all-features"
    
    # Memory pool efficiency
    run_test_category "Memory Pool Efficiency" "cargo test --release test_memory_pool_efficiency --all-features"
    
    # Garbage collection stress
    run_test_category "GC Stress Test" "cargo test --release test_gc_stress --all-features"
}

# Function to run multi-exchange tests
run_multi_exchange_tests() {
    echo -e "\n${BLUE}🌐 MULTI-EXCHANGE TORTURE${NC}"
    
    local exchanges=("binance" "coinbase" "bybit" "bitget" "hyperliquid" "kucoin" "kraken" "okx")
    
    for exchange in "${exchanges[@]}"; do
        run_test_category "$exchange Exchange Tests" "cargo test --release test_${exchange}_exchange --all-features"
    done
    
    # All exchanges simultaneously
    run_test_category "All Exchanges Simultaneous" "cargo test --release test_all_exchanges_simultaneous --all-features"
    
    # Exchange failover scenarios
    run_test_category "Exchange Failover" "cargo test --release test_exchange_failover --all-features"
}

# Function to run chaos engineering tests
run_chaos_tests() {
    echo -e "\n${BLUE}🌪️  CHAOS ENGINEERING TESTS${NC}"
    
    # Network partition simulation
    run_test_category "Network Partition" "cargo test --release test_network_partition --all-features"
    
    # Exchange outage simulation
    run_test_category "Exchange Outage" "cargo test --release test_exchange_outage --all-features"
    
    # Resource exhaustion
    run_test_category "Resource Exhaustion" "cargo test --release test_resource_exhaustion --all-features"
    
    # Cascading failure simulation
    run_test_category "Cascading Failures" "cargo test --release test_cascading_failures --all-features"
}

# Function to run 24-hour stability test
run_apocalypse_mode() {
    echo -e "\n${RED}💀 APOCALYPSE MODE - 24 HOUR TORTURE 💀${NC}"
    echo "⚠️  This will run for 24 hours with extreme load"
    echo "10,000 concurrent users | 100K orders/sec | 1M market data/sec"
    
    read -p "Are you sure you want to continue? (yes/no): " confirm
    if [[ $confirm != "yes" ]]; then
        echo "Apocalypse mode cancelled."
        return 0
    fi
    
    # Start 24-hour stress test
    run_test_category "24-Hour Apocalypse" "cargo test --release test_24_hour_apocalypse --all-features -- --nocapture"
}

# Function to validate financial precision
run_financial_precision_tests() {
    echo -e "\n${BLUE}💰 FINANCIAL PRECISION VALIDATION${NC}"
    
    # Satoshi-level precision
    run_test_category "Satoshi Precision" "cargo test --release test_satoshi_precision --all-features"
    
    # Decimal arithmetic accuracy
    run_test_category "Decimal Arithmetic" "cargo test --release test_decimal_arithmetic --all-features"
    
    # Large portfolio calculations
    run_test_category "Large Portfolio Math" "cargo test --release test_large_portfolio_calculations --all-features"
    
    # Currency conversion accuracy
    run_test_category "Currency Conversion" "cargo test --release test_currency_conversion_accuracy --all-features"
}

# Main execution logic
FAILED_TESTS=0

case "$CATEGORY" in
    "performance")
        run_performance_tests || ((FAILED_TESTS++))
        ;;
    "hft")
        run_hft_tests || ((FAILED_TESTS++))
        ;;
    "memory")
        run_memory_tests || ((FAILED_TESTS++))
        ;;
    "multi-exchange")
        run_multi_exchange_tests || ((FAILED_TESTS++))
        ;;
    "chaos")
        run_chaos_tests || ((FAILED_TESTS++))
        ;;
    "financial")
        run_financial_precision_tests || ((FAILED_TESTS++))
        ;;
    "all")
        run_performance_tests || ((FAILED_TESTS++))
        run_hft_tests || ((FAILED_TESTS++))
        run_memory_tests || ((FAILED_TESTS++))
        run_multi_exchange_tests || ((FAILED_TESTS++))
        run_chaos_tests || ((FAILED_TESTS++))
        run_financial_precision_tests || ((FAILED_TESTS++))
        ;;
    *)
        echo -e "${RED}❌ Unknown category: $CATEGORY${NC}"
        echo "Available categories: performance, hft, memory, multi-exchange, chaos, financial, all"
        exit 1
        ;;
esac

# Run extended tests if requested
if [[ "$EXTENDED" == "--extended" ]]; then
    echo -e "\n${YELLOW}🔥 EXTENDED TORTURE TESTS${NC}"
    run_test_category "Extended Stress Test" "cargo test --release test_extended_stress --all-features"
fi

# Run apocalypse mode if requested
if [[ "$APOCALYPSE" == "--apocalypse" ]]; then
    run_apocalypse_mode || ((FAILED_TESTS++))
fi

# Final report
echo -e "\n${BLUE}========================================${NC}"
echo -e "${BLUE}🏁 ADVERSARIAL TEST SUITE COMPLETE${NC}"
echo -e "${BLUE}========================================${NC}"

if [[ $FAILED_TESTS -eq 0 ]]; then
    echo -e "${GREEN}✅ ALL TESTS PASSED!${NC}"
    echo -e "${GREEN}🎯 JACKBOT SENSOR IS TORTURE-TESTED AND READY FOR PRODUCTION${NC}"
    exit 0
else
    echo -e "${RED}❌ $FAILED_TESTS TEST CATEGORIES FAILED${NC}"
    echo -e "${RED}🚨 SENSOR IS NOT READY FOR PRODUCTION${NC}"
    exit 1
fi