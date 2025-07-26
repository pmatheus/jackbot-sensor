#!/bin/bash
LOG_DIR="$1"
FRONTEND_URL="${FRONTEND_URL:-http://localhost:3000}"
WS_URL="${WS_URL:-ws://localhost:8080/ws}"
API_URL="${API_URL:-http://localhost:8080/api}"

echo "=== Testing Market Data Display ===" | tee "$LOG_DIR/market_data_summary.txt"
echo "Frontend URL: $FRONTEND_URL" | tee -a "$LOG_DIR/market_data_summary.txt"
echo "WebSocket URL: $WS_URL" | tee -a "$LOG_DIR/market_data_summary.txt"
echo "API URL: $API_URL" | tee -a "$LOG_DIR/market_data_summary.txt"

# Test 1: WebSocket Connection Test
echo -e "\n[1] Testing WebSocket Price Updates..." | tee -a "$LOG_DIR/market_data_summary.txt"

# Create wscat test script
cat > "$LOG_DIR/ws_test.js" << 'EOF'
const WebSocket = require('ws');
const fs = require('fs');

const wsUrl = process.env.WS_URL || 'ws://localhost:8080/ws';
const logFile = process.env.LOG_FILE || 'ws_test.log';
const testDuration = parseInt(process.env.TEST_DURATION || '30') * 1000;

console.log(`Connecting to ${wsUrl}`);
const results = {
    connected: false,
    messages: [],
    errors: [],
    latencies: [],
    startTime: Date.now()
};

const ws = new WebSocket(wsUrl);

ws.on('open', () => {
    console.log('WebSocket connected');
    results.connected = true;
    
    // Subscribe to market data
    ws.send(JSON.stringify({
        type: 'subscribe',
        channel: 'market_data',
        symbols: ['BTC-USD', 'ETH-USD']
    }));
});

ws.on('message', (data) => {
    const receiveTime = Date.now();
    try {
        const msg = JSON.parse(data);
        const latency = msg.timestamp ? receiveTime - msg.timestamp : 0;
        
        results.messages.push({
            time: receiveTime,
            type: msg.type,
            symbol: msg.symbol,
            price: msg.price,
            latency: latency
        });
        
        if (latency > 0) {
            results.latencies.push(latency);
        }
        
        console.log(`Received: ${msg.type} - ${msg.symbol} @ ${msg.price} (latency: ${latency}ms)`);
    } catch (e) {
        results.errors.push({
            time: receiveTime,
            error: e.message,
            data: data.toString()
        });
    }
});

ws.on('error', (err) => {
    console.error('WebSocket error:', err);
    results.errors.push({
        time: Date.now(),
        error: err.message
    });
});

ws.on('close', () => {
    console.log('WebSocket closed');
    saveResults();
});

function saveResults() {
    results.endTime = Date.now();
    results.duration = results.endTime - results.startTime;
    results.messageCount = results.messages.length;
    results.errorCount = results.errors.length;
    
    if (results.latencies.length > 0) {
        results.avgLatency = results.latencies.reduce((a, b) => a + b) / results.latencies.length;
        results.maxLatency = Math.max(...results.latencies);
        results.minLatency = Math.min(...results.latencies);
    }
    
    fs.writeFileSync(logFile, JSON.stringify(results, null, 2));
    process.exit(0);
}

// Run for specified duration
setTimeout(() => {
    ws.close();
}, testDuration);
EOF

# Run WebSocket test
if command -v node > /dev/null; then
    WS_URL="$WS_URL" LOG_FILE="$LOG_DIR/ws_test_results.json" TEST_DURATION="30" \
        timeout 35 node "$LOG_DIR/ws_test.js" 2>&1 | tee "$LOG_DIR/ws_test.log" || true
    
    # Parse results
    if [ -f "$LOG_DIR/ws_test_results.json" ]; then
        echo "WebSocket Test Results:" | tee -a "$LOG_DIR/market_data_summary.txt"
        jq -r '
            "- Connected: \(.connected)",
            "- Messages Received: \(.messageCount)",
            "- Errors: \(.errorCount)",
            "- Avg Latency: \(.avgLatency // 0)ms",
            "- Max Latency: \(.maxLatency // 0)ms"
        ' "$LOG_DIR/ws_test_results.json" | tee -a "$LOG_DIR/market_data_summary.txt"
    fi
else
    echo "Node.js not found, using wscat fallback..." | tee -a "$LOG_DIR/market_data_summary.txt"
    # Fallback to wscat if available
    if command -v wscat > /dev/null; then
        echo '{"type":"subscribe","channel":"market_data","symbols":["BTC-USD"]}' | \
            timeout 10 wscat -c "$WS_URL" > "$LOG_DIR/wscat_output.log" 2>&1 || true
        echo "WebSocket messages captured: $(wc -l < "$LOG_DIR/wscat_output.log")" | tee -a "$LOG_DIR/market_data_summary.txt"
    else
        echo "WARNING: Neither node nor wscat available for WebSocket testing" | tee -a "$LOG_DIR/market_data_summary.txt"
    fi
fi

# Test 2: Order Book API Test
echo -e "\n[2] Testing Order Book API..." | tee -a "$LOG_DIR/market_data_summary.txt"

# Test order book endpoints
for symbol in BTC-USD ETH-USD; do
    echo "Testing order book for $symbol..." | tee -a "$LOG_DIR/market_data_summary.txt"
    
    START_TIME=$(date +%s%N)
    RESPONSE=$(curl -s -w "\n%{http_code}\n%{time_total}" \
        -H "Content-Type: application/json" \
        "$API_URL/orderbook/$symbol" 2>/dev/null || echo "000\n0")
    END_TIME=$(date +%s%N)
    
    HTTP_CODE=$(echo "$RESPONSE" | tail -n2 | head -n1)
    RESPONSE_TIME=$(echo "$RESPONSE" | tail -n1)
    BODY=$(echo "$RESPONSE" | head -n-2)
    
    echo "- HTTP Status: $HTTP_CODE" | tee -a "$LOG_DIR/market_data_summary.txt"
    echo "- Response Time: ${RESPONSE_TIME}s" | tee -a "$LOG_DIR/market_data_summary.txt"
    
    # Validate response structure
    if [ "$HTTP_CODE" = "200" ] && [ -n "$BODY" ]; then
        echo "$BODY" > "$LOG_DIR/orderbook_${symbol}.json"
        
        # Check for required fields
        HAS_BIDS=$(echo "$BODY" | jq -r 'has("bids")')
        HAS_ASKS=$(echo "$BODY" | jq -r 'has("asks")')
        BID_COUNT=$(echo "$BODY" | jq -r '.bids | length' 2>/dev/null || echo "0")
        ASK_COUNT=$(echo "$BODY" | jq -r '.asks | length' 2>/dev/null || echo "0")
        
        echo "- Has Bids: $HAS_BIDS (count: $BID_COUNT)" | tee -a "$LOG_DIR/market_data_summary.txt"
        echo "- Has Asks: $HAS_ASKS (count: $ASK_COUNT)" | tee -a "$LOG_DIR/market_data_summary.txt"
    fi
done

# Test 3: Chart Data API Test
echo -e "\n[3] Testing Chart Data API..." | tee -a "$LOG_DIR/market_data_summary.txt"

# Test chart data endpoints
for interval in 1m 5m 1h; do
    echo "Testing chart data for interval: $interval" | tee -a "$LOG_DIR/market_data_summary.txt"
    
    RESPONSE=$(curl -s -w "\n%{http_code}\n%{time_total}" \
        -H "Content-Type: application/json" \
        "$API_URL/chart/BTC-USD?interval=$interval&limit=100" 2>/dev/null || echo "000\n0")
    
    HTTP_CODE=$(echo "$RESPONSE" | tail -n2 | head -n1)
    RESPONSE_TIME=$(echo "$RESPONSE" | tail -n1)
    BODY=$(echo "$RESPONSE" | head -n-2)
    
    echo "- HTTP Status: $HTTP_CODE" | tee -a "$LOG_DIR/market_data_summary.txt"
    echo "- Response Time: ${RESPONSE_TIME}s" | tee -a "$LOG_DIR/market_data_summary.txt"
    
    if [ "$HTTP_CODE" = "200" ] && [ -n "$BODY" ]; then
        echo "$BODY" > "$LOG_DIR/chart_${interval}.json"
        CANDLE_COUNT=$(echo "$BODY" | jq -r '. | length' 2>/dev/null || echo "0")
        echo "- Candles Returned: $CANDLE_COUNT" | tee -a "$LOG_DIR/market_data_summary.txt"
    fi
done

# Test 4: Concurrent Load Test
echo -e "\n[4] Testing Market Data Under Load..." | tee -a "$LOG_DIR/market_data_summary.txt"

# Create concurrent request script
cat > "$LOG_DIR/load_test.sh" << 'EOF'
#!/bin/bash
ENDPOINT="$1"
RESULTS_FILE="$2"
DURATION="${3:-30}"

END_TIME=$(($(date +%s) + DURATION))

while [ $(date +%s) -lt $END_TIME ]; do
    START_TIME=$(date +%s%N)
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "$ENDPOINT" 2>/dev/null || echo "000")
    END_TIME_NS=$(date +%s%N)
    
    RESPONSE_TIME=$((($END_TIME_NS - $START_TIME) / 1000000))
    echo "$(date +%s),$HTTP_CODE,$RESPONSE_TIME" >> "$RESULTS_FILE"
    
    sleep 0.1
done
EOF

chmod +x "$LOG_DIR/load_test.sh"

# Run concurrent load test
echo "Running 10 concurrent users for 30 seconds..." | tee -a "$LOG_DIR/market_data_summary.txt"
for i in {1..10}; do
    "$LOG_DIR/load_test.sh" "$API_URL/orderbook/BTC-USD" "$LOG_DIR/load_user_$i.csv" 30 &
done

wait

# Analyze load test results
cat "$LOG_DIR"/load_user_*.csv | awk -F',' '
    BEGIN { total=0; success=0; fail=0; sum_time=0; }
    {
        total++;
        sum_time+=$3;
        if ($2 == "200") success++;
        else fail++;
    }
    END {
        print "\nLoad Test Results:";
        print "- Total Requests: " total;
        print "- Successful: " success;
        print "- Failed: " fail;
        print "- Success Rate: " (total > 0 ? (success/total)*100 : 0) "%";
        print "- Avg Response Time: " (total > 0 ? sum_time/total : 0) "ms";
    }
' | tee -a "$LOG_DIR/market_data_summary.txt"

# Generate final summary
echo -e "\n=== Market Data Test Summary ===" | tee -a "$LOG_DIR/market_data_summary.txt"
echo "Test completed at: $(date)" | tee -a "$LOG_DIR/market_data_summary.txt"

# Check if all critical components passed
WEBSOCKET_OK=false
ORDERBOOK_OK=false
CHART_OK=false
PERFORMANCE_OK=false

if [ -f "$LOG_DIR/ws_test_results.json" ]; then
    WS_CONNECTED=$(jq -r '.connected' "$LOG_DIR/ws_test_results.json" 2>/dev/null)
    [ "$WS_CONNECTED" = "true" ] && WEBSOCKET_OK=true
fi

if [ -f "$LOG_DIR/orderbook_BTC-USD.json" ]; then
    HAS_DATA=$(jq -r 'has("bids") and has("asks")' "$LOG_DIR/orderbook_BTC-USD.json" 2>/dev/null)
    [ "$HAS_DATA" = "true" ] && ORDERBOOK_OK=true
fi

if [ -f "$LOG_DIR/chart_1m.json" ]; then
    CANDLE_COUNT=$(jq -r '. | length' "$LOG_DIR/chart_1m.json" 2>/dev/null || echo "0")
    [ "$CANDLE_COUNT" -gt "0" ] && CHART_OK=true
fi

# Check average response time from load test
AVG_RESPONSE=$(cat "$LOG_DIR"/load_user_*.csv 2>/dev/null | \
    awk -F',' '{sum+=$3; count++} END {print (count > 0 ? sum/count : 999)}')
[ "${AVG_RESPONSE%.*}" -lt "100" ] && PERFORMANCE_OK=true

echo -e "\nComponent Status:" | tee -a "$LOG_DIR/market_data_summary.txt"
echo "- WebSocket Updates: $([ "$WEBSOCKET_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/market_data_summary.txt"
echo "- Order Book API: $([ "$ORDERBOOK_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/market_data_summary.txt"
echo "- Chart Data API: $([ "$CHART_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/market_data_summary.txt"
echo "- Performance (<100ms): $([ "$PERFORMANCE_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/market_data_summary.txt"

# Generate JSON summary
cat > "$LOG_DIR/market_data_results.json" << EOF
{
    "feature": "Market Data Display",
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "components": {
        "websocket": $WEBSOCKET_OK,
        "orderbook": $ORDERBOOK_OK,
        "charts": $CHART_OK,
        "performance": $PERFORMANCE_OK
    },
    "metrics": {
        "avgResponseTime": ${AVG_RESPONSE%.*},
        "websocketMessages": $(jq -r '.messageCount // 0' "$LOG_DIR/ws_test_results.json" 2>/dev/null || echo "0"),
        "loadTestRequests": $(cat "$LOG_DIR"/load_user_*.csv 2>/dev/null | wc -l || echo "0")
    },
    "passed": $([ "$WEBSOCKET_OK" = "true" ] && [ "$ORDERBOOK_OK" = "true" ] && [ "$CHART_OK" = "true" ] && [ "$PERFORMANCE_OK" = "true" ] && echo "true" || echo "false")
}
EOF

echo -e "\nDetailed results saved to: $LOG_DIR" | tee -a "$LOG_DIR/market_data_summary.txt"