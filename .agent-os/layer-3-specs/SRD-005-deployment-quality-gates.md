# SRD-005: Deployment & Quality Gates
**Status**: FINAL PHASE  
**Priority**: P0  
**Timeline**: Hours 9-12  
**Version**: 1.0.0  
**Last Updated**: 2025-07-26

## Executive Summary

This SRD defines the final deployment process and quality gates to achieve zero errors in production within 12 hours. It includes automated deployment pipelines, comprehensive quality checks, monitoring setup, and rollback procedures.

## Quality Gate Criteria

### Gate 1: Build Quality (Hour 9)
- **Compilation**: Zero errors, zero warnings
- **Tests**: 100% pass rate across all test suites
- **Coverage**: Minimum 80% code coverage
- **Linting**: Zero violations of agreed standards

### Gate 2: Performance Validation (Hour 10)
- **Latency**: P99 < 100ms for all operations
- **Throughput**: Sustained 10K msg/sec per exchange
- **Resource Usage**: Within defined limits
- **Memory Leaks**: Zero detected over 1-hour run

### Gate 3: Security Audit (Hour 10.5)
- **Dependencies**: No critical vulnerabilities
- **Secrets**: Zero hardcoded credentials
- **Permissions**: Least privilege enforced
- **Network**: TLS for all external connections

### Gate 4: Integration Verification (Hour 11)
- **E2E Tests**: All scenarios passing
- **Data Integrity**: Zero data loss scenarios
- **Failover**: Automatic recovery verified
- **Monitoring**: All alerts configured

## Deployment Pipeline

### CI/CD Configuration

**File**: `/.github/workflows/deploy.yml`

```yaml
name: Production Deployment

on:
  push:
    branches: [main]
  workflow_dispatch:
    inputs:
      environment:
        description: 'Deployment environment'
        required: true
        default: 'staging'
        type: choice
        options:
          - staging
          - production

env:
  RUST_VERSION: 1.75.0
  NODE_VERSION: 20.x

jobs:
  quality-gates:
    runs-on: ubuntu-latest
    outputs:
      deploy-approved: ${{ steps.gates.outputs.approved }}
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: ${{ env.RUST_VERSION }}
          components: rustfmt, clippy
      
      - name: Cache Dependencies
        uses: Swatinem/rust-cache@v2
      
      # Gate 1: Build Quality
      - name: Check Formatting
        run: cargo fmt --all -- --check
      
      - name: Run Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings
      
      - name: Build All
        run: cargo build --release --all
      
      - name: Run Tests
        run: |
          cargo test --all --release
          cargo test --doc
      
      - name: Check Coverage
        run: |
          cargo install cargo-tarpaulin
          cargo tarpaulin --out Xml --all --release
          
      - name: Upload Coverage
        uses: codecov/codecov-action@v3
        with:
          fail_ci_if_error: true
          
      # Gate 2: Performance Validation
      - name: Run Benchmarks
        run: |
          cargo bench --all
          python3 scripts/validate_benchmarks.py
      
      # Gate 3: Security Audit
      - name: Security Scan
        run: |
          cargo install cargo-audit
          cargo audit
          
      - name: Dependency Check
        run: |
          cargo tree --duplicate
          cargo outdated --exit-code 1
      
      - name: Secret Scanning
        uses: trufflesecurity/trufflehog@main
        with:
          path: ./
          
      # Gate 4: Integration Tests
      - name: Start Test Environment
        run: |
          docker-compose -f docker-compose.test.yml up -d
          ./scripts/wait-for-services.sh
          
      - name: Run Integration Tests
        run: |
          cargo test --test '*integration*' --release
          npm run test:e2e
          
      - name: Quality Gate Decision
        id: gates
        run: |
          python3 scripts/evaluate_quality_gates.py
          echo "approved=$?" >> $GITHUB_OUTPUT

  build-artifacts:
    needs: quality-gates
    if: needs.quality-gates.outputs.deploy-approved == 'true'
    runs-on: ubuntu-latest
    strategy:
      matrix:
        component: [sensor, backend, frontend]
    steps:
      - uses: actions/checkout@v4
      
      - name: Build Docker Image
        run: |
          docker build -t jackbot-${{ matrix.component }}:${{ github.sha }} \
            -f docker/${{ matrix.component }}/Dockerfile .
            
      - name: Push to Registry
        run: |
          echo ${{ secrets.DOCKER_PASSWORD }} | docker login -u ${{ secrets.DOCKER_USERNAME }} --password-stdin
          docker push jackbot-${{ matrix.component }}:${{ github.sha }}

  deploy-staging:
    needs: build-artifacts
    if: github.event.inputs.environment != 'production'
    runs-on: ubuntu-latest
    steps:
      - name: Deploy to Staging
        run: |
          kubectl set image deployment/jackbot-sensor \
            sensor=jackbot-sensor:${{ github.sha }} \
            -n staging
            
      - name: Wait for Rollout
        run: |
          kubectl rollout status deployment/jackbot-sensor -n staging
          
      - name: Run Smoke Tests
        run: |
          npm run test:smoke -- --env staging

  deploy-production:
    needs: build-artifacts
    if: github.event.inputs.environment == 'production'
    runs-on: ubuntu-latest
    environment: production
    steps:
      - name: Blue-Green Deployment
        run: |
          ./scripts/blue-green-deploy.sh ${{ github.sha }}
          
      - name: Health Check
        run: |
          ./scripts/health-check.sh production
          
      - name: Switch Traffic
        run: |
          ./scripts/switch-traffic.sh green
```

### Deployment Scripts

**File**: `/scripts/blue-green-deploy.sh`

```bash
#!/bin/bash
set -euo pipefail

VERSION=$1
ENVIRONMENT=${2:-production}

echo "Starting blue-green deployment for version $VERSION"

# Deploy to green environment
kubectl apply -f - <<EOF
apiVersion: apps/v1
kind: Deployment
metadata:
  name: jackbot-sensor-green
  namespace: $ENVIRONMENT
spec:
  replicas: 3
  selector:
    matchLabels:
      app: jackbot-sensor
      version: green
  template:
    metadata:
      labels:
        app: jackbot-sensor
        version: green
    spec:
      containers:
      - name: sensor
        image: jackbot-sensor:$VERSION
        env:
        - name: ENVIRONMENT
          value: $ENVIRONMENT
        resources:
          requests:
            memory: "1Gi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "1000m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
EOF

# Wait for green deployment
kubectl rollout status deployment/jackbot-sensor-green -n $ENVIRONMENT

# Run validation
echo "Running deployment validation..."
./scripts/validate-deployment.sh green

echo "Green deployment successful"
```

**File**: `/scripts/validate-deployment.sh`

```bash
#!/bin/bash
set -euo pipefail

DEPLOYMENT=$1
NAMESPACE=${2:-production}

echo "Validating $DEPLOYMENT deployment..."

# Check pod status
READY_PODS=$(kubectl get pods -l version=$DEPLOYMENT -n $NAMESPACE -o json | \
  jq '.items | map(select(.status.phase == "Running")) | length')
TOTAL_PODS=$(kubectl get pods -l version=$DEPLOYMENT -n $NAMESPACE -o json | \
  jq '.items | length')

if [ "$READY_PODS" != "$TOTAL_PODS" ]; then
  echo "ERROR: Not all pods are ready ($READY_PODS/$TOTAL_PODS)"
  exit 1
fi

# Check service endpoints
ENDPOINTS=$(kubectl get endpoints jackbot-sensor-$DEPLOYMENT -n $NAMESPACE -o json | \
  jq '.subsets[0].addresses | length')
  
if [ "$ENDPOINTS" -lt 1 ]; then
  echo "ERROR: No service endpoints available"
  exit 1
fi

# Run health checks
for i in {1..10}; do
  POD=$(kubectl get pods -l version=$DEPLOYMENT -n $NAMESPACE -o json | \
    jq -r '.items[0].metadata.name')
    
  HEALTH=$(kubectl exec $POD -n $NAMESPACE -- curl -s localhost:8080/health | \
    jq -r '.status')
    
  if [ "$HEALTH" != "healthy" ]; then
    echo "WARNING: Health check failed, attempt $i/10"
    sleep 5
  else
    echo "Health check passed"
    break
  fi
done

# Verify metrics
METRICS=$(kubectl exec $POD -n $NAMESPACE -- curl -s localhost:8080/metrics)
if ! echo "$METRICS" | grep -q "jackbot_messages_processed_total"; then
  echo "ERROR: Required metrics not exposed"
  exit 1
fi

echo "Deployment validation successful"
```

## Monitoring & Alerting

### Prometheus Configuration

**File**: `/monitoring/prometheus-rules.yml`

```yaml
groups:
  - name: jackbot_alerts
    interval: 30s
    rules:
      # Latency Alerts
      - alert: HighLatency
        expr: |
          histogram_quantile(0.99, 
            rate(jackbot_processing_latency_seconds_bucket[5m])
          ) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High message processing latency"
          description: "P99 latency is {{ $value }}s (threshold: 0.1s)"
      
      # Error Rate Alerts
      - alert: HighErrorRate
        expr: |
          rate(jackbot_errors_total[5m]) > 0.01
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "High error rate detected"
          description: "Error rate is {{ $value }} errors/sec"
      
      # WebSocket Connection Alerts
      - alert: WebSocketDisconnected
        expr: |
          jackbot_websocket_connected == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "WebSocket disconnected"
          description: "Exchange {{ $labels.exchange }} WebSocket is disconnected"
      
      # Resource Usage Alerts
      - alert: HighMemoryUsage
        expr: |
          container_memory_usage_bytes{pod=~"jackbot-.*"} / 
          container_spec_memory_limit_bytes > 0.9
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High memory usage"
          description: "Pod {{ $labels.pod }} memory usage is {{ $value | humanizePercentage }}"
```

### Grafana Dashboard

**File**: `/monitoring/dashboards/jackbot-production.json`

```json
{
  "dashboard": {
    "title": "Jackbot Production Dashboard",
    "panels": [
      {
        "title": "Message Processing Rate",
        "targets": [
          {
            "expr": "rate(jackbot_messages_processed_total[5m])",
            "legendFormat": "{{ exchange }}"
          }
        ],
        "gridPos": {"h": 8, "w": 12, "x": 0, "y": 0}
      },
      {
        "title": "Processing Latency",
        "targets": [
          {
            "expr": "histogram_quantile(0.99, rate(jackbot_processing_latency_seconds_bucket[5m]))",
            "legendFormat": "P99"
          },
          {
            "expr": "histogram_quantile(0.95, rate(jackbot_processing_latency_seconds_bucket[5m]))",
            "legendFormat": "P95"
          }
        ],
        "gridPos": {"h": 8, "w": 12, "x": 12, "y": 0}
      },
      {
        "title": "Error Rate",
        "targets": [
          {
            "expr": "rate(jackbot_errors_total[5m])",
            "legendFormat": "{{ error_type }}"
          }
        ],
        "gridPos": {"h": 8, "w": 12, "x": 0, "y": 8}
      },
      {
        "title": "WebSocket Status",
        "targets": [
          {
            "expr": "jackbot_websocket_connected",
            "legendFormat": "{{ exchange }}"
          }
        ],
        "gridPos": {"h": 8, "w": 12, "x": 12, "y": 8}
      }
    ]
  }
}
```

## Rollback Procedures

### Automated Rollback

**File**: `/scripts/auto-rollback.sh`

```bash
#!/bin/bash
set -euo pipefail

ENVIRONMENT=$1
THRESHOLD_ERROR_RATE=0.05
THRESHOLD_LATENCY=0.2
CHECK_DURATION=300 # 5 minutes

echo "Monitoring deployment for automatic rollback..."

START_TIME=$(date +%s)

while true; do
  CURRENT_TIME=$(date +%s)
  ELAPSED=$((CURRENT_TIME - START_TIME))
  
  if [ $ELAPSED -gt $CHECK_DURATION ]; then
    echo "Monitoring period complete, deployment stable"
    exit 0
  fi
  
  # Check error rate
  ERROR_RATE=$(curl -s http://prometheus:9090/api/v1/query \
    --data-urlencode 'query=rate(jackbot_errors_total[1m])' | \
    jq -r '.data.result[0].value[1]' || echo "0")
    
  if (( $(echo "$ERROR_RATE > $THRESHOLD_ERROR_RATE" | bc -l) )); then
    echo "ERROR: Error rate $ERROR_RATE exceeds threshold $THRESHOLD_ERROR_RATE"
    ./scripts/switch-traffic.sh blue
    exit 1
  fi
  
  # Check latency
  LATENCY=$(curl -s http://prometheus:9090/api/v1/query \
    --data-urlencode 'query=histogram_quantile(0.99, rate(jackbot_processing_latency_seconds_bucket[1m]))' | \
    jq -r '.data.result[0].value[1]' || echo "0")
    
  if (( $(echo "$LATENCY > $THRESHOLD_LATENCY" | bc -l) )); then
    echo "ERROR: Latency $LATENCY exceeds threshold $THRESHOLD_LATENCY"
    ./scripts/switch-traffic.sh blue
    exit 1
  fi
  
  echo "Metrics OK - Error Rate: $ERROR_RATE, Latency: $LATENCY"
  sleep 30
done
```

### Manual Rollback

```bash
# Quick rollback to previous version
kubectl rollout undo deployment/jackbot-sensor -n production

# Rollback to specific revision
kubectl rollout undo deployment/jackbot-sensor --to-revision=42 -n production

# Emergency shutdown
kubectl scale deployment/jackbot-sensor --replicas=0 -n production
```

## Post-Deployment Validation

### Smoke Tests

**File**: `/tests/smoke/production_smoke.rs`

```rust
#[tokio::test]
async fn smoke_test_production() {
    let client = reqwest::Client::new();
    
    // Health check
    let health = client.get("https://api.jackbot.io/health")
        .send()
        .await
        .expect("Health check failed");
    assert_eq!(health.status(), 200);
    
    // WebSocket connectivity
    let ws_status = client.get("https://api.jackbot.io/ws/status")
        .send()
        .await
        .expect("WebSocket status failed");
    let ws_data: HashMap<String, bool> = ws_status.json().await.unwrap();
    assert!(ws_data.values().all(|&v| v));
    
    // Sample market data
    let market_data = client.get("https://api.jackbot.io/market/BTCUSDT")
        .send()
        .await
        .expect("Market data request failed");
    assert_eq!(market_data.status(), 200);
    
    // Metrics endpoint
    let metrics = client.get("https://api.jackbot.io/metrics")
        .send()
        .await
        .expect("Metrics request failed");
    assert!(metrics.text().await.unwrap().contains("jackbot_messages_processed_total"));
}
```

### Performance Validation

```rust
#[tokio::test]
async fn validate_production_performance() {
    let start = Instant::now();
    let mut latencies = Vec::new();
    
    // Send 1000 requests
    for _ in 0..1000 {
        let request_start = Instant::now();
        
        let response = reqwest::get("https://api.jackbot.io/market/BTCUSDT")
            .await
            .expect("Request failed");
            
        latencies.push(request_start.elapsed());
        
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    
    // Calculate percentiles
    latencies.sort();
    let p50 = latencies[500];
    let p95 = latencies[950];
    let p99 = latencies[990];
    
    println!("Latency - P50: {:?}, P95: {:?}, P99: {:?}", p50, p95, p99);
    
    assert!(p99 < Duration::from_millis(100), "P99 latency exceeds 100ms");
}
```

## Success Metrics Dashboard

### Real-time KPIs
- **Zero Error Achievement**: Error count in last hour
- **Latency Compliance**: P99 < 100ms sustained
- **Uptime**: Minutes since last incident
- **Message Throughput**: Current msgs/sec
- **Active Connections**: WebSocket connection count

### Historical Metrics
- **Error Trend**: 24-hour error rate graph
- **Performance Trend**: Latency percentiles over time
- **Resource Usage**: CPU/Memory utilization
- **Deployment History**: Recent deployments and outcomes

## Deployment Checklist

### Pre-Deployment (Hour 9)
- [ ] All quality gates passed
- [ ] Security scan completed
- [ ] Performance benchmarks validated
- [ ] Integration tests successful
- [ ] Rollback plan reviewed

### Deployment (Hour 10-11)
- [ ] Blue environment deployed
- [ ] Health checks passing
- [ ] Smoke tests successful
- [ ] Metrics validated
- [ ] Gradual traffic shift completed

### Post-Deployment (Hour 11-12)
- [ ] Production smoke tests passed
- [ ] Performance validation complete
- [ ] Monitoring alerts configured
- [ ] Auto-rollback enabled
- [ ] Documentation updated

## Emergency Procedures

### Incident Response
1. **Immediate Actions**
   - Trigger rollback if error rate > 5%
   - Page on-call engineer
   - Create incident channel

2. **Investigation**
   - Check deployment logs
   - Review recent commits
   - Analyze error patterns

3. **Resolution**
   - Apply hotfix if simple
   - Full rollback if complex
   - Update monitoring

### Communication Plan
- **Internal**: Slack #incidents channel
- **External**: Status page updates
- **Executive**: Email updates every 30 min

## Final Validation

The deployment is considered successful when:
1. Zero errors for 60 consecutive minutes
2. All performance metrics within thresholds
3. All health checks passing
4. No manual interventions required
5. Positive feedback from initial users

This completes the zero-error deployment within the 12-hour timeline.