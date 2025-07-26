#!/bin/bash

echo "=== Jackbot MVP Quick Test ==="
echo "Started at: $(date)"
echo

# Test results
PASS_COUNT=0
FAIL_COUNT=0

# Function to check test result
check_result() {
    local test_name="$1"
    local result="$2"
    if [ "$result" = "0" ]; then
        echo "✓ $test_name: PASS"
        ((PASS_COUNT++))
    else
        echo "✗ $test_name: FAIL"
        ((FAIL_COUNT++))
    fi
}

# Test 1: Frontend Accessibility
echo "[1] Testing Frontend Accessibility..."
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:3000 2>/dev/null || echo "000")
[ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "304" ]
check_result "Frontend HTTP Response" $?
echo "   HTTP Status: $HTTP_CODE"
echo

# Test 2: GraphQL Endpoint
echo "[2] Testing GraphQL Endpoint..."
GRAPHQL_RESPONSE=$(curl -s -X POST \
    -H "Content-Type: application/json" \
    -H "x-api-key: da2-fakeapikeyforthesimulator" \
    -d '{"query":"{ __typename }"}' \
    http://localhost:20002/graphql 2>/dev/null || echo "ERROR")

if [[ "$GRAPHQL_RESPONSE" == *"__typename"* ]] || [[ "$GRAPHQL_RESPONSE" == *"data"* ]]; then
    check_result "GraphQL Endpoint" 0
else
    check_result "GraphQL Endpoint" 1
fi
echo "   Response: ${GRAPHQL_RESPONSE:0:100}..."
echo

# Test 3: WebSocket Connectivity
echo "[3] Testing WebSocket Endpoint..."
# Simple connectivity test using curl upgrade headers
WS_TEST=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Upgrade: websocket" \
    -H "Connection: Upgrade" \
    http://localhost:20003/graphql 2>/dev/null || echo "000")

# WebSocket typically returns 101 (Switching Protocols) or 426 (Upgrade Required)
if [ "$WS_TEST" = "101" ] || [ "$WS_TEST" = "426" ] || [ "$WS_TEST" = "200" ]; then
    check_result "WebSocket Endpoint Reachable" 0
else
    check_result "WebSocket Endpoint Reachable" 1
fi
echo "   HTTP Status: $WS_TEST"
echo

# Test 4: Backend Services
echo "[4] Testing Backend Services..."
SERVICES=("postgres:5433" "redis:6379" "kafka:9092")
for service in "${SERVICES[@]}"; do
    IFS=':' read -r name port <<< "$service"
    nc -z localhost $port 2>/dev/null
    check_result "$name on port $port" $?
done
echo

# Test 5: Frontend Content Check
echo "[5] Testing Frontend Content..."
CONTENT=$(curl -s http://localhost:3000 2>/dev/null || echo "")
if [[ "$CONTENT" == *"<html"* ]] || [[ "$CONTENT" == *"<!DOCTYPE"* ]]; then
    check_result "Frontend HTML Content" 0
    
    # Check for React/Next.js
    if [[ "$CONTENT" == *"_next"* ]] || [[ "$CONTENT" == *"__NEXT_DATA__"* ]]; then
        check_result "Next.js Framework Detected" 0
    else
        check_result "Next.js Framework Detected" 1
    fi
else
    check_result "Frontend HTML Content" 1
fi
echo

# Test 6: Performance Check
echo "[6] Quick Performance Check..."
START_TIME=$(date +%s%N)
curl -s -o /dev/null http://localhost:3000
END_TIME=$(date +%s%N)
RESPONSE_TIME=$((($END_TIME - $START_TIME) / 1000000))
echo "   Frontend Load Time: ${RESPONSE_TIME}ms"

if [ $RESPONSE_TIME -lt 1000 ]; then
    check_result "Frontend Performance (<1s)" 0
else
    check_result "Frontend Performance (<1s)" 1
fi
echo

# Summary
echo "=== Test Summary ==="
echo "Total Tests: $(($PASS_COUNT + $FAIL_COUNT))"
echo "Passed: $PASS_COUNT"
echo "Failed: $FAIL_COUNT"
echo "Success Rate: $(( $PASS_COUNT * 100 / ($PASS_COUNT + $FAIL_COUNT) ))%"
echo
echo "Completed at: $(date)"

# Generate JSON summary
cat > mvp_test_results.json << EOF
{
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "summary": {
        "total": $(($PASS_COUNT + $FAIL_COUNT)),
        "passed": $PASS_COUNT,
        "failed": $FAIL_COUNT,
        "successRate": $(( $PASS_COUNT * 100 / ($PASS_COUNT + $FAIL_COUNT) ))
    },
    "environment": {
        "frontend": "http://localhost:3000",
        "graphql": "http://localhost:20002/graphql",
        "websocket": "ws://localhost:20003/graphql"
    },
    "performance": {
        "frontendLoadTime": $RESPONSE_TIME
    }
}
EOF

echo
echo "Results saved to: mvp_test_results.json"