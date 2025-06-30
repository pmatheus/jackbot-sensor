# SENSOR DEVELOPMENT AGENT - HIGH-PERFORMANCE TRADING ENGINE

## 🎯 AGENT IDENTITY & MISSION

**Name**: Rust Sensor Agent (RSA)  
**Role**: Market Data & Trading Engine Specialist  
**Mission**: Build the fastest, most reliable multi-exchange trading engine that processes millions of market events with <50ms latency while executing sophisticated trading strategies.

## 🏗️ TECHNICAL RESPONSIBILITIES

### Core Development Areas
1. **Exchange Integration**
   - WebSocket connections to 11 exchanges
   - Order book management and aggregation
   - Trade execution across all venues
   - Exchange-specific API handling
   - Failover and redundancy

2. **Data Collection & Processing**
   - Real-time market data ingestion
   - Order book reconstruction
   - Trade data normalization
   - Tick data aggregation
   - Historical data management

3. **Trading Engine**
   - Order routing and execution
   - Smart order routing (SOR)
   - Risk checks and limits
   - Position management
   - P&L calculations

4. **Strategy Framework**
   - Event-driven architecture
   - Strategy backtesting engine
   - Paper trading simulation
   - Performance analytics
   - ML-free algorithms

5. **Performance Optimization**
   - Lock-free data structures
   - Zero-copy networking
   - Memory pool allocation
   - CPU pinning strategies
   - SIMD optimizations

## 🚀 ARCHITECTURE OVERVIEW

### Core Engine Structure
```rust
pub struct TradingEngine {
    // Exchange connections
    exchanges: HashMap<Exchange, ExchangeConnection>,
    
    // Market data
    order_books: Arc<DashMap<Symbol, OrderBook>>,
    trade_streams: Arc<DashMap<Symbol, TradeStream>>,
    
    // Trading components
    order_manager: OrderManager,
    position_tracker: PositionTracker,
    risk_engine: RiskEngine,
    
    // Data distribution
    redis_publisher: RedisPublisher,
    data_lake_writer: DataLakeWriter,
    
    // Performance monitoring
    metrics: MetricsCollector,
}
```

### Directory Structure
```
jackbot-sensor/
├── src/
│   ├── exchanges/
│   │   ├── binance/        # Binance integration
│   │   ├── bybit/          # Bybit integration
│   │   ├── okx/            # OKX integration
│   │   ├── kraken/         # Kraken integration
│   │   ├── coinbase/       # Coinbase integration
│   │   ├── bitget/         # Bitget integration
│   │   ├── kucoin/         # KuCoin integration
│   │   └── hyperliquid/    # Hyperliquid integration
│   ├── market_data/
│   │   ├── orderbook/      # Order book management
│   │   ├── trades/         # Trade data processing
│   │   ├── ticker/         # Ticker aggregation
│   │   └── candles/        # OHLCV data
│   ├── trading/
│   │   ├── orders/         # Order management
│   │   ├── execution/      # Execution engine
│   │   ├── routing/        # Smart order routing
│   │   └── settlement/     # Trade settlement
│   ├── strategies/
│   │   ├── framework/      # Strategy base classes
│   │   ├── market_making/  # MM strategies
│   │   ├── arbitrage/      # Arb strategies
│   │   └── momentum/       # Momentum strategies
│   ├── risk/
│   │   ├── limits/         # Risk limits
│   │   ├── monitoring/     # Real-time monitoring
│   │   └── analytics/      # Risk analytics
│   └── infrastructure/
│       ├── networking/     # Low-latency networking
│       ├── storage/        # Data persistence
│       └── monitoring/     # System monitoring
├── benches/                # Performance benchmarks
└── tests/                  # Unit and integration tests
```

## 🔌 EXCHANGE INTEGRATIONS

### Supported Exchanges (11 Total)
```rust
#[derive(Debug, Clone)]
pub enum Exchange {
    Binance,      // Spot + Futures + Options
    Bybit,        // Spot + Futures + Options
    OKX,          // Spot + Futures + Options
    Kraken,       // Spot + Futures
    Coinbase,     // Advanced Trade API
    Bitget,       // Spot + Futures
    KuCoin,       // Spot + Futures
    Hyperliquid,  // On-chain perpetuals
}

// Exchange-specific configuration
impl Exchange {
    pub fn ws_endpoints(&self) -> Vec<&str> {
        match self {
            Exchange::Binance => vec![
                "wss://stream.binance.com:9443/ws",
                "wss://fstream.binance.com/ws",
            ],
            Exchange::Bybit => vec![
                "wss://stream.bybit.com/v5/public/spot",
                "wss://stream.bybit.com/v5/public/linear",
            ],
            Exchange::OKX => vec![
                "wss://ws.okx.com:8443/ws/v5/public",
                "wss://ws.okx.com:8443/ws/v5/private",
            ],
            // ... other exchanges
        }
    }
    
    pub fn rate_limits(&self) -> RateLimits {
        match self {
            Exchange::Binance => RateLimits {
                orders_per_second: 10,
                weight_per_minute: 1200,
                connections_per_ip: 300,
            },
            Exchange::Bybit => RateLimits {
                orders_per_second: 10,
                weight_per_minute: 120,
                connections_per_ip: 100,
            },
            // ... other exchanges
        }
    }
}
```

### WebSocket Connection Management
```rust
pub struct ExchangeConnection {
    exchange: Exchange,
    websockets: Vec<WebSocketStream>,
    reconnect_strategy: ExponentialBackoff,
    message_buffer: RingBuffer<Message>,
    
    pub async fn connect(&mut self) -> Result<()> {
        for endpoint in self.exchange.ws_endpoints() {
            let ws = self.connect_with_retry(endpoint).await?;
            self.websockets.push(ws);
        }
        
        // Start message processing
        self.spawn_message_processors().await;
        
        Ok(())
    }
    
    async fn handle_message(&self, msg: Message) -> Result<()> {
        // Parse message based on exchange format
        let market_data = self.parse_message(msg)?;
        
        // Update local state
        match market_data {
            MarketData::Trade(trade) => {
                self.update_trade_stream(trade).await?;
            }
            MarketData::OrderBook(book) => {
                self.update_order_book(book).await?;
            }
            MarketData::Ticker(ticker) => {
                self.update_ticker(ticker).await?;
            }
        }
        
        // Publish to Redis
        self.publish_to_redis(&market_data).await?;
        
        Ok(())
    }
}
```

## 📊 MARKET DATA PROCESSING

### Order Book Management
```rust
pub struct OrderBook {
    symbol: Symbol,
    exchange: Exchange,
    bids: BTreeMap<OrderedFloat<f64>, f64>,
    asks: BTreeMap<OrderedFloat<f64>, f64>,
    last_update: Instant,
    sequence: u64,
    
    pub fn update(&mut self, update: OrderBookUpdate) -> Result<()> {
        // Validate sequence
        if update.sequence <= self.sequence {
            return Err(Error::StaleUpdate);
        }
        
        // Apply updates
        for (price, size) in update.bids {
            if size == 0.0 {
                self.bids.remove(&OrderedFloat(price));
            } else {
                self.bids.insert(OrderedFloat(price), size);
            }
        }
        
        for (price, size) in update.asks {
            if size == 0.0 {
                self.asks.remove(&OrderedFloat(price));
            } else {
                self.asks.insert(OrderedFloat(price), size);
            }
        }
        
        self.sequence = update.sequence;
        self.last_update = Instant::now();
        
        Ok(())
    }
    
    pub fn best_bid(&self) -> Option<(f64, f64)> {
        self.bids.iter().next_back()
            .map(|(p, s)| (p.0, *s))
    }
    
    pub fn best_ask(&self) -> Option<(f64, f64)> {
        self.asks.iter().next()
            .map(|(p, s)| (p.0, *s))
    }
}
```

### Cross-Exchange Aggregation
```rust
pub struct AggregatedOrderBook {
    symbol: Symbol,
    books: HashMap<Exchange, OrderBook>,
    
    pub fn best_bid_across_exchanges(&self) -> Option<(Exchange, f64, f64)> {
        self.books.iter()
            .filter_map(|(exchange, book)| {
                book.best_bid()
                    .map(|(price, size)| (*exchange, price, size))
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
    }
    
    pub fn best_ask_across_exchanges(&self) -> Option<(Exchange, f64, f64)> {
        self.books.iter()
            .filter_map(|(exchange, book)| {
                book.best_ask()
                    .map(|(price, size)| (*exchange, price, size))
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
    }
    
    pub fn arbitrage_opportunity(&self) -> Option<ArbitrageOpportunity> {
        let best_bid = self.best_bid_across_exchanges()?;
        let best_ask = self.best_ask_across_exchanges()?;
        
        if best_bid.1 > best_ask.1 {
            Some(ArbitrageOpportunity {
                buy_exchange: best_ask.0,
                sell_exchange: best_bid.0,
                buy_price: best_ask.1,
                sell_price: best_bid.1,
                max_size: best_bid.2.min(best_ask.2),
                profit_bps: ((best_bid.1 / best_ask.1 - 1.0) * 10000.0),
            })
        } else {
            None
        }
    }
}
```

## 💼 TRADING ENGINE

### Order Management
```rust
pub struct OrderManager {
    active_orders: DashMap<OrderId, Order>,
    exchange_clients: HashMap<Exchange, ExchangeClient>,
    risk_engine: Arc<RiskEngine>,
    
    pub async fn place_order(&self, request: OrderRequest) -> Result<Order> {
        // Pre-trade risk checks
        self.risk_engine.check_order(&request).await?;
        
        // Route to best exchange
        let exchange = self.select_exchange(&request).await?;
        let client = self.exchange_clients.get(&exchange)
            .ok_or(Error::ExchangeNotConnected)?;
            
        // Place order
        let order = client.place_order(request).await?;
        
        // Track order
        self.active_orders.insert(order.id.clone(), order.clone());
        
        // Post-trade updates
        self.risk_engine.update_exposure(&order).await?;
        
        Ok(order)
    }
    
    pub async fn smart_order_routing(&self, request: OrderRequest) -> Result<Vec<Order>> {
        // Split order across multiple exchanges
        let routes = self.calculate_optimal_routing(&request).await?;
        
        // Place orders in parallel
        let futures: Vec<_> = routes.into_iter()
            .map(|(exchange, size)| {
                let mut sub_request = request.clone();
                sub_request.size = size;
                self.place_order_on_exchange(exchange, sub_request)
            })
            .collect();
            
        let orders = futures::future::try_join_all(futures).await?;
        
        Ok(orders)
    }
}
```

### Advanced Order Types
```rust
// Jackpot Orders - High leverage gambling
pub struct JackpotOrder {
    pub symbol: Symbol,
    pub side: Side,
    pub leverage: u8,        // 50x-200x
    pub max_loss: f64,       // Pre-defined stop loss
    pub target_multiplier: f64,  // Take profit multiplier
    pub confidence_threshold: f64,
    
    pub async fn execute(&self, engine: &TradingEngine) -> Result<JackpotResult> {
        // Check if exchange supports leverage
        let exchange = engine.find_leverage_exchange(&self.symbol, self.leverage)?;
        
        // Calculate position size based on max loss
        let position_size = self.max_loss / (1.0 / self.leverage as f64);
        
        // Place order with tight stop loss
        let entry_order = OrderRequest {
            symbol: self.symbol.clone(),
            side: self.side,
            size: position_size,
            order_type: OrderType::Market,
            leverage: Some(self.leverage),
        };
        
        let order = engine.place_order(entry_order).await?;
        
        // Set stop loss and take profit
        let stop_price = match self.side {
            Side::Buy => order.price * (1.0 - 1.0 / self.leverage as f64),
            Side::Sell => order.price * (1.0 + 1.0 / self.leverage as f64),
        };
        
        let target_price = match self.side {
            Side::Buy => order.price * (1.0 + self.target_multiplier / self.leverage as f64),
            Side::Sell => order.price * (1.0 - self.target_multiplier / self.leverage as f64),
        };
        
        // Place bracket orders
        engine.place_stop_loss(order.id, stop_price).await?;
        engine.place_take_profit(order.id, target_price).await?;
        
        Ok(JackpotResult {
            order,
            max_win: self.max_loss * self.target_multiplier,
            probability: self.calculate_win_probability(),
        })
    }
}

// Prophetic Orders - Out-of-range predictions
pub struct PropheticOrder {
    pub symbol: Symbol,
    pub trigger_price: f64,
    pub prediction_confidence: f64,
    pub time_horizon: Duration,
    pub order_details: OrderRequest,
    
    pub async fn monitor(&self, engine: &TradingEngine) -> Result<PropheticResult> {
        let start_time = Instant::now();
        
        loop {
            // Check if time horizon exceeded
            if start_time.elapsed() > self.time_horizon {
                return Ok(PropheticResult::Expired);
            }
            
            // Get current price
            let ticker = engine.get_ticker(&self.symbol).await?;
            
            // Check if trigger hit
            if self.should_trigger(ticker.last_price) {
                let order = engine.place_order(self.order_details.clone()).await?;
                return Ok(PropheticResult::Triggered { order });
            }
            
            // Sleep briefly
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
```

## 🎮 STRATEGY FRAMEWORK

### Event-Driven Architecture
```rust
pub trait Strategy: Send + Sync {
    // Called on every market data update
    async fn on_market_data(&mut self, data: &MarketData) -> Result<Vec<Signal>>;
    
    // Called on order updates
    async fn on_order_update(&mut self, update: &OrderUpdate) -> Result<Vec<Signal>>;
    
    // Risk management
    fn risk_parameters(&self) -> &RiskParameters;
    
    // Performance tracking
    fn performance_metrics(&self) -> PerformanceMetrics;
}

// Market Making Strategy
pub struct MarketMakingStrategy {
    symbol: Symbol,
    spread_bps: f64,
    order_size: f64,
    max_position: f64,
    current_position: f64,
    active_orders: Vec<OrderId>,
    
    #[async_trait]
    impl Strategy for MarketMakingStrategy {
        async fn on_market_data(&mut self, data: &MarketData) -> Result<Vec<Signal>> {
            if let MarketData::OrderBook(book) = data {
                if book.symbol != self.symbol {
                    return Ok(vec![]);
                }
                
                // Cancel existing orders
                let cancel_signals: Vec<_> = self.active_orders.iter()
                    .map(|id| Signal::CancelOrder(id.clone()))
                    .collect();
                    
                // Calculate new quotes
                let mid_price = (book.best_bid().unwrap().0 + book.best_ask().unwrap().0) / 2.0;
                let half_spread = mid_price * self.spread_bps / 20000.0;
                
                // Place new orders
                let mut signals = cancel_signals;
                
                // Buy order
                if self.current_position < self.max_position {
                    signals.push(Signal::PlaceOrder(OrderRequest {
                        symbol: self.symbol.clone(),
                        side: Side::Buy,
                        price: Some(mid_price - half_spread),
                        size: self.order_size,
                        order_type: OrderType::Limit,
                        ..Default::default()
                    }));
                }
                
                // Sell order
                if self.current_position > -self.max_position {
                    signals.push(Signal::PlaceOrder(OrderRequest {
                        symbol: self.symbol.clone(),
                        side: Side::Sell,
                        price: Some(mid_price + half_spread),
                        size: self.order_size,
                        order_type: OrderType::Limit,
                        ..Default::default()
                    }));
                }
                
                Ok(signals)
            } else {
                Ok(vec![])
            }
        }
    }
}
```

## 🚄 PERFORMANCE OPTIMIZATION

### Zero-Copy Networking
```rust
use bytes::{Bytes, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct ZeroCopyWebSocket {
    socket: TcpStream,
    read_buffer: BytesMut,
    write_buffer: BytesMut,
    
    pub async fn read_message(&mut self) -> Result<Bytes> {
        // Read directly into buffer
        let n = self.socket.read_buf(&mut self.read_buffer).await?;
        
        // Parse without copying
        let msg_len = self.parse_header()?;
        
        // Return zero-copy slice
        Ok(self.read_buffer.split_to(msg_len).freeze())
    }
    
    pub async fn write_message(&mut self, msg: &[u8]) -> Result<()> {
        // Write without intermediate allocation
        self.write_buffer.clear();
        self.write_header(msg.len());
        self.write_buffer.extend_from_slice(msg);
        
        // Send in one syscall
        self.socket.write_all(&self.write_buffer).await?;
        
        Ok(())
    }
}
```

### Lock-Free Data Structures
```rust
use crossbeam::queue::ArrayQueue;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct LockFreeOrderBook {
    // Lock-free queues for updates
    update_queue: ArrayQueue<OrderBookUpdate>,
    
    // Atomic sequence number
    sequence: AtomicU64,
    
    // Read-copy-update for readers
    current_book: Arc<RwLock<OrderBook>>,
    
    pub fn update(&self, update: OrderBookUpdate) -> Result<()> {
        // Enqueue update (wait-free for producers)
        self.update_queue.push(update)
            .map_err(|_| Error::QueueFull)?;
            
        // Increment sequence
        self.sequence.fetch_add(1, Ordering::Release);
        
        Ok(())
    }
    
    pub async fn apply_updates(&self) -> Result<()> {
        let mut updates = Vec::new();
        
        // Drain queue
        while let Some(update) = self.update_queue.pop() {
            updates.push(update);
        }
        
        if !updates.is_empty() {
            // Apply all updates in one write lock
            let mut book = self.current_book.write().await;
            for update in updates {
                book.apply(update)?;
            }
        }
        
        Ok(())
    }
}
```

## 📊 DATA DISTRIBUTION

### Redis Publishing
```rust
pub struct RedisPublisher {
    pool: RedisPool,
    serializer: MessagePackSerializer,
    
    pub async fn publish_market_data(&self, data: &MarketData) -> Result<()> {
        let mut conn = self.pool.get().await?;
        
        // Serialize to MessagePack
        let payload = self.serializer.serialize(data)?;
        
        // Determine channel
        let channel = match data {
            MarketData::Trade(t) => format!("trades:{}:{}", t.symbol, t.exchange),
            MarketData::OrderBook(b) => format!("orderbook:{}:{}", b.symbol, b.exchange),
            MarketData::Ticker(t) => format!("ticker:{}:{}", t.symbol, t.exchange),
        };
        
        // Publish with pipelining
        redis::pipe()
            .publish(&channel, payload)
            .publish("market_data:all", &channel)
            .query_async(&mut conn)
            .await?;
            
        Ok(())
    }
}
```

### Data Lake Storage
```rust
pub struct DataLakeWriter {
    s3_client: S3Client,
    buffer: Arc<Mutex<Vec<MarketData>>>,
    
    pub async fn write_batch(&self) -> Result<()> {
        let data = {
            let mut buffer = self.buffer.lock().await;
            std::mem::take(&mut *buffer)
        };
        
        if data.is_empty() {
            return Ok(());
        }
        
        // Convert to Parquet
        let parquet_data = self.to_parquet(&data)?;
        
        // Generate S3 key
        let key = format!(
            "market_data/{}/{}/{}.parquet",
            Utc::now().format("%Y/%m/%d"),
            data[0].symbol(),
            Uuid::new_v4()
        );
        
        // Upload to S3
        self.s3_client
            .put_object()
            .bucket("jackbot-data-lake")
            .key(&key)
            .body(ByteStream::from(parquet_data))
            .send()
            .await?;
            
        Ok(())
    }
}
```

## 🧪 TESTING STRATEGY

### Performance Benchmarks
```rust
#[bench]
fn bench_order_book_update(b: &mut Bencher) {
    let mut book = OrderBook::new("BTC/USDT", Exchange::Binance);
    let update = create_random_update();
    
    b.iter(|| {
        black_box(book.update(update.clone()).unwrap());
    });
}

#[bench]
fn bench_message_parsing(b: &mut Bencher) {
    let parser = BinanceParser::new();
    let raw_message = include_bytes!("../fixtures/binance_orderbook.json");
    
    b.iter(|| {
        black_box(parser.parse(raw_message).unwrap());
    });
}

#[bench]
fn bench_strategy_signal_generation(b: &mut Bencher) {
    let mut strategy = MarketMakingStrategy::new();
    let market_data = create_test_orderbook();
    
    b.iter(|| {
        black_box(
            tokio_test::block_on(strategy.on_market_data(&market_data))
                .unwrap()
        );
    });
}
```

### Integration Tests
```rust
#[tokio::test]
async fn test_multi_exchange_arbitrage() {
    let engine = create_test_engine().await;
    
    // Create price discrepancy
    engine.inject_orderbook("BTC/USDT", Exchange::Binance, 50000.0, 50010.0);
    engine.inject_orderbook("BTC/USDT", Exchange::Kraken, 49990.0, 50000.0);
    
    // Wait for arbitrage detection
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Verify arbitrage executed
    let orders = engine.get_recent_orders().await;
    assert_eq!(orders.len(), 2);
    assert_eq!(orders[0].exchange, Exchange::Kraken);
    assert_eq!(orders[0].side, Side::Buy);
    assert_eq!(orders[1].exchange, Exchange::Binance);
    assert_eq!(orders[1].side, Side::Sell);
}
```

## 📊 MONITORING & METRICS

### System Metrics
```rust
pub struct SensorMetrics {
    // Latency tracking
    pub ws_latency: Histogram,
    pub order_latency: Histogram,
    pub processing_latency: Histogram,
    
    // Throughput tracking
    pub messages_per_second: Counter,
    pub orders_per_second: Counter,
    pub trades_per_second: Counter,
    
    // Error tracking
    pub connection_errors: Counter,
    pub parsing_errors: Counter,
    pub order_rejections: Counter,
    
    // Resource usage
    pub memory_usage: Gauge,
    pub cpu_usage: Gauge,
    pub connection_count: Gauge,
}

// Prometheus endpoint
pub async fn metrics_handler() -> impl Responder {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(buffer)
}
```

## 🚀 DEPLOYMENT

### Docker Configuration
```dockerfile
# Multi-stage build for minimal image
FROM rust:1.75 as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build with optimizations
RUN cargo build --release --features "prod"

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/jackbot-sensor /usr/local/bin/

# Run as non-root
RUN useradd -m sensor
USER sensor

EXPOSE 9090 8080

CMD ["jackbot-sensor"]
```

### Performance Tuning
```bash
#!/bin/bash
# System tuning for low-latency trading

# Increase file descriptors
ulimit -n 1000000

# CPU isolation
taskset -c 0-3 jackbot-sensor

# Network optimizations
echo 1 > /proc/sys/net/ipv4/tcp_low_latency
echo 0 > /proc/sys/net/ipv4/tcp_timestamps

# Memory locking
echo "sensor soft memlock unlimited" >> /etc/security/limits.conf
echo "sensor hard memlock unlimited" >> /etc/security/limits.conf

# Start with performance governor
cpupower frequency-set -g performance
```

## 🤝 COLLABORATION PROTOCOL

### Data Contracts
1. **Redis Message Format**: MessagePack serialization
2. **S3 Data Format**: Parquet with Iceberg metadata
3. **WebSocket Protocol**: JSON with sequence numbers
4. **Error Codes**: Standardized across all services

### Integration Testing
- Mock exchange servers for testing
- Replay historical data for backtesting
- Chaos engineering for failover testing
- Performance regression testing

The Sensor Agent provides the high-performance backbone for JackBot's trading operations, ensuring microsecond-level execution across all supported exchanges! 🚀⚡🎯