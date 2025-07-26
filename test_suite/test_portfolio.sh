#!/bin/bash
LOG_DIR="$1"
API_URL="${API_URL:-http://localhost:8080/api}"
AUTH_TOKEN="${AUTH_TOKEN:-test-token}"

echo "=== Testing Portfolio View ===" | tee "$LOG_DIR/portfolio_summary.txt"
echo "API URL: $API_URL" | tee -a "$LOG_DIR/portfolio_summary.txt"

# Test 1: Account Balance API
echo -e "\n[1] Testing Account Balances..." | tee -a "$LOG_DIR/portfolio_summary.txt"

RESPONSE=$(curl -s -w "\n%{http_code}\n%{time_total}" \
    -H "Authorization: Bearer $AUTH_TOKEN" \
    -H "Content-Type: application/json" \
    "$API_URL/account/balances" 2>/dev/null || echo "000\n0")

HTTP_CODE=$(echo "$RESPONSE" | tail -n2 | head -n1)
RESPONSE_TIME=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-2)

echo "- HTTP Status: $HTTP_CODE" | tee -a "$LOG_DIR/portfolio_summary.txt"
echo "- Response Time: ${RESPONSE_TIME}s" | tee -a "$LOG_DIR/portfolio_summary.txt"

BALANCE_OK=false
if [ "$HTTP_CODE" = "200" ] && [ -n "$BODY" ]; then
    echo "$BODY" > "$LOG_DIR/balances.json"
    
    # Validate balance structure
    HAS_USD=$(echo "$BODY" | jq -r 'has("USD") or has("usd")' 2>/dev/null)
    HAS_BTC=$(echo "$BODY" | jq -r 'has("BTC") or has("btc")' 2>/dev/null)
    
    echo "- Has USD Balance: $HAS_USD" | tee -a "$LOG_DIR/portfolio_summary.txt"
    echo "- Has BTC Balance: $HAS_BTC" | tee -a "$LOG_DIR/portfolio_summary.txt"
    
    [ "$HAS_USD" = "true" ] || [ "$HAS_BTC" = "true" ] && BALANCE_OK=true
fi

# Test 2: Current Positions API
echo -e "\n[2] Testing Current Positions..." | tee -a "$LOG_DIR/portfolio_summary.txt"

RESPONSE=$(curl -s -w "\n%{http_code}\n%{time_total}" \
    -H "Authorization: Bearer $AUTH_TOKEN" \
    -H "Content-Type: application/json" \
    "$API_URL/positions" 2>/dev/null || echo "000\n0")

HTTP_CODE=$(echo "$RESPONSE" | tail -n2 | head -n1)
RESPONSE_TIME=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-2)

echo "- HTTP Status: $HTTP_CODE" | tee -a "$LOG_DIR/portfolio_summary.txt"
echo "- Response Time: ${RESPONSE_TIME}s" | tee -a "$LOG_DIR/portfolio_summary.txt"

POSITIONS_OK=false
if [ "$HTTP_CODE" = "200" ] && [ -n "$BODY" ]; then
    echo "$BODY" > "$LOG_DIR/positions.json"
    
    # Check if it's an array or object with positions
    IS_ARRAY=$(echo "$BODY" | jq -r 'type == "array"' 2>/dev/null)
    POSITION_COUNT=0
    
    if [ "$IS_ARRAY" = "true" ]; then
        POSITION_COUNT=$(echo "$BODY" | jq -r 'length' 2>/dev/null || echo "0")
    else
        # Might be wrapped in an object
        POSITION_COUNT=$(echo "$BODY" | jq -r '.positions | length' 2>/dev/null || echo "0")
    fi
    
    echo "- Position Count: $POSITION_COUNT" | tee -a "$LOG_DIR/portfolio_summary.txt"
    
    # Validate position structure (if any positions exist)
    if [ "$POSITION_COUNT" -gt "0" ]; then
        HAS_SYMBOL=$(echo "$BODY" | jq -r '.[0].symbol // .positions[0].symbol // false' 2>/dev/null)
        HAS_QUANTITY=$(echo "$BODY" | jq -r '.[0].quantity // .positions[0].quantity // false' 2>/dev/null)
        HAS_ENTRY_PRICE=$(echo "$BODY" | jq -r '.[0].entryPrice // .positions[0].entryPrice // false' 2>/dev/null)
        
        echo "- Sample Position Has Symbol: $([ "$HAS_SYMBOL" != "false" ] && echo "true" || echo "false")" | tee -a "$LOG_DIR/portfolio_summary.txt"
        echo "- Sample Position Has Quantity: $([ "$HAS_QUANTITY" != "false" ] && echo "true" || echo "false")" | tee -a "$LOG_DIR/portfolio_summary.txt"
        echo "- Sample Position Has Entry Price: $([ "$HAS_ENTRY_PRICE" != "false" ] && echo "true" || echo "false")" | tee -a "$LOG_DIR/portfolio_summary.txt"
    fi
    
    POSITIONS_OK=true
fi

# Test 3: P&L Calculations
echo -e "\n[3] Testing P&L Calculations..." | tee -a "$LOG_DIR/portfolio_summary.txt"

RESPONSE=$(curl -s -w "\n%{http_code}\n%{time_total}" \
    -H "Authorization: Bearer $AUTH_TOKEN" \
    -H "Content-Type: application/json" \
    "$API_URL/account/pnl" 2>/dev/null || echo "000\n0")

HTTP_CODE=$(echo "$RESPONSE" | tail -n2 | head -n1)
RESPONSE_TIME=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-2)

echo "- HTTP Status: $HTTP_CODE" | tee -a "$LOG_DIR/portfolio_summary.txt"
echo "- Response Time: ${RESPONSE_TIME}s" | tee -a "$LOG_DIR/portfolio_summary.txt"

PNL_OK=false
if [ "$HTTP_CODE" = "200" ] && [ -n "$BODY" ]; then
    echo "$BODY" > "$LOG_DIR/pnl.json"
    
    # Validate P&L structure
    HAS_REALIZED=$(echo "$BODY" | jq -r 'has("realizedPnl") or has("realized_pnl") or has("realized")' 2>/dev/null)
    HAS_UNREALIZED=$(echo "$BODY" | jq -r 'has("unrealizedPnl") or has("unrealized_pnl") or has("unrealized")' 2>/dev/null)
    HAS_TOTAL=$(echo "$BODY" | jq -r 'has("totalPnl") or has("total_pnl") or has("total")' 2>/dev/null)
    
    echo "- Has Realized P&L: $HAS_REALIZED" | tee -a "$LOG_DIR/portfolio_summary.txt"
    echo "- Has Unrealized P&L: $HAS_UNREALIZED" | tee -a "$LOG_DIR/portfolio_summary.txt"
    echo "- Has Total P&L: $HAS_TOTAL" | tee -a "$LOG_DIR/portfolio_summary.txt"
    
    [ "$HAS_REALIZED" = "true" ] || [ "$HAS_UNREALIZED" = "true" ] || [ "$HAS_TOTAL" = "true" ] && PNL_OK=true
fi

# Test 4: Portfolio Updates via WebSocket
echo -e "\n[4] Testing Real-time Portfolio Updates..." | tee -a "$LOG_DIR/portfolio_summary.txt"

if command -v node > /dev/null; then
    cat > "$LOG_DIR/portfolio_ws_test.js" << 'EOF'
const WebSocket = require('ws');
const fs = require('fs');

const wsUrl = process.env.WS_URL || 'ws://localhost:8080/ws';
const authToken = process.env.AUTH_TOKEN || 'test-token';
const logFile = process.env.LOG_FILE || 'portfolio_ws_test.log';

const results = {
    connected: false,
    authenticated: false,
    portfolioUpdates: [],
    errors: []
};

const ws = new WebSocket(wsUrl, {
    headers: {
        'Authorization': `Bearer ${authToken}`
    }
});

ws.on('open', () => {
    console.log('WebSocket connected');
    results.connected = true;
    
    // Subscribe to portfolio updates
    ws.send(JSON.stringify({
        type: 'subscribe',
        channel: 'portfolio',
        auth: authToken
    }));
});

ws.on('message', (data) => {
    try {
        const msg = JSON.parse(data);
        
        if (msg.type === 'auth_success') {
            results.authenticated = true;
        }
        
        if (msg.type === 'portfolio_update' || msg.channel === 'portfolio') {
            results.portfolioUpdates.push({
                time: Date.now(),
                data: msg
            });
            console.log('Portfolio update received:', msg);
        }
    } catch (e) {
        results.errors.push({
            time: Date.now(),
            error: e.message
        });
    }
});

ws.on('error', (err) => {
    results.errors.push({
        time: Date.now(),
        error: err.message
    });
});

// Run for 15 seconds
setTimeout(() => {
    ws.close();
    fs.writeFileSync(logFile, JSON.stringify(results, null, 2));
    process.exit(0);
}, 15000);
EOF

    WS_URL="${WS_URL:-ws://localhost:8080/ws}" AUTH_TOKEN="$AUTH_TOKEN" \
        LOG_FILE="$LOG_DIR/portfolio_ws_results.json" \
        timeout 20 node "$LOG_DIR/portfolio_ws_test.js" 2>&1 | tee "$LOG_DIR/portfolio_ws.log" || true
    
    if [ -f "$LOG_DIR/portfolio_ws_results.json" ]; then
        WS_CONNECTED=$(jq -r '.connected' "$LOG_DIR/portfolio_ws_results.json" 2>/dev/null)
        UPDATE_COUNT=$(jq -r '.portfolioUpdates | length' "$LOG_DIR/portfolio_ws_results.json" 2>/dev/null || echo "0")
        
        echo "- WebSocket Connected: $WS_CONNECTED" | tee -a "$LOG_DIR/portfolio_summary.txt"
        echo "- Portfolio Updates Received: $UPDATE_COUNT" | tee -a "$LOG_DIR/portfolio_summary.txt"
    fi
fi

# Test 5: Portfolio Performance Under Load
echo -e "\n[5] Testing Portfolio APIs Under Load..." | tee -a "$LOG_DIR/portfolio_summary.txt"

# Test concurrent portfolio requests
cat > "$LOG_DIR/portfolio_load_test.sh" << 'EOF'
#!/bin/bash
API_URL="$1"
AUTH_TOKEN="$2"
RESULTS_FILE="$3"
DURATION="${4:-30}"

ENDPOINTS=(
    "/account/balances"
    "/positions"
    "/account/pnl"
)

END_TIME=$(($(date +%s) + DURATION))

while [ $(date +%s) -lt $END_TIME ]; do
    ENDPOINT=${ENDPOINTS[$RANDOM % ${#ENDPOINTS[@]}]}
    
    START_TIME=$(date +%s%N)
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -H "Authorization: Bearer $AUTH_TOKEN" \
        "$API_URL$ENDPOINT" 2>/dev/null || echo "000")
    END_TIME_NS=$(date +%s%N)
    
    RESPONSE_TIME=$((($END_TIME_NS - $START_TIME) / 1000000))
    echo "$(date +%s),$ENDPOINT,$HTTP_CODE,$RESPONSE_TIME" >> "$RESULTS_FILE"
    
    sleep 0.2
done
EOF

chmod +x "$LOG_DIR/portfolio_load_test.sh"

# Run load test with 5 concurrent users
echo "Running 5 concurrent users for 30 seconds..." | tee -a "$LOG_DIR/portfolio_summary.txt"
for i in {1..5}; do
    "$LOG_DIR/portfolio_load_test.sh" "$API_URL" "$AUTH_TOKEN" "$LOG_DIR/portfolio_load_$i.csv" 30 &
done

wait

# Analyze load test results
cat "$LOG_DIR"/portfolio_load_*.csv | awk -F',' '
    BEGIN { total=0; success=0; sum_time=0; }
    {
        total++;
        sum_time+=$4;
        if ($3 == "200") success++;
    }
    END {
        print "\nPortfolio Load Test Results:";
        print "- Total Requests: " total;
        print "- Successful: " success;
        print "- Success Rate: " (total > 0 ? (success/total)*100 : 0) "%";
        print "- Avg Response Time: " (total > 0 ? sum_time/total : 0) "ms";
    }
' | tee -a "$LOG_DIR/portfolio_summary.txt"

# Calculate average response time for performance check
AVG_RESPONSE=$(cat "$LOG_DIR"/portfolio_load_*.csv 2>/dev/null | \
    awk -F',' '{sum+=$4; count++} END {print (count > 0 ? sum/count : 999)}')
PERFORMANCE_OK=$([ "${AVG_RESPONSE%.*}" -lt "100" ] && echo "true" || echo "false")

# Generate summary
echo -e "\n=== Portfolio Test Summary ===" | tee -a "$LOG_DIR/portfolio_summary.txt"
echo "Test completed at: $(date)" | tee -a "$LOG_DIR/portfolio_summary.txt"

echo -e "\nComponent Status:" | tee -a "$LOG_DIR/portfolio_summary.txt"
echo "- Account Balances: $([ "$BALANCE_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/portfolio_summary.txt"
echo "- Current Positions: $([ "$POSITIONS_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/portfolio_summary.txt"
echo "- P&L Calculations: $([ "$PNL_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/portfolio_summary.txt"
echo "- Performance (<100ms): $([ "$PERFORMANCE_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/portfolio_summary.txt"

# Generate JSON summary
cat > "$LOG_DIR/portfolio_results.json" << EOF
{
    "feature": "Portfolio View",
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "components": {
        "balances": $BALANCE_OK,
        "positions": $POSITIONS_OK,
        "pnl": $PNL_OK,
        "performance": $PERFORMANCE_OK
    },
    "metrics": {
        "avgResponseTime": ${AVG_RESPONSE%.*},
        "loadTestRequests": $(cat "$LOG_DIR"/portfolio_load_*.csv 2>/dev/null | wc -l || echo "0")
    },
    "passed": $([ "$BALANCE_OK" = "true" ] && [ "$POSITIONS_OK" = "true" ] && [ "$PNL_OK" = "true" ] && [ "$PERFORMANCE_OK" = "true" ] && echo "true" || echo "false")
}
EOF

echo -e "\nDetailed results saved to: $LOG_DIR" | tee -a "$LOG_DIR/portfolio_summary.txt"