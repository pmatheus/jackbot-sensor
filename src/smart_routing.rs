//! Smart order routing module for optimal execution across exchanges
//!
//! This module implements intelligent order routing logic to achieve best execution
//! by analyzing market conditions, liquidity, and exchange-specific factors.

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

use crate::api::{OrderBookData, TickerData};
use crate::connector::{
    Balance, Exchange, MarketData, Order, OrderId, OrderResult, OrderSide, OrderStatus, OrderType,
};

/// Configuration for smart order routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartRoutingConfig {
    /// Maximum number of exchanges to consider for routing
    pub max_exchanges: usize,
    /// Minimum liquidity threshold (in quote currency)
    pub min_liquidity: f64,
    /// Maximum acceptable spread percentage
    pub max_spread_pct: f64,
    /// Order splitting threshold (orders larger than this will be split)
    pub split_threshold: f64,
    /// Maximum order size per exchange (in quote currency)
    pub max_order_size: f64,
    /// Circuit breaker threshold (orders/minute)
    pub rate_limit_threshold: u32,
}

impl Default for SmartRoutingConfig {
    fn default() -> Self {
        Self {
            max_exchanges: 3,
            min_liquidity: 10000.0,
            max_spread_pct: 0.5,
            split_threshold: 50000.0,
            max_order_size: 25000.0,
            rate_limit_threshold: 10,
        }
    }
}

/// Exchange-specific market data and metrics
#[derive(Debug, Clone)]
pub struct ExchangeMetrics {
    pub exchange_name: String,
    pub ticker: Option<TickerData>,
    pub order_book: Option<OrderBookData>,
    pub latency_ms: f64,
    pub fee_rate: f64,
    pub available_liquidity: f64,
    pub last_update: Instant,
    pub reliability_score: f64,
}

/// Order routing decision
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub exchange: String,
    pub quantity: f64,
    pub expected_price: f64,
    pub confidence: f64,
    pub rationale: String,
}

/// Smart order router implementing best execution logic
pub struct SmartOrderRouter {
    config: SmartRoutingConfig,
    exchanges: HashMap<String, Arc<dyn Exchange>>,
    metrics: Arc<RwLock<HashMap<String, ExchangeMetrics>>>,
    order_history: Arc<Mutex<VecDeque<(Instant, String)>>>,
}

impl SmartOrderRouter {
    /// Create a new smart order router
    pub fn new(config: SmartRoutingConfig) -> Self {
        Self {
            config,
            exchanges: HashMap::new(),
            metrics: Arc::new(RwLock::new(HashMap::new())),
            order_history: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Add an exchange to the routing pool
    pub fn add_exchange(&mut self, name: String, exchange: Arc<dyn Exchange>) {
        self.exchanges.insert(name.clone(), exchange);
        
        // Initialize metrics
        let metrics = ExchangeMetrics {
            exchange_name: name.clone(),
            ticker: None,
            order_book: None,
            latency_ms: 0.0,
            fee_rate: 0.001, // Default 0.1% fee
            available_liquidity: 0.0,
            last_update: Instant::now(),
            reliability_score: 1.0,
        };
        
        tokio::spawn({
            let metrics_store = Arc::clone(&self.metrics);
            async move {
                let mut guard = metrics_store.write().await;
                guard.insert(name, metrics);
            }
        });
    }

    /// Update market data for an exchange
    pub async fn update_market_data(&self, exchange_name: &str, data: MarketData) {
        let mut metrics = self.metrics.write().await;
        
        if let Some(exchange_metrics) = metrics.get_mut(exchange_name) {
            match data {
                MarketData::Ticker(ticker) => {
                    exchange_metrics.ticker = Some(ticker);
                    exchange_metrics.last_update = Instant::now();
                }
                MarketData::OrderBook(book) => {
                    // Calculate available liquidity
                    let liquidity = book.bids.iter().take(10).map(|b| b[0] * b[1]).sum::<f64>()
                        + book.asks.iter().take(10).map(|a| a[0] * a[1]).sum::<f64>();
                    
                    exchange_metrics.order_book = Some(book);
                    exchange_metrics.available_liquidity = liquidity;
                    exchange_metrics.last_update = Instant::now();
                }
                _ => {}
            }
        }
    }

    /// Route an order to the best exchange(s)
    pub async fn route_order(&self, order: Order) -> Result<Vec<RoutingDecision>> {
        info!("Routing order: {:?}", order);
        
        // Check rate limits
        self.check_rate_limits().await?;
        
        // Get current market conditions
        let market_snapshot = self.get_market_snapshot(&order.symbol).await?;
        
        // Analyze and rank exchanges
        let ranked_exchanges = self.rank_exchanges(&order, &market_snapshot).await?;
        
        if ranked_exchanges.is_empty() {
            return Err(anyhow::anyhow!("No suitable exchanges found for order"));
        }
        
        // Determine optimal routing strategy
        let routing_decisions = if order.quantity * market_snapshot.best_bid > self.config.split_threshold {
            self.split_order(&order, &ranked_exchanges).await?
        } else {
            // Route to best exchange
            vec![RoutingDecision {
                exchange: ranked_exchanges[0].exchange_name.clone(),
                quantity: order.quantity,
                expected_price: market_snapshot.best_bid,
                confidence: ranked_exchanges[0].reliability_score,
                rationale: "Best single exchange execution".to_string(),
            }]
        };
        
        info!("Routing decisions: {:?}", routing_decisions);
        Ok(routing_decisions)
    }

    /// Execute a routed order across multiple exchanges
    pub async fn execute_routed_order(&self, order: Order) -> Result<Vec<OrderResult>> {
        let routing_decisions = self.route_order(order.clone()).await?;
        let mut results = Vec::new();
        
        for decision in routing_decisions {
            if let Some(exchange) = self.exchanges.get(&decision.exchange) {
                let sub_order = Order {
                    id: Some(format!("{}-{}", order.id.as_ref().unwrap_or(&"auto".to_string()), decision.exchange)),
                    symbol: order.symbol.clone(),
                    side: order.side.clone(),
                    order_type: order.order_type.clone(),
                    price: Some(decision.expected_price),
                    quantity: decision.quantity,
                    time_in_force: order.time_in_force.clone(),
                    status: OrderStatus::New,
                };
                
                let start_time = Instant::now();
                
                match exchange.place_order(sub_order).await {
                    Ok(result) => {
                        // Update latency metrics
                        let latency = start_time.elapsed().as_millis() as f64;
                        self.update_latency_metric(&decision.exchange, latency).await;
                        
                        results.push(result);
                        
                        // Record successful order
                        self.record_order(&decision.exchange).await;
                    }
                    Err(e) => {
                        warn!("Failed to execute order on {}: {}", decision.exchange, e);
                        // Update reliability score
                        self.update_reliability_score(&decision.exchange, false).await;
                        return Err(e);
                    }
                }
            }
        }
        
        Ok(results)
    }

    /// Get aggregated account balances across all exchanges
    pub async fn get_aggregated_balances(&self) -> Result<HashMap<String, Balance>> {
        let mut aggregated = HashMap::new();
        
        for (exchange_name, exchange) in &self.exchanges {
            match exchange.get_balance().await {
                Ok(balances) => {
                    for balance in balances {
                        aggregated
                            .entry(balance.asset.clone())
                            .and_modify(|b: &mut Balance| {
                                b.free += balance.free;
                                b.locked += balance.locked;
                                b.total += balance.total;
                            })
                            .or_insert(balance);
                    }
                }
                Err(e) => {
                    warn!("Failed to get balance from {}: {}", exchange_name, e);
                }
            }
        }
        
        Ok(aggregated)
    }

    /// Get market data snapshot for routing decisions
    async fn get_market_snapshot(&self, symbol: &str) -> Result<MarketSnapshot> {
        let metrics = self.metrics.read().await;
        
        let mut best_bid = 0.0;
        let mut best_ask = f64::MAX;
        let mut total_liquidity = 0.0;
        let mut active_exchanges = 0;
        
        for (_, exchange_metrics) in metrics.iter() {
            if let Some(ticker) = &exchange_metrics.ticker {
                if ticker.symbol == symbol {
                    if ticker.bid > best_bid {
                        best_bid = ticker.bid;
                    }
                    if ticker.ask < best_ask {
                        best_ask = ticker.ask;
                    }
                    total_liquidity += exchange_metrics.available_liquidity;
                    active_exchanges += 1;
                }
            }
        }
        
        if active_exchanges == 0 {
            return Err(anyhow::anyhow!("No market data available for symbol: {}", symbol));
        }
        
        Ok(MarketSnapshot {
            symbol: symbol.to_string(),
            best_bid,
            best_ask,
            spread_pct: ((best_ask - best_bid) / best_bid) * 100.0,
            total_liquidity,
            active_exchanges,
        })
    }

    /// Rank exchanges based on multiple factors
    async fn rank_exchanges(
        &self,
        order: &Order,
        snapshot: &MarketSnapshot,
    ) -> Result<Vec<ExchangeMetrics>> {
        let metrics = self.metrics.read().await;
        let mut candidates = Vec::new();
        
        for (_, exchange_metrics) in metrics.iter() {
            if let Some(ticker) = &exchange_metrics.ticker {
                if ticker.symbol == order.symbol {
                    // Calculate execution score
                    let mut score = exchange_metrics.reliability_score;
                    
                    // Liquidity factor (higher is better)
                    let liquidity_factor = (exchange_metrics.available_liquidity / 100000.0).min(1.0);
                    score *= liquidity_factor;
                    
                    // Latency factor (lower is better)
                    let latency_factor = (100.0 / (exchange_metrics.latency_ms + 1.0)).min(1.0);
                    score *= latency_factor;
                    
                    // Price factor (depends on order side)
                    let price_factor = match order.side {
                        OrderSide::Buy => {
                            // For buy orders, prefer lower ask prices
                            if ticker.ask > 0.0 {
                                snapshot.best_ask / ticker.ask
                            } else {
                                0.0
                            }
                        }
                        OrderSide::Sell => {
                            // For sell orders, prefer higher bid prices
                            if snapshot.best_bid > 0.0 {
                                ticker.bid / snapshot.best_bid
                            } else {
                                0.0
                            }
                        }
                    };
                    score *= price_factor;
                    
                    // Fee factor
                    let fee_factor = 1.0 - exchange_metrics.fee_rate;
                    score *= fee_factor;
                    
                    if score > 0.1 {
                        // Only consider exchanges with reasonable scores
                        let mut scored_metrics = exchange_metrics.clone();
                        scored_metrics.reliability_score = score;
                        candidates.push(scored_metrics);
                    }
                }
            }
        }
        
        // Sort by score (highest first)
        candidates.sort_by(|a, b| b.reliability_score.partial_cmp(&a.reliability_score).unwrap());
        
        // Limit to configured maximum
        candidates.truncate(self.config.max_exchanges);
        
        Ok(candidates)
    }

    /// Split large orders across multiple exchanges
    async fn split_order(
        &self,
        order: &Order,
        exchanges: &[ExchangeMetrics],
    ) -> Result<Vec<RoutingDecision>> {
        let mut decisions = Vec::new();
        let mut remaining_quantity = order.quantity;
        
        for exchange_metrics in exchanges {
            if remaining_quantity <= 0.0 {
                break;
            }
            
            // Calculate optimal quantity for this exchange
            let max_qty_by_liquidity = exchange_metrics.available_liquidity * 0.1; // Use 10% of available liquidity
            let max_qty_by_config = self.config.max_order_size;
            let max_qty = max_qty_by_liquidity.min(max_qty_by_config);
            
            let quantity = remaining_quantity.min(max_qty);
            
            if quantity > 0.0 {
                let expected_price = match &exchange_metrics.ticker {
                    Some(ticker) => match order.side {
                        OrderSide::Buy => ticker.ask,
                        OrderSide::Sell => ticker.bid,
                    },
                    None => order.price.unwrap_or(0.0),
                };
                
                decisions.push(RoutingDecision {
                    exchange: exchange_metrics.exchange_name.clone(),
                    quantity,
                    expected_price,
                    confidence: exchange_metrics.reliability_score,
                    rationale: format!("Split order: {} of {}", quantity, order.quantity),
                });
                
                remaining_quantity -= quantity;
            }
        }
        
        Ok(decisions)
    }

    /// Check rate limits before executing orders
    async fn check_rate_limits(&self) -> Result<()> {
        let mut history = self.order_history.lock().await;
        let now = Instant::now();
        
        // Remove orders older than 1 minute
        while let Some((timestamp, _)) = history.front() {
            if now.duration_since(*timestamp).as_secs() > 60 {
                history.pop_front();
            } else {
                break;
            }
        }
        
        // Check if we're under the rate limit
        if history.len() >= self.config.rate_limit_threshold as usize {
            return Err(anyhow::anyhow!(
                "Rate limit exceeded: {} orders in the last minute",
                history.len()
            ));
        }
        
        Ok(())
    }

    /// Record an order execution for rate limiting
    async fn record_order(&self, exchange: &str) {
        let mut history = self.order_history.lock().await;
        history.push_back((Instant::now(), exchange.to_string()));
    }

    /// Update latency metric for an exchange
    async fn update_latency_metric(&self, exchange: &str, latency_ms: f64) {
        let mut metrics = self.metrics.write().await;
        if let Some(exchange_metrics) = metrics.get_mut(exchange) {
            // Use exponential moving average
            exchange_metrics.latency_ms = exchange_metrics.latency_ms * 0.9 + latency_ms * 0.1;
        }
    }

    /// Update reliability score based on execution success/failure
    async fn update_reliability_score(&self, exchange: &str, success: bool) {
        let mut metrics = self.metrics.write().await;
        if let Some(exchange_metrics) = metrics.get_mut(exchange) {
            let adjustment = if success { 0.01 } else { -0.05 };
            exchange_metrics.reliability_score = (exchange_metrics.reliability_score + adjustment)
                .max(0.1)
                .min(1.0);
        }
    }
}

/// Market data snapshot for routing decisions
#[derive(Debug, Clone)]
struct MarketSnapshot {
    symbol: String,
    best_bid: f64,
    best_ask: f64,
    spread_pct: f64,
    total_liquidity: f64,
    active_exchanges: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::{MockExchangeClient, OrderResult, OrderStatus};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    struct MockExchange {
        name: String,
        latency_ms: u64,
        should_fail: bool,
    }

    #[async_trait]
    impl Exchange for MockExchange {
        async fn connect(&self) -> Result<crate::connector::Connection> {
            Ok(Arc::new(()) as crate::connector::Connection)
        }

        async fn subscribe_market_data(
            &self,
            _symbols: Vec<String>,
        ) -> Result<crate::connector::MarketDataStream> {
            unimplemented!()
        }

        async fn place_order(&self, order: Order) -> Result<OrderResult> {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.latency_ms)).await;
            
            if self.should_fail {
                return Err(anyhow::anyhow!("Mock exchange failure"));
            }

            Ok(OrderResult {
                order_id: format!("{}-{}", self.name, uuid::Uuid::new_v4()),
                status: OrderStatus::New,
                filled_quantity: 0.0,
                remaining_quantity: order.quantity,
                average_price: order.price.unwrap_or(50000.0),
                commission: 0.001,
                commission_asset: "USDT".to_string(),
                timestamp: chrono::Utc::now().timestamp_millis(),
            })
        }

        async fn cancel_order(&self, _id: OrderId) -> Result<()> {
            Ok(())
        }

        async fn get_balance(&self) -> Result<Vec<Balance>> {
            Ok(vec![Balance {
                asset: "USDT".to_string(),
                free: 10000.0,
                locked: 0.0,
                total: 10000.0,
            }])
        }
    }

    #[tokio::test]
    async fn test_smart_routing_basic() {
        let config = SmartRoutingConfig::default();
        let mut router = SmartOrderRouter::new(config);

        // Add mock exchanges
        router.add_exchange(
            "exchange1".to_string(),
            Arc::new(MockExchange {
                name: "exchange1".to_string(),
                latency_ms: 50,
                should_fail: false,
            }),
        );

        router.add_exchange(
            "exchange2".to_string(),
            Arc::new(MockExchange {
                name: "exchange2".to_string(),
                latency_ms: 100,
                should_fail: false,
            }),
        );

        // Update market data
        router
            .update_market_data(
                "exchange1",
                MarketData::Ticker(TickerData {
                    symbol: "BTC/USDT".to_string(),
                    exchange: "exchange1".to_string(),
                    price: 50000.0,
                    bid: 49900.0,
                    ask: 50100.0,
                    volume_24h: 1000.0,
                    change_24h: 1.5,
                    high_24h: 51000.0,
                    low_24h: 49000.0,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                }),
            )
            .await;

        let order = Order {
            id: Some("test-order".to_string()),
            symbol: "BTC/USDT".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(50000.0),
            quantity: 1.0,
            time_in_force: Some(crate::connector::TimeInForce::GTC),
            status: OrderStatus::New,
        };

        let results = router.execute_routed_order(order).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_order_splitting() {
        let mut config = SmartRoutingConfig::default();
        config.split_threshold = 1000.0; // Low threshold to trigger splitting
        config.max_order_size = 0.5; // Force splitting

        let mut router = SmartOrderRouter::new(config);

        router.add_exchange(
            "exchange1".to_string(),
            Arc::new(MockExchange {
                name: "exchange1".to_string(),
                latency_ms: 50,
                should_fail: false,
            }),
        );

        router.add_exchange(
            "exchange2".to_string(),
            Arc::new(MockExchange {
                name: "exchange2".to_string(),
                latency_ms: 75,
                should_fail: false,
            }),
        );

        // Large order that should be split
        let order = Order {
            id: Some("test-large-order".to_string()),
            symbol: "BTC/USDT".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(50000.0),
            quantity: 2.0, // Large quantity
            time_in_force: Some(crate::connector::TimeInForce::GTC),
            status: OrderStatus::New,
        };

        let routing_decisions = router.route_order(order).await.unwrap();
        assert!(routing_decisions.len() > 1, "Order should be split across exchanges");
    }
}