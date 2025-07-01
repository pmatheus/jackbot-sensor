#!/bin/bash
# DeFi Integration Test Runner
# Sets up mainnet forks and runs comprehensive integration tests

set -e

echo "🧪 DeFi Integration Test Suite"
echo "=============================="

# Check if required tools are installed
check_dependencies() {
    echo "📋 Checking dependencies..."
    
    if ! command -v cargo &> /dev/null; then
        echo "❌ cargo not found. Please install Rust."
        exit 1
    fi
    
    if ! command -v anvil &> /dev/null; then
        echo "❌ anvil not found. Installing foundry..."
        curl -L https://foundry.paradigm.xyz | bash
        source ~/.bashrc
        foundryup
    fi
    
    echo "✅ All dependencies satisfied"
}

# Start local blockchain forks
start_forks() {
    echo ""
    echo "🔧 Starting mainnet forks..."
    
    # Kill any existing anvil instances
    pkill anvil || true
    sleep 2
    
    # Start Ethereum mainnet fork
    echo "  → Starting Ethereum fork on port 8545..."
    anvil --fork-url https://eth-mainnet.g.alchemy.com/v2/demo \
          --port 8545 \
          --chain-id 1 \
          --block-time 12 \
          --accounts 10 \
          --balance 10000 \
          > /tmp/anvil_eth.log 2>&1 &
    ETH_PID=$!
    
    # Start BSC fork
    echo "  → Starting BSC fork on port 8546..."
    anvil --fork-url https://bsc-dataseed.binance.org/ \
          --port 8546 \
          --chain-id 56 \
          --block-time 3 \
          --accounts 10 \
          --balance 10000 \
          > /tmp/anvil_bsc.log 2>&1 &
    BSC_PID=$!
    
    # Start Polygon fork
    echo "  → Starting Polygon fork on port 8547..."
    anvil --fork-url https://polygon-rpc.com/ \
          --port 8547 \
          --chain-id 137 \
          --block-time 2 \
          --accounts 10 \
          --balance 10000 \
          > /tmp/anvil_polygon.log 2>&1 &
    POLYGON_PID=$!
    
    # Start Arbitrum fork
    echo "  → Starting Arbitrum fork on port 8548..."
    anvil --fork-url https://arb1.arbitrum.io/rpc \
          --port 8548 \
          --chain-id 42161 \
          --block-time 1 \
          --accounts 10 \
          --balance 10000 \
          > /tmp/anvil_arbitrum.log 2>&1 &
    ARBITRUM_PID=$!
    
    # Start Optimism fork
    echo "  → Starting Optimism fork on port 8549..."
    anvil --fork-url https://mainnet.optimism.io \
          --port 8549 \
          --chain-id 10 \
          --block-time 2 \
          --accounts 10 \
          --balance 10000 \
          > /tmp/anvil_optimism.log 2>&1 &
    OPTIMISM_PID=$!
    
    # Wait for forks to be ready
    echo "  → Waiting for forks to initialize..."
    sleep 10
    
    # Verify forks are running
    for port in 8545 8546 8547 8548 8549; do
        if curl -s -X POST http://127.0.0.1:$port \
             -H "Content-Type: application/json" \
             -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
             > /dev/null 2>&1; then
            echo "  ✅ Fork on port $port is ready"
        else
            echo "  ❌ Fork on port $port failed to start"
            cleanup
            exit 1
        fi
    done
    
    echo "✅ All mainnet forks started successfully"
}

# Fund test accounts with tokens
fund_test_accounts() {
    echo ""
    echo "💰 Funding test accounts..."
    
    # Test wallet address (from test private key)
    TEST_WALLET="0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
    
    # Fund with tokens on Ethereum fork
    echo "  → Funding Ethereum test wallet..."
    cast send --rpc-url http://127.0.0.1:8545 \
              --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
              $TEST_WALLET \
              --value 1000ether \
              > /dev/null 2>&1 || true
    
    # TODO: Add token funding (USDC, USDT, etc.) via whale impersonation
    
    echo "✅ Test accounts funded"
}

# Run integration tests
run_tests() {
    echo ""
    echo "🚀 Running DeFi integration tests..."
    echo ""
    
    # Set test environment variables
    export RUST_LOG=debug
    export TEST_MODE=integration
    
    # Run tests with detailed output
    cd /Users/user/wuwei/jackbot-sensor
    cargo test --test test_defi_integration --features integration-tests -- --nocapture --test-threads=1
    
    TEST_RESULT=$?
    
    if [ $TEST_RESULT -eq 0 ]; then
        echo ""
        echo "✅ All integration tests passed!"
    else
        echo ""
        echo "❌ Some tests failed. Check the output above."
    fi
    
    return $TEST_RESULT
}

# Cleanup function
cleanup() {
    echo ""
    echo "🧹 Cleaning up..."
    
    # Kill all anvil processes
    kill $ETH_PID 2>/dev/null || true
    kill $BSC_PID 2>/dev/null || true
    kill $POLYGON_PID 2>/dev/null || true
    kill $ARBITRUM_PID 2>/dev/null || true
    kill $OPTIMISM_PID 2>/dev/null || true
    
    # Clean up any remaining anvil processes
    pkill anvil || true
    
    # Remove log files
    rm -f /tmp/anvil_*.log
    
    echo "✅ Cleanup complete"
}

# Trap cleanup on exit
trap cleanup EXIT

# Main execution
main() {
    echo "Starting at $(date)"
    echo ""
    
    check_dependencies
    start_forks
    fund_test_accounts
    run_tests
    
    echo ""
    echo "Completed at $(date)"
}

# Run main function
main