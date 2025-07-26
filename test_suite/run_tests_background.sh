#!/bin/bash
TEST_TYPE="${1:-all}"
TEST_NAME="${2:-test_run}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_DIR="test_results_${TIMESTAMP}"
PID_FILE="test_${TEST_NAME}.pid"
STATUS_FILE="test_${TEST_NAME}.status"

mkdir -p "$LOG_DIR"

# Function to run test in background
run_test_suite() {
    echo "RUNNING" > "$STATUS_FILE"
    echo "Test Type: $TEST_TYPE" | tee "$LOG_DIR/summary.log"
    echo "Started: $(date)" | tee -a "$LOG_DIR/summary.log"
    
    case "$TEST_TYPE" in
        "market-data")
            ./test_market_data.sh "$LOG_DIR" 2>&1 | tee "$LOG_DIR/market_data.log"
            ;;
        "portfolio")
            ./test_portfolio.sh "$LOG_DIR" 2>&1 | tee "$LOG_DIR/portfolio.log"
            ;;
        "orders")
            ./test_orders.sh "$LOG_DIR" 2>&1 | tee "$LOG_DIR/orders.log"
            ;;
        "api-load")
            ./test_api_load.sh "$LOG_DIR" 2>&1 | tee "$LOG_DIR/api_load.log"
            ;;
        "websocket")
            ./test_websocket.sh "$LOG_DIR" 2>&1 | tee "$LOG_DIR/websocket.log"
            ;;
        "e2e")
            ./test_e2e_playwright.sh "$LOG_DIR" 2>&1 | tee "$LOG_DIR/e2e.log"
            ;;
        "all")
            ./test_all_mvp.sh "$LOG_DIR" 2>&1 | tee "$LOG_DIR/all_tests.log"
            ;;
    esac
    
    echo "Completed: $(date)" | tee -a "$LOG_DIR/summary.log"
    echo "COMPLETED" > "$STATUS_FILE"
}

# Run in background
nohup bash -c "$(declare -f run_test_suite); run_test_suite" > /dev/null 2>&1 &
echo $! > "$PID_FILE"

echo "Tests started in background"
echo "PID: $(cat $PID_FILE)"
echo "Results directory: $LOG_DIR"
echo "Check status with: ./check_test_status.sh $TEST_NAME"