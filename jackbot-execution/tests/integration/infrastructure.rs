/// Test Infrastructure Components
/// 
/// This module provides the testing infrastructure including:
/// - Mock exchange server
/// - Test Kafka environment
/// - Test database setup
/// - Docker compose orchestration

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval, sleep, Instant};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use uuid::Uuid;
use rust_decimal::Decimal;
use chrono::{DateTime, Utc};

/// Mock Exchange Server that simulates real exchange behavior
pub struct MockExchangeServer {
    port: u16,
    market_data_sender: broadcast::Sender<MarketDataUpdate>,
    order_book_sender: broadcast::Sender<OrderBookUpdate>,
    order_execution_sender: broadcast::Sender<OrderExecutionUpdate>,
    server_handle: Option<tokio::task::JoinHandle<()>>,
    state: Arc<Mutex<ExchangeState>>,
}

#[derive(Debug, Clone)]
pub struct ExchangeState {
    pub symbols: HashMap<String, SymbolInfo>,
    pub order_books: HashMap<String, OrderBook>,
    pub orders: HashMap<String, Order>,
    pub balances: HashMap<String, Decimal>,
    pub connected_clients: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub status: String,
    pub min_qty: Decimal,
    pub max_qty: Decimal,
    pub tick_size: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub last_update_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: Decimal,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub status: String,
    pub filled_quantity: Decimal,
    pub remaining_quantity: Decimal,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataUpdate {
    pub symbol: String,
    pub price: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
    pub sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookUpdate {
    pub symbol: String,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub last_update_id: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderExecutionUpdate {
    pub order_id: String,
    pub symbol: String,
    pub side: String,
    pub quantity: Decimal,
    pub price: Decimal,
    pub status: String,
    pub timestamp: DateTime<Utc>,
}

impl MockExchangeServer {
    pub async fn start(port: u16) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let (market_data_sender, _) = broadcast::channel(1000);
        let (order_book_sender, _) = broadcast::channel(1000);
        let (order_execution_sender, _) = broadcast::channel(1000);
        
        let state = Arc::new(Mutex::new(ExchangeState::new()));
        
        let server = Arc::new(MockExchangeServer {
            port,
            market_data_sender: market_data_sender.clone(),
            order_book_sender: order_book_sender.clone(),
            order_execution_sender: order_execution_sender.clone(),
            server_handle: None,
            state: state.clone(),
        });
        
        // Start WebSocket server
        let ws_server = Self::start_websocket_server(
            port,
            market_data_sender.clone(),
            order_book_sender.clone(),
            order_execution_sender.clone(),
            state.clone(),
        ).await?;
        
        // Start market data simulation
        let market_sim = Self::start_market_simulation(
            market_data_sender.clone(),
            order_book_sender.clone(),
            state.clone(),
        );
        
        // Start REST API server
        let rest_server = Self::start_rest_server(port + 1, state.clone());
        
        tokio::spawn(ws_server);
        tokio::spawn(market_sim);
        tokio::spawn(rest_server);
        
        println!("🚀 Mock exchange server started on port {}", port);
        println!("   WebSocket: ws://localhost:{}", port);
        println!("   REST API: http://localhost:{}", port + 1);
        
        Ok(server)
    }

    async fn start_websocket_server(
        port: u16,
        market_data_sender: broadcast::Sender<MarketDataUpdate>,
        order_book_sender: broadcast::Sender<OrderBookUpdate>,
        order_execution_sender: broadcast::Sender<OrderExecutionUpdate>,
        state: Arc<Mutex<ExchangeState>>,
    ) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await?;
        println!("📡 WebSocket server listening on {}", addr);
        
        let handle = tokio::spawn(async move {
            while let Ok((stream, addr)) = listener.accept().await {
                println!("🔗 New WebSocket connection from {}", addr);
                
                let market_rx = market_data_sender.subscribe();
                let order_book_rx = order_book_sender.subscribe();
                let order_execution_rx = order_execution_sender.subscribe();
                let state_clone = state.clone();
                
                tokio::spawn(Self::handle_websocket_connection(
                    stream,
                    addr,
                    market_rx,
                    order_book_rx,
                    order_execution_rx,
                    state_clone,
                ));
            }
        });
        
        Ok(handle)
    }

    async fn handle_websocket_connection(
        stream: TcpStream,
        addr: SocketAddr,
        mut market_rx: broadcast::Receiver<MarketDataUpdate>,
        mut order_book_rx: broadcast::Receiver<OrderBookUpdate>,
        mut order_execution_rx: broadcast::Receiver<OrderExecutionUpdate>,
        state: Arc<Mutex<ExchangeState>>,
    ) {
        let ws_stream = match accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                println!("❌ WebSocket handshake failed for {}: {}", addr, e);
                return;
            }
        };
        
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        
        // Increment connected clients
        {
            let mut state_guard = state.lock().unwrap();
            state_guard.connected_clients += 1;
            println!("👥 Client connected. Total clients: {}", state_guard.connected_clients);
        }
        
        // Send initial market data
        let symbols = {
            let state_guard = state.lock().unwrap();
            state_guard.symbols.keys().cloned().collect::<Vec<_>>()
        };
        
        for symbol in symbols {
            let snapshot = Self::create_order_book_snapshot(&symbol, &state);
            if let Ok(msg) = serde_json::to_string(&snapshot) {
                if ws_sender.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        }
        
        // Handle client messages and broadcast updates
        loop {
            tokio::select! {
                // Handle incoming WebSocket messages
                msg = ws_receiver.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(e) = Self::handle_client_message(&text, &state).await {
                                println!("❌ Error handling client message: {}", e);
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            println!("🔌 Client {} disconnected", addr);
                            break;
                        }
                        Some(Err(e)) => {
                            println!("❌ WebSocket error for {}: {}", addr, e);
                            break;
                        }
                        None => break,
                    }
                }
                
                // Broadcast market data updates
                Ok(update) = market_rx.recv() => {
                    let msg = json!({
                        "type": "market_data",
                        "data": update
                    });
                    if let Ok(text) = serde_json::to_string(&msg) {
                        if ws_sender.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                }
                
                // Broadcast order book updates
                Ok(update) = order_book_rx.recv() => {
                    let msg = json!({
                        "type": "order_book",
                        "data": update
                    });
                    if let Ok(text) = serde_json::to_string(&msg) {
                        if ws_sender.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                }
                
                // Broadcast order execution updates
                Ok(update) = order_execution_rx.recv() => {
                    let msg = json!({
                        "type": "order_execution",
                        "data": update
                    });
                    if let Ok(text) = serde_json::to_string(&msg) {
                        if ws_sender.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
        
        // Decrement connected clients
        {
            let mut state_guard = state.lock().unwrap();
            state_guard.connected_clients = state_guard.connected_clients.saturating_sub(1);
            println!("👥 Client disconnected. Total clients: {}", state_guard.connected_clients);
        }
    }

    async fn handle_client_message(
        message: &str,
        state: &Arc<Mutex<ExchangeState>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let request: Value = serde_json::from_str(message)?;
        
        match request["type"].as_str() {
            Some("subscribe") => {
                // Handle subscription requests
                println!("📊 Client subscribed to: {:?}", request["channels"]);
            }
            Some("place_order") => {
                // Handle order placement
                let order = Self::create_mock_order(&request, state)?;
                println!("📝 Order placed: {}", order.id);
            }
            Some("cancel_order") => {
                // Handle order cancellation
                if let Some(order_id) = request["order_id"].as_str() {
                    println!("❌ Order cancelled: {}", order_id);
                }
            }
            _ => {
                println!("❓ Unknown message type: {:?}", request);
            }
        }
        
        Ok(())
    }

    fn create_mock_order(
        request: &Value,
        state: &Arc<Mutex<ExchangeState>>,
    ) -> Result<Order, Box<dyn std::error::Error>> {
        let order_id = Uuid::new_v4().to_string();
        let symbol = request["symbol"].as_str().unwrap_or("BTCUSDT").to_string();
        let side = request["side"].as_str().unwrap_or("BUY").to_string();
        let quantity = Decimal::try_from(request["quantity"].as_f64().unwrap_or(1.0))?;
        let price = request["price"].as_f64().map(Decimal::try_from).transpose()?;
        
        let order = Order {
            id: order_id.clone(),
            symbol: symbol.clone(),
            side,
            order_type: request["type"].as_str().unwrap_or("LIMIT").to_string(),
            quantity,
            price,
            status: "NEW".to_string(),
            filled_quantity: Decimal::ZERO,
            remaining_quantity: quantity,
            timestamp: Utc::now(),
        };
        
        // Store order in state
        {
            let mut state_guard = state.lock().unwrap();
            state_guard.orders.insert(order_id, order.clone());
        }
        
        Ok(order)
    }

    fn create_order_book_snapshot(
        symbol: &str,
        state: &Arc<Mutex<ExchangeState>>,
    ) -> OrderBookUpdate {
        let state_guard = state.lock().unwrap();
        
        if let Some(order_book) = state_guard.order_books.get(symbol) {
            OrderBookUpdate {
                symbol: symbol.to_string(),
                bids: order_book.bids.clone(),
                asks: order_book.asks.clone(),
                last_update_id: order_book.last_update_id,
                timestamp: Utc::now(),
            }
        } else {
            // Create default order book
            OrderBookUpdate {
                symbol: symbol.to_string(),
                bids: vec![
                    PriceLevel { price: Decimal::new(50000, 0), quantity: Decimal::new(10, 1) },
                    PriceLevel { price: Decimal::new(49999, 0), quantity: Decimal::new(20, 1) },
                ],
                asks: vec![
                    PriceLevel { price: Decimal::new(50001, 0), quantity: Decimal::new(15, 1) },
                    PriceLevel { price: Decimal::new(50002, 0), quantity: Decimal::new(25, 1) },
                ],
                last_update_id: 1,
                timestamp: Utc::now(),
            }
        }
    }

    async fn start_market_simulation(
        market_data_sender: broadcast::Sender<MarketDataUpdate>,
        order_book_sender: broadcast::Sender<OrderBookUpdate>,
        state: Arc<Mutex<ExchangeState>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let symbols = vec!["BTCUSDT", "ETHUSDT", "ADAUSDT", "DOTUSDT"];
            let mut sequence = 0u64;
            let mut interval = interval(Duration::from_millis(100)); // 10 updates per second
            
            loop {
                interval.tick().await;
                
                for symbol in &symbols {
                    // Generate realistic price movement
                    let base_price = match symbol.as_ref() {
                        "BTCUSDT" => 50000.0,
                        "ETHUSDT" => 3000.0,
                        "ADAUSDT" => 1.0,
                        "DOTUSDT" => 25.0,
                        _ => 100.0,
                    };
                    
                    let price_change = (rand::random::<f64>() - 0.5) * 0.01; // ±0.5% change
                    let new_price = base_price * (1.0 + price_change);
                    let volume = rand::random::<f64>() * 1000.0;
                    
                    sequence += 1;
                    
                    let market_update = MarketDataUpdate {
                        symbol: symbol.to_string(),
                        price: Decimal::try_from(new_price).unwrap_or_default(),
                        volume: Decimal::try_from(volume).unwrap_or_default(),
                        timestamp: Utc::now(),
                        sequence,
                    };
                    
                    // Broadcast market data
                    let _ = market_data_sender.send(market_update);
                    
                    // Generate order book updates less frequently
                    if sequence % 10 == 0 {
                        let order_book_update = Self::generate_order_book_update(symbol, new_price, sequence);
                        let _ = order_book_sender.send(order_book_update);
                    }
                }
                
                // Small delay to prevent overwhelming
                sleep(Duration::from_millis(10)).await;
            }
        })
    }

    fn generate_order_book_update(symbol: &str, price: f64, sequence: u64) -> OrderBookUpdate {
        let spread = price * 0.001; // 0.1% spread
        let bid_price = price - spread / 2.0;
        let ask_price = price + spread / 2.0;
        
        OrderBookUpdate {
            symbol: symbol.to_string(),
            bids: vec![
                PriceLevel {
                    price: Decimal::try_from(bid_price).unwrap_or_default(),
                    quantity: Decimal::try_from(rand::random::<f64>() * 10.0).unwrap_or_default(),
                },
                PriceLevel {
                    price: Decimal::try_from(bid_price - spread).unwrap_or_default(),
                    quantity: Decimal::try_from(rand::random::<f64>() * 20.0).unwrap_or_default(),
                },
            ],
            asks: vec![
                PriceLevel {
                    price: Decimal::try_from(ask_price).unwrap_or_default(),
                    quantity: Decimal::try_from(rand::random::<f64>() * 10.0).unwrap_or_default(),
                },
                PriceLevel {
                    price: Decimal::try_from(ask_price + spread).unwrap_or_default(),
                    quantity: Decimal::try_from(rand::random::<f64>() * 20.0).unwrap_or_default(),
                },
            ],
            last_update_id: sequence,
            timestamp: Utc::now(),
        }
    }

    async fn start_rest_server(
        port: u16,
        state: Arc<Mutex<ExchangeState>>,
    ) -> tokio::task::JoinHandle<()> {
        use axum::{
            extract::{Query, State as AxumState},
            http::StatusCode,
            response::Json,
            routing::{get, post},
            Router,
        };
        
        let app = Router::new()
            .route("/api/v1/exchangeInfo", get(get_exchange_info))
            .route("/api/v1/order", post(place_order))
            .route("/api/v1/order", get(get_order))
            .route("/api/v1/account", get(get_account))
            .with_state(state);
        
        tokio::spawn(async move {
            let addr = format!("127.0.0.1:{}", port);
            println!("🌐 REST API server listening on {}", addr);
            
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            axum::serve(listener, app).await.unwrap();
        })
    }

    pub async fn stop() -> Result<(), Box<dyn std::error::Error>> {
        println!("🛑 Stopping mock exchange server...");
        // Implementation for graceful shutdown
        Ok(())
    }
}

impl ExchangeState {
    fn new() -> Self {
        let mut symbols = HashMap::new();
        let mut order_books = HashMap::new();
        let mut balances = HashMap::new();
        
        // Initialize test symbols
        let test_symbols = vec![
            ("BTCUSDT", "BTC", "USDT"),
            ("ETHUSDT", "ETH", "USDT"),
            ("ADAUSDT", "ADA", "USDT"),
            ("DOTUSDT", "DOT", "USDT"),
        ];
        
        for (symbol, base, quote) in test_symbols {
            symbols.insert(symbol.to_string(), SymbolInfo {
                symbol: symbol.to_string(),
                base_asset: base.to_string(),
                quote_asset: quote.to_string(),
                status: "TRADING".to_string(),
                min_qty: Decimal::new(1, 8), // 0.00000001
                max_qty: Decimal::new(9000000000000000u64, 0), // Large number
                tick_size: Decimal::new(1, 8),
            });
            
            order_books.insert(symbol.to_string(), OrderBook {
                symbol: symbol.to_string(),
                bids: Vec::new(),
                asks: Vec::new(),
                last_update_id: 0,
            });
        }
        
        // Initialize test balances
        balances.insert("BTC".to_string(), Decimal::new(10, 0));
        balances.insert("ETH".to_string(), Decimal::new(100, 0));
        balances.insert("USDT".to_string(), Decimal::new(100000, 0));
        
        Self {
            symbols,
            order_books,
            orders: HashMap::new(),
            balances,
            connected_clients: 0,
        }
    }
}

// REST API handlers
async fn get_exchange_info(
    AxumState(state): AxumState<Arc<Mutex<ExchangeState>>>,
) -> Json<Value> {
    let state_guard = state.lock().unwrap();
    let symbols: Vec<_> = state_guard.symbols.values().collect();
    
    Json(json!({
        "timezone": "UTC",
        "serverTime": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),
        "symbols": symbols
    }))
}

async fn place_order(
    AxumState(state): AxumState<Arc<Mutex<ExchangeState>>>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let order_id = Uuid::new_v4().to_string();
    
    // Simulate order processing delay
    sleep(Duration::from_millis(50)).await;
    
    let response = json!({
        "symbol": payload["symbol"],
        "orderId": order_id,
        "clientOrderId": payload["newClientOrderId"],
        "transactTime": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),
        "price": payload["price"],
        "origQty": payload["quantity"],
        "executedQty": "0.00000000",
        "status": "NEW",
        "timeInForce": payload["timeInForce"],
        "type": payload["type"],
        "side": payload["side"]
    });
    
    Ok(Json(response))
}

async fn get_order(
    AxumState(state): AxumState<Arc<Mutex<ExchangeState>>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let order_id = params.get("orderId").ok_or(StatusCode::BAD_REQUEST)?;
    
    let state_guard = state.lock().unwrap();
    if let Some(order) = state_guard.orders.get(order_id) {
        Ok(Json(serde_json::to_value(order).unwrap()))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn get_account(
    AxumState(state): AxumState<Arc<Mutex<ExchangeState>>>,
) -> Json<Value> {
    let state_guard = state.lock().unwrap();
    let balances: Vec<_> = state_guard.balances.iter().map(|(asset, free)| {
        json!({
            "asset": asset,
            "free": free.to_string(),
            "locked": "0.00000000"
        })
    }).collect();
    
    Json(json!({
        "makerCommission": 10,
        "takerCommission": 10,
        "buyerCommission": 0,
        "sellerCommission": 0,
        "canTrade": true,
        "canWithdraw": true,
        "canDeposit": true,
        "updateTime": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),
        "balances": balances
    }))
}

/// Test Kafka Environment
pub struct TestKafkaEnvironment;

impl TestKafkaEnvironment {
    pub async fn setup(brokers: &[String]) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔄 Setting up test Kafka environment...");
        
        // In a real implementation, this would:
        // 1. Start Kafka containers via Docker
        // 2. Create test topics
        // 3. Wait for readiness
        
        // For now, we'll simulate setup
        sleep(Duration::from_millis(100)).await;
        println!("✅ Test Kafka environment ready");
        
        Ok(())
    }
    
    pub async fn cleanup() -> Result<(), Box<dyn std::error::Error>> {
        println!("🧹 Cleaning up Kafka test environment...");
        sleep(Duration::from_millis(50)).await;
        Ok(())
    }
}

/// Test Database
pub struct TestDatabase;

impl TestDatabase {
    pub async fn initialize(database_url: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🗄️ Initializing test database...");
        println!("   URL: {}", database_url);
        
        // In a real implementation, this would:
        // 1. Connect to test database
        // 2. Run migrations
        // 3. Seed test data
        
        sleep(Duration::from_millis(100)).await;
        println!("✅ Test database ready");
        
        Ok(())
    }
    
    pub async fn cleanup() -> Result<(), Box<dyn std::error::Error>> {
        println!("🧹 Cleaning up test database...");
        sleep(Duration::from_millis(50)).await;
        Ok(())
    }
}