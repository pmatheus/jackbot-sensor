#!/bin/bash
LOG_DIR="$1"

echo "=== Running All MVP Tests ===" | tee "$LOG_DIR/all_tests_summary.txt"
echo "Started at: $(date)" | tee -a "$LOG_DIR/all_tests_summary.txt"

# Initialize results
RESULTS_JSON="$LOG_DIR/test_results.json"
cat > "$RESULTS_JSON" << EOF
{
    "testRun": "MVP Feature Tests",
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "environment": {
        "frontend": "${FRONTEND_URL:-http://localhost:3000}",
        "api": "${API_URL:-http://localhost:8080/api}",
        "websocket": "${WS_URL:-ws://localhost:8080/ws}"
    },
    "features": {}
}
EOF

# Function to update results
update_results() {
    local feature="$1"
    local results_file="$2"
    
    if [ -f "$results_file" ]; then
        # Merge feature results into main results
        jq --arg feature "$feature" --slurpfile new "$results_file" \
            '.features[$feature] = $new[0]' "$RESULTS_JSON" > "$RESULTS_JSON.tmp" && \
            mv "$RESULTS_JSON.tmp" "$RESULTS_JSON"
    fi
}

# Test 1: Market Data Display
echo -e "\n[1/3] Testing Market Data Display..." | tee -a "$LOG_DIR/all_tests_summary.txt"
echo "======================================" | tee -a "$LOG_DIR/all_tests_summary.txt"

./test_market_data.sh "$LOG_DIR" 2>&1 | tee -a "$LOG_DIR/all_tests_summary.txt"
update_results "marketData" "$LOG_DIR/market_data_results.json"

# Add delay between tests
sleep 5

# Test 2: Portfolio View
echo -e "\n[2/3] Testing Portfolio View..." | tee -a "$LOG_DIR/all_tests_summary.txt"
echo "================================" | tee -a "$LOG_DIR/all_tests_summary.txt"

./test_portfolio.sh "$LOG_DIR" 2>&1 | tee -a "$LOG_DIR/all_tests_summary.txt"
update_results "portfolio" "$LOG_DIR/portfolio_results.json"

# Add delay between tests
sleep 5

# Test 3: Order Placement
echo -e "\n[3/3] Testing Order Placement..." | tee -a "$LOG_DIR/all_tests_summary.txt"
echo "==================================" | tee -a "$LOG_DIR/all_tests_summary.txt"

./test_orders.sh "$LOG_DIR" 2>&1 | tee -a "$LOG_DIR/all_tests_summary.txt"
update_results "orders" "$LOG_DIR/orders_results.json"

# Run E2E tests if requested
if [ "${RUN_E2E:-true}" = "true" ]; then
    echo -e "\n[Bonus] Running E2E Tests..." | tee -a "$LOG_DIR/all_tests_summary.txt"
    echo "=============================" | tee -a "$LOG_DIR/all_tests_summary.txt"
    
    ./test_e2e_playwright.sh "$LOG_DIR" 2>&1 | tee -a "$LOG_DIR/all_tests_summary.txt"
    update_results "e2e" "$LOG_DIR/e2e_results.json"
fi

# Generate final summary
echo -e "\n\n=========== FINAL TEST SUMMARY ===========" | tee -a "$LOG_DIR/all_tests_summary.txt"
echo "Completed at: $(date)" | tee -a "$LOG_DIR/all_tests_summary.txt"

# Calculate overall results
if [ -f "$RESULTS_JSON" ]; then
    # Add summary to results
    jq '
        .summary = {
            totalFeatures: (.features | length),
            passedFeatures: (.features | to_entries | map(select(.value.passed == true)) | length),
            failedFeatures: (.features | to_entries | map(select(.value.passed == false)) | length),
            coverage: 0
        } |
        .summary.successRate = ((.summary.passedFeatures / .summary.totalFeatures) * 100) |
        .summary.allPassed = (.summary.passedFeatures == .summary.totalFeatures)
    ' "$RESULTS_JSON" > "$RESULTS_JSON.tmp" && mv "$RESULTS_JSON.tmp" "$RESULTS_JSON"
    
    # Display summary
    echo -e "\nFeature Test Results:" | tee -a "$LOG_DIR/all_tests_summary.txt"
    jq -r '
        .features | to_entries | .[] | 
        "- " + .key + ": " + (if .value.passed then "✓ PASSED" else "✗ FAILED" end)
    ' "$RESULTS_JSON" | tee -a "$LOG_DIR/all_tests_summary.txt"
    
    echo -e "\nPerformance Metrics:" | tee -a "$LOG_DIR/all_tests_summary.txt"
    jq -r '
        .features | to_entries | .[] |
        if .value.metrics.avgResponseTime then
            "- " + .key + " Avg Response: " + (.value.metrics.avgResponseTime | tostring) + "ms"
        else empty end
    ' "$RESULTS_JSON" | tee -a "$LOG_DIR/all_tests_summary.txt"
    
    # Overall success criteria check
    echo -e "\nSuccess Criteria Check:" | tee -a "$LOG_DIR/all_tests_summary.txt"
    
    # Check if all features work
    ALL_FEATURES_WORK=$(jq -r '.summary.allPassed' "$RESULTS_JSON")
    echo "- All MVP features work E2E: $([ "$ALL_FEATURES_WORK" = "true" ] && echo "✓ YES" || echo "✗ NO")" | tee -a "$LOG_DIR/all_tests_summary.txt"
    
    # Check performance
    AVG_RESPONSE_OK=true
    jq -r '.features | to_entries | .[] | select(.value.metrics.avgResponseTime) | .value.metrics.avgResponseTime' "$RESULTS_JSON" | while read -r response_time; do
        if [ "${response_time%.*}" -ge "100" ]; then
            AVG_RESPONSE_OK=false
        fi
    done
    
    echo "- API responses <100ms: $([ "$AVG_RESPONSE_OK" = "true" ] && echo "✓ YES" || echo "✗ NO")" | tee -a "$LOG_DIR/all_tests_summary.txt"
    
    # Coverage estimate (based on tests run)
    TOTAL_TESTS=$(jq -r '[.features[].metrics | to_entries | .[].value] | add' "$RESULTS_JSON" 2>/dev/null || echo "0")
    COVERAGE_ESTIMATE=$((TOTAL_TESTS * 100 / 500))  # Rough estimate
    [ $COVERAGE_ESTIMATE -gt 15 ] && COVERAGE_ESTIMATE=15
    
    echo "- Coverage estimate: ~${COVERAGE_ESTIMATE}%" | tee -a "$LOG_DIR/all_tests_summary.txt"
    
    # Final verdict
    echo -e "\n============ FINAL VERDICT ============" | tee -a "$LOG_DIR/all_tests_summary.txt"
    if [ "$ALL_FEATURES_WORK" = "true" ] && [ "$AVG_RESPONSE_OK" = "true" ]; then
        echo "✓ MVP READY: All critical features pass!" | tee -a "$LOG_DIR/all_tests_summary.txt"
    else
        echo "✗ NOT READY: Some features need attention" | tee -a "$LOG_DIR/all_tests_summary.txt"
    fi
    echo "=======================================" | tee -a "$LOG_DIR/all_tests_summary.txt"
fi

# List all generated files
echo -e "\nGenerated Test Artifacts:" | tee -a "$LOG_DIR/all_tests_summary.txt"
echo "- Summary: $LOG_DIR/all_tests_summary.txt" | tee -a "$LOG_DIR/all_tests_summary.txt"
echo "- Results: $LOG_DIR/test_results.json" | tee -a "$LOG_DIR/all_tests_summary.txt"
echo "- Logs: $LOG_DIR/*.log" | tee -a "$LOG_DIR/all_tests_summary.txt"
echo "- Screenshots: $LOG_DIR/*.png" | tee -a "$LOG_DIR/all_tests_summary.txt"

# Create a simple HTML report if possible
cat > "$LOG_DIR/test_report.html" << 'HTML'
<!DOCTYPE html>
<html>
<head>
    <title>Jackbot MVP Test Report</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; }
        .pass { color: green; }
        .fail { color: red; }
        .metric { background: #f0f0f0; padding: 10px; margin: 10px 0; }
        table { border-collapse: collapse; width: 100%; }
        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
        th { background-color: #4CAF50; color: white; }
    </style>
</head>
<body>
    <h1>Jackbot MVP Test Report</h1>
    <p>Generated: <script>document.write(new Date().toLocaleString());</script></p>
    
    <h2>Test Results Summary</h2>
    <div id="results">Loading results...</div>
    
    <script>
        // This would normally load the JSON results
        document.getElementById('results').innerHTML = 
            '<p>View test_results.json for detailed results</p>';
    </script>
</body>
</html>
HTML

echo -e "\nTest report generated: $LOG_DIR/test_report.html" | tee -a "$LOG_DIR/all_tests_summary.txt"