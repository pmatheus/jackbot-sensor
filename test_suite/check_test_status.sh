#!/bin/bash
TEST_NAME="${1:-test_run}"
PID_FILE="test_${TEST_NAME}.pid"
STATUS_FILE="test_${TEST_NAME}.status"

if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE")
    if ps -p $PID > /dev/null; then
        echo "Tests still running (PID: $PID)"
        echo "Status: $(cat $STATUS_FILE 2>/dev/null || echo 'UNKNOWN')"
        
        # Show recent activity
        LATEST_LOG_DIR=$(ls -td test_results_* 2>/dev/null | head -1)
        if [ -d "$LATEST_LOG_DIR" ]; then
            echo -e "\nRecent activity from $LATEST_LOG_DIR:"
            tail -n 5 "$LATEST_LOG_DIR"/*.log 2>/dev/null | grep -v "^$" | head -10
        fi
    else
        echo "Tests completed"
        rm -f "$PID_FILE"
        
        # Show summary
        LATEST_LOG_DIR=$(ls -td test_results_* 2>/dev/null | head -1)
        if [ -f "$LATEST_LOG_DIR/summary.log" ]; then
            echo -e "\nTest Summary:"
            cat "$LATEST_LOG_DIR/summary.log"
        fi
        
        # Show test results
        if [ -f "$LATEST_LOG_DIR/test_results.json" ]; then
            echo -e "\nTest Results:"
            cat "$LATEST_LOG_DIR/test_results.json"
        fi
    fi
else
    echo "No tests running for: $TEST_NAME"
fi