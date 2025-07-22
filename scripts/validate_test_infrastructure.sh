#!/bin/bash

#============================================================================
# JACKBOT SENSOR TEST INFRASTRUCTURE VALIDATION
# Verify all adversarial test components are in place and ready
#============================================================================

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🔍 JACKBOT SENSOR TEST INFRASTRUCTURE VALIDATION${NC}"
echo "=================================================="

cd "$(dirname "$0")/.."

# Function to check file exists
check_file() {
    local file="$1"
    local description="$2"
    
    if [[ -f "$file" ]]; then
        echo -e "${GREEN}✅ $description${NC}"
        return 0
    else
        echo -e "${RED}❌ $description - FILE MISSING: $file${NC}"
        return 1
    fi
}

# Function to check directory exists
check_directory() {
    local dir="$1"
    local description="$2"
    
    if [[ -d "$dir" ]]; then
        echo -e "${GREEN}✅ $description${NC}"
        return 0
    else
        echo -e "${RED}❌ $description - DIRECTORY MISSING: $dir${NC}"
        return 1
    fi
}

FAILED_CHECKS=0

echo -e "\n${BLUE}📁 CHECKING TEST INFRASTRUCTURE FILES${NC}"

# Test scripts
check_file "scripts/run_sensor_adversarial_tests.sh" "Adversarial Test Suite Script" || ((FAILED_CHECKS++))

# Test files
check_file "tests/adversarial_performance_tests.rs" "Core Adversarial Performance Tests" || ((FAILED_CHECKS++))
check_file "tests/multi_exchange_adversarial_tests.rs" "Multi-Exchange Torture Tests" || ((FAILED_CHECKS++))
check_file "tests/binance_order_execution_tests.rs" "Binance Order Execution Tests" || ((FAILED_CHECKS++))

# Performance infrastructure
check_file "src/performance_benchmarks.rs" "Performance Benchmarks Module" || ((FAILED_CHECKS++))
check_file "jackbot-data/src/performance/latency_tracker.rs" "Latency Tracker Implementation" || ((FAILED_CHECKS++))
check_file "jackbot-data/src/performance/memory_pool.rs" "Memory Pool Implementation" || ((FAILED_CHECKS++))

# Documentation
check_file "PERFORMANCE_VALIDATION_REPORT.md" "Performance Validation Report" || ((FAILED_CHECKS++))

echo -e "\n${BLUE}📂 CHECKING DIRECTORY STRUCTURE${NC}"

# Test directories
check_directory "tests" "Tests Directory" || ((FAILED_CHECKS++))
check_directory "scripts" "Scripts Directory" || ((FAILED_CHECKS++))
check_directory "jackbot-data/src/performance" "Performance Module Directory" || ((FAILED_CHECKS++))

echo -e "\n${BLUE}🔧 CHECKING SCRIPT PERMISSIONS${NC}"

if [[ -x "scripts/run_sensor_adversarial_tests.sh" ]]; then
    echo -e "${GREEN}✅ Adversarial test script is executable${NC}"
else
    echo -e "${RED}❌ Adversarial test script is not executable${NC}"
    echo -e "${YELLOW}   Run: chmod +x scripts/run_sensor_adversarial_tests.sh${NC}"
    ((FAILED_CHECKS++))
fi

echo -e "\n${BLUE}📋 VALIDATING TEST CATEGORIES${NC}"

# Check test categories are implemented
REQUIRED_TESTS=(
    "test_market_data_processing_latency"
    "test_order_execution_latency"
    "test_throughput_stress"
    "test_memory_leak_detection"
    "test_race_conditions"
    "test_satoshi_precision"
    "test_all_exchanges_simultaneous"
    "test_exchange_failover"
)

for test_name in "${REQUIRED_TESTS[@]}"; do
    if grep -q "$test_name" tests/adversarial_performance_tests.rs tests/multi_exchange_adversarial_tests.rs 2>/dev/null; then
        echo -e "${GREEN}✅ $test_name implemented${NC}"
    else
        echo -e "${RED}❌ $test_name not found${NC}"
        ((FAILED_CHECKS++))
    fi
done

echo -e "\n${BLUE}🎯 PERFORMANCE TARGETS VALIDATION${NC}"

# Check performance targets are properly defined
TARGETS=(
    "MARKET_DATA_LATENCY_TARGET"
    "ORDER_EXECUTION_LATENCY_TARGET"
    "THROUGHPUT_TARGET_MPS"
    "SUCCESS_RATE_TARGET"
)

for target in "${TARGETS[@]}"; do
    if grep -q "$target" tests/adversarial_performance_tests.rs 2>/dev/null; then
        echo -e "${GREEN}✅ $target defined${NC}"
    else
        echo -e "${RED}❌ $target not found${NC}"
        ((FAILED_CHECKS++))
    fi
done

echo -e "\n${BLUE}🔄 CHECKING CARGO CONFIGURATION${NC}"

if [[ -f "Cargo.toml" ]]; then
    echo -e "${GREEN}✅ Cargo.toml exists${NC}"
    
    # Check for required dependencies
    REQUIRED_DEPS=(
        "tokio"
        "futures"
        "anyhow"
        "tracing"
        "serde"
    )
    
    for dep in "${REQUIRED_DEPS[@]}"; do
        if grep -q "^$dep = " Cargo.toml; then
            echo -e "${GREEN}✅ Dependency: $dep${NC}"
        else
            echo -e "${YELLOW}⚠️  Dependency might be missing: $dep${NC}"
        fi
    done
else
    echo -e "${RED}❌ Cargo.toml not found${NC}"
    ((FAILED_CHECKS++))
fi

echo -e "\n${BLUE}🚀 TEST INFRASTRUCTURE SUMMARY${NC}"
echo "============================================"

if [[ $FAILED_CHECKS -eq 0 ]]; then
    echo -e "${GREEN}✅ ALL INFRASTRUCTURE CHECKS PASSED!${NC}"
    echo -e "${GREEN}🎯 ADVERSARIAL TEST SUITE IS READY FOR EXECUTION${NC}"
    echo -e "${GREEN}⚡ PERFORMANCE VALIDATION CAN PROCEED${NC}"
    
    echo -e "\n${BLUE}📋 READY TO EXECUTE:${NC}"
    echo "  ./scripts/run_sensor_adversarial_tests.sh performance"
    echo "  ./scripts/run_sensor_adversarial_tests.sh hft"
    echo "  ./scripts/run_sensor_adversarial_tests.sh memory"
    echo "  ./scripts/run_sensor_adversarial_tests.sh multi-exchange"
    echo "  ./scripts/run_sensor_adversarial_tests.sh all"
    echo "  ./scripts/run_sensor_adversarial_tests.sh all --extended --apocalypse"
    
    exit 0
else
    echo -e "${RED}❌ $FAILED_CHECKS INFRASTRUCTURE ISSUES DETECTED${NC}"
    echo -e "${RED}🚨 RESOLVE ISSUES BEFORE RUNNING PERFORMANCE TESTS${NC}"
    exit 1
fi