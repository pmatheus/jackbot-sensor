#!/bin/bash
LOG_DIR="$1"
API_URL="${API_URL:-http://localhost:8080/api}"
AUTH_TOKEN="${AUTH_TOKEN:-test-token}"

echo "=== Testing Order Placement ===" | tee "$LOG_DIR/orders_summary.txt"
echo "API URL: $API_URL" | tee -a "$LOG_DIR/orders_summary.txt"

# Test 1: Order Validation
echo -e "\n[1] Testing Order Validation..." | tee -a "$LOG_DIR/orders_summary.txt"

# Test market order validation
MARKET_ORDER=$(cat <<EOF
{
    "symbol": "BTC-USD",
    "side": "buy",
    "type": "market",
    "quantity": 0.001
}
EOF
)

echo "Testing market order validation..." | tee -a "$LOG_DIR/orders_summary.txt"
RESPONSE=$(curl -s -w "\n%{http_code}\n%{time_total}" \
    -X POST \
    -H "Authorization: Bearer $AUTH_TOKEN" \
    -H "Content-Type: application/json" \
    -d "$MARKET_ORDER" \
    "$API_URL/orders/validate" 2>/dev/null || echo "000\n0")

HTTP_CODE=$(echo "$RESPONSE" | tail -n2 | head -n1)
RESPONSE_TIME=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-2)

echo "- Market Order Validation Status: $HTTP_CODE" | tee -a "$LOG_DIR/orders_summary.txt"
echo "- Response Time: ${RESPONSE_TIME}s" | tee -a "$LOG_DIR/orders_summary.txt"

MARKET_VALIDATION_OK=false
if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "201" ]; then
    MARKET_VALIDATION_OK=true
    echo "$BODY" > "$LOG_DIR/market_order_validation.json"
fi

# Test limit order validation
LIMIT_ORDER=$(cat <<EOF
{
    "symbol": "ETH-USD",
    "side": "sell",
    "type": "limit",
    "quantity": 0.1,
    "price": 3500.00
}
EOF
)

echo -e "\nTesting limit order validation..." | tee -a "$LOG_DIR/orders_summary.txt"
RESPONSE=$(curl -s -w "\n%{http_code}\n%{time_total}" \
    -X POST \
    -H "Authorization: Bearer $AUTH_TOKEN" \
    -H "Content-Type: application/json" \
    -d "$LIMIT_ORDER" \
    "$API_URL/orders/validate" 2>/dev/null || echo "000\n0")

HTTP_CODE=$(echo "$RESPONSE" | tail -n2 | head -n1)
RESPONSE_TIME=$(echo "$RESPONSE" | tail -n1)

echo "- Limit Order Validation Status: $HTTP_CODE" | tee -a "$LOG_DIR/orders_summary.txt"
echo "- Response Time: ${RESPONSE_TIME}s" | tee -a "$LOG_DIR/orders_summary.txt"

LIMIT_VALIDATION_OK=false
if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "201" ]; then
    LIMIT_VALIDATION_OK=true
fi

# Test 2: Order Placement
echo -e "\n[2] Testing Order Placement..." | tee -a "$LOG_DIR/orders_summary.txt"

# Place a test market order
echo "Placing test market order..." | tee -a "$LOG_DIR/orders_summary.txt"
RESPONSE=$(curl -s -w "\n%{http_code}\n%{time_total}" \
    -X POST \
    -H "Authorization: Bearer $AUTH_TOKEN" \
    -H "Content-Type: application/json" \
    -d "$MARKET_ORDER" \
    "$API_URL/orders" 2>/dev/null || echo "000\n0")

HTTP_CODE=$(echo "$RESPONSE" | tail -n2 | head -n1)
RESPONSE_TIME=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-2)

echo "- Order Placement Status: $HTTP_CODE" | tee -a "$LOG_DIR/orders_summary.txt"
echo "- Response Time: ${RESPONSE_TIME}s" | tee -a "$LOG_DIR/orders_summary.txt"

ORDER_PLACEMENT_OK=false
ORDER_ID=""
if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "201" ]; then
    ORDER_PLACEMENT_OK=true
    echo "$BODY" > "$LOG_DIR/order_response.json"
    
    # Extract order ID for further tests
    ORDER_ID=$(echo "$BODY" | jq -r '.orderId // .order_id // .id // empty' 2>/dev/null)
    if [ -n "$ORDER_ID" ]; then
        echo "- Order ID: $ORDER_ID" | tee -a "$LOG_DIR/orders_summary.txt"
    fi
    
    # Validate order response structure
    HAS_STATUS=$(echo "$BODY" | jq -r 'has("status")' 2>/dev/null)
    HAS_SYMBOL=$(echo "$BODY" | jq -r 'has("symbol")' 2>/dev/null)
    HAS_TYPE=$(echo "$BODY" | jq -r 'has("type") or has("orderType")' 2>/dev/null)
    
    echo "- Response Has Status: $HAS_STATUS" | tee -a "$LOG_DIR/orders_summary.txt"
    echo "- Response Has Symbol: $HAS_SYMBOL" | tee -a "$LOG_DIR/orders_summary.txt"
    echo "- Response Has Type: $HAS_TYPE" | tee -a "$LOG_DIR/orders_summary.txt"
fi

# Test 3: Order Status Check
echo -e "\n[3] Testing Order Status Check..." | tee -a "$LOG_DIR/orders_summary.txt"

ORDER_STATUS_OK=false
if [ -n "$ORDER_ID" ]; then
    sleep 2  # Wait for order to process
    
    RESPONSE=$(curl -s -w "\n%{http_code}\n%{time_total}" \
        -H "Authorization: Bearer $AUTH_TOKEN" \
        -H "Content-Type: application/json" \
        "$API_URL/orders/$ORDER_ID" 2>/dev/null || echo "000\n0")
    
    HTTP_CODE=$(echo "$RESPONSE" | tail -n2 | head -n1)
    RESPONSE_TIME=$(echo "$RESPONSE" | tail -n1)
    BODY=$(echo "$RESPONSE" | head -n-2)
    
    echo "- Order Status Check: $HTTP_CODE" | tee -a "$LOG_DIR/orders_summary.txt"
    echo "- Response Time: ${RESPONSE_TIME}s" | tee -a "$LOG_DIR/orders_summary.txt"
    
    if [ "$HTTP_CODE" = "200" ]; then
        ORDER_STATUS_OK=true
        ORDER_STATUS=$(echo "$BODY" | jq -r '.status // empty' 2>/dev/null)
        echo "- Order Status: $ORDER_STATUS" | tee -a "$LOG_DIR/orders_summary.txt"
    fi
else
    echo "- No order ID available for status check" | tee -a "$LOG_DIR/orders_summary.txt"
fi

# Test 4: Order History
echo -e "\n[4] Testing Order History..." | tee -a "$LOG_DIR/orders_summary.txt"

RESPONSE=$(curl -s -w "\n%{http_code}\n%{time_total}" \
    -H "Authorization: Bearer $AUTH_TOKEN" \
    -H "Content-Type: application/json" \
    "$API_URL/orders?limit=10" 2>/dev/null || echo "000\n0")

HTTP_CODE=$(echo "$RESPONSE" | tail -n2 | head -n1)
RESPONSE_TIME=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-2)

echo "- Order History Status: $HTTP_CODE" | tee -a "$LOG_DIR/orders_summary.txt"
echo "- Response Time: ${RESPONSE_TIME}s" | tee -a "$LOG_DIR/orders_summary.txt"

ORDER_HISTORY_OK=false
if [ "$HTTP_CODE" = "200" ]; then
    ORDER_HISTORY_OK=true
    echo "$BODY" > "$LOG_DIR/order_history.json"
    
    # Count orders
    ORDER_COUNT=$(echo "$BODY" | jq -r '. | if type == "array" then length else .orders | length end' 2>/dev/null || echo "0")
    echo "- Orders in History: $ORDER_COUNT" | tee -a "$LOG_DIR/orders_summary.txt"
fi

# Test 5: Order Cancellation
echo -e "\n[5] Testing Order Cancellation..." | tee -a "$LOG_DIR/orders_summary.txt"

# Place a limit order to cancel
CANCEL_ORDER=$(cat <<EOF
{
    "symbol": "BTC-USD",
    "side": "buy",
    "type": "limit",
    "quantity": 0.001,
    "price": 40000.00
}
EOF
)

RESPONSE=$(curl -s -w "\n%{http_code}" \
    -X POST \
    -H "Authorization: Bearer $AUTH_TOKEN" \
    -H "Content-Type: application/json" \
    -d "$CANCEL_ORDER" \
    "$API_URL/orders" 2>/dev/null || echo "000")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-1)

CANCEL_OK=false
if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "201" ]; then
    CANCEL_ORDER_ID=$(echo "$BODY" | jq -r '.orderId // .order_id // .id // empty' 2>/dev/null)
    
    if [ -n "$CANCEL_ORDER_ID" ]; then
        sleep 1
        
        # Cancel the order
        RESPONSE=$(curl -s -w "\n%{http_code}" \
            -X DELETE \
            -H "Authorization: Bearer $AUTH_TOKEN" \
            "$API_URL/orders/$CANCEL_ORDER_ID" 2>/dev/null || echo "000")
        
        HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
        echo "- Cancel Order Status: $HTTP_CODE" | tee -a "$LOG_DIR/orders_summary.txt"
        
        if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "204" ]; then
            CANCEL_OK=true
        fi
    fi
fi

# Test 6: Order Flow Performance
echo -e "\n[6] Testing Order Flow Performance..." | tee -a "$LOG_DIR/orders_summary.txt"

# Test complete order flow timing
cat > "$LOG_DIR/order_flow_test.sh" << 'EOF'
#!/bin/bash
API_URL="$1"
AUTH_TOKEN="$2"
RESULTS_FILE="$3"

# Time complete order flow
START_TIME=$(date +%s%N)

# 1. Validate order
VALIDATE_RESP=$(curl -s -w "\n%{http_code}" \
    -X POST \
    -H "Authorization: Bearer $AUTH_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"symbol":"BTC-USD","side":"buy","type":"market","quantity":0.001}' \
    "$API_URL/orders/validate" 2>/dev/null)

VALIDATE_CODE=$(echo "$VALIDATE_RESP" | tail -n1)
VALIDATE_TIME=$(date +%s%N)

# 2. Place order
PLACE_RESP=$(curl -s -w "\n%{http_code}" \
    -X POST \
    -H "Authorization: Bearer $AUTH_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"symbol":"BTC-USD","side":"buy","type":"market","quantity":0.001}' \
    "$API_URL/orders" 2>/dev/null)

PLACE_CODE=$(echo "$PLACE_RESP" | tail -n1)
PLACE_TIME=$(date +%s%N)

# 3. Check status
ORDER_ID=$(echo "$PLACE_RESP" | head -n-1 | jq -r '.orderId // .order_id // .id // empty' 2>/dev/null)
if [ -n "$ORDER_ID" ]; then
    sleep 0.5
    STATUS_RESP=$(curl -s -w "\n%{http_code}" \
        -H "Authorization: Bearer $AUTH_TOKEN" \
        "$API_URL/orders/$ORDER_ID" 2>/dev/null)
    
    STATUS_CODE=$(echo "$STATUS_RESP" | tail -n1)
else
    STATUS_CODE="000"
fi
STATUS_TIME=$(date +%s%N)

# Calculate times in milliseconds
VALIDATE_MS=$((($VALIDATE_TIME - $START_TIME) / 1000000))
PLACE_MS=$((($PLACE_TIME - $VALIDATE_TIME) / 1000000))
STATUS_MS=$((($STATUS_TIME - $PLACE_TIME) / 1000000))
TOTAL_MS=$((($STATUS_TIME - $START_TIME) / 1000000))

echo "$(date +%s),$VALIDATE_CODE,$PLACE_CODE,$STATUS_CODE,$VALIDATE_MS,$PLACE_MS,$STATUS_MS,$TOTAL_MS" >> "$RESULTS_FILE"
EOF

chmod +x "$LOG_DIR/order_flow_test.sh"

# Run order flow tests
echo "Running order flow tests..." | tee -a "$LOG_DIR/orders_summary.txt"
for i in {1..10}; do
    "$LOG_DIR/order_flow_test.sh" "$API_URL" "$AUTH_TOKEN" "$LOG_DIR/order_flow_results.csv"
    sleep 1
done

# Analyze flow results
if [ -f "$LOG_DIR/order_flow_results.csv" ]; then
    awk -F',' '
        BEGIN { count=0; total_time=0; }
        {
            count++;
            total_time+=$8;
        }
        END {
            print "Order Flow Performance:";
            print "- Tests Run: " count;
            print "- Avg Total Flow Time: " (count > 0 ? total_time/count : 0) "ms";
        }
    ' "$LOG_DIR/order_flow_results.csv" | tee -a "$LOG_DIR/orders_summary.txt"
    
    AVG_FLOW_TIME=$(awk -F',' '{sum+=$8; count++} END {print (count > 0 ? sum/count : 999)}' "$LOG_DIR/order_flow_results.csv")
else
    AVG_FLOW_TIME=999
fi

PERFORMANCE_OK=$([ "${AVG_FLOW_TIME%.*}" -lt "1000" ] && echo "true" || echo "false")

# Generate summary
echo -e "\n=== Order Placement Test Summary ===" | tee -a "$LOG_DIR/orders_summary.txt"
echo "Test completed at: $(date)" | tee -a "$LOG_DIR/orders_summary.txt"

echo -e "\nComponent Status:" | tee -a "$LOG_DIR/orders_summary.txt"
echo "- Market Order Validation: $([ "$MARKET_VALIDATION_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/orders_summary.txt"
echo "- Limit Order Validation: $([ "$LIMIT_VALIDATION_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/orders_summary.txt"
echo "- Order Placement: $([ "$ORDER_PLACEMENT_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/orders_summary.txt"
echo "- Order Status Check: $([ "$ORDER_STATUS_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/orders_summary.txt"
echo "- Order History: $([ "$ORDER_HISTORY_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/orders_summary.txt"
echo "- Order Cancellation: $([ "$CANCEL_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/orders_summary.txt"
echo "- Flow Performance (<1s): $([ "$PERFORMANCE_OK" = "true" ] && echo "PASS" || echo "FAIL")" | tee -a "$LOG_DIR/orders_summary.txt"

# Generate JSON summary
cat > "$LOG_DIR/orders_results.json" << EOF
{
    "feature": "Order Placement",
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "components": {
        "marketValidation": $MARKET_VALIDATION_OK,
        "limitValidation": $LIMIT_VALIDATION_OK,
        "orderPlacement": $ORDER_PLACEMENT_OK,
        "orderStatus": $ORDER_STATUS_OK,
        "orderHistory": $ORDER_HISTORY_OK,
        "orderCancellation": $CANCEL_OK,
        "performance": $PERFORMANCE_OK
    },
    "metrics": {
        "avgFlowTime": ${AVG_FLOW_TIME%.*},
        "flowTestsRun": $([ -f "$LOG_DIR/order_flow_results.csv" ] && wc -l < "$LOG_DIR/order_flow_results.csv" || echo "0")
    },
    "passed": $([ "$MARKET_VALIDATION_OK" = "true" ] && [ "$ORDER_PLACEMENT_OK" = "true" ] && [ "$PERFORMANCE_OK" = "true" ] && echo "true" || echo "false")
}
EOF

echo -e "\nDetailed results saved to: $LOG_DIR" | tee -a "$LOG_DIR/orders_summary.txt"