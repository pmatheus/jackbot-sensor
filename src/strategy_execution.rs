//! Strategy Execution Engine for Manual Trading
//!
//! High-performance execution engine for manual trading strategies
//! with support for advanced order types and smart execution algorithms.

use anyhow::{Result, Context, bail};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, info, warn, error};
use uuid::Uuid;

use crate::connector::{Exchange, Order, OrderId, OrderResult, OrderSide, OrderType, OrderStatus};
use crate::connectors::{SupportedExchange, create_connector};
use crate::order_book_aggregator_ultra::{OrderBookAggregatorUltra, FastAggregatedBook};
use crate::market_arbitrage::ArbitrageDetector;

/// Trading strategy types (manual strategies only - NO AI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyType {
    /// Market making with bid/ask spreads
    MarketMaking {
        spread_bps: u32,
        order_size: f64,
        max_position: f64,
    },
    
    /// Arbitrage across exchanges
    Arbitrage {
        min_profit_bps: u32,
        max_exposure: f64,
    },
    
    /// Dollar Cost Averaging
    DCA {
        interval: Duration,
        amount_per_interval: f64,
        total_budget: f64,
    },
    
    /// Grid trading
    GridTrading {
        grid_levels: u32,
        grid_spacing_bps: u32,
        order_size_per_level: f64,
    },
    
    /// Simple limit orders
    LimitOrder {
        side: OrderSide,
        price: f64,
        quantity: f64,
        time_in_force: TimeInForce,
    },
    
    /// TWAP (Time-Weighted Average Price)
    TWAP {
        total_quantity: f64,
        duration: Duration,
        slice_count: u32,
    },
    
    /// Iceberg orders
    Iceberg {
        total_quantity: f64,
        visible_quantity: f64,
        side: OrderSide,
        limit_price: Option<f64>,
    },
}

/// Time in force options
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TimeInForce {
    GTC,  // Good Till Cancelled
    IOC,  // Immediate Or Cancel
    FOK,  // Fill Or Kill
    GTD(u64), // Good Till Date (timestamp)
}

/// Strategy execution state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyState {
    pub id: String,
    pub strategy_type: StrategyType,
    pub symbol: String,
    pub exchange: SupportedExchange,
    pub status: StrategyStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub orders: Vec<String>,
    pub filled_quantity: f64,
    pub average_price: f64,
    pub pnl: f64,
}

/// Strategy status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum StrategyStatus {
    Active,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

/// Execution command
#[derive(Debug)]
pub enum ExecutionCommand {
    StartStrategy {
        strategy: StrategyType,
        symbol: String,
        exchange: SupportedExchange,
        response: oneshot::Sender<Result<String>>,
    },
    StopStrategy {
        strategy_id: String,
        response: oneshot::Sender<Result<()>>,
    },
    PauseStrategy {
        strategy_id: String,
        response: oneshot::Sender<Result<()>>,
    },
    ResumeStrategy {
        strategy_id: String,
        response: oneshot::Sender<Result<()>>,
    },
    GetStrategyState {
        strategy_id: String,
        response: oneshot::Sender<Result<StrategyState>>,
    },
}

/// Strategy execution engine
pub struct StrategyExecutionEngine {
    /// Active strategies
    strategies: Arc<RwLock<HashMap<String, StrategyState>>>,
    
    /// Exchange connectors
    exchanges: Arc<RwLock<HashMap<SupportedExchange, Box<dyn Exchange>>>>,
    
    /// Order book aggregator
    order_book_aggregator: Arc<OrderBookAggregatorUltra>,
    
    /// Arbitrage detector
    arbitrage_detector: Arc<ArbitrageDetector>,
    
    /// Command channel
    command_rx: mpsc::UnboundedReceiver<ExecutionCommand>,
    command_tx: mpsc::UnboundedSender<ExecutionCommand>,
    
    /// Strategy update notifications
    update_tx: broadcast::Sender<StrategyState>,
}

impl StrategyExecutionEngine {
    /// Create a new strategy execution engine
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (update_tx, _) = broadcast::channel(1000);
        
        Self {
            strategies: Arc::new(RwLock::new(HashMap::new())),
            exchanges: Arc::new(RwLock::new(HashMap::new())),
            order_book_aggregator: Arc::new(OrderBookAggregatorUltra::new()),
            arbitrage_detector: Arc::new(ArbitrageDetector::new(0.1)), // 0.1% min profit
            command_rx,
            command_tx,
            update_tx,
        }
    }
    
    /// Get command sender for external control
    pub fn get_command_sender(&self) -> mpsc::UnboundedSender<ExecutionCommand> {
        self.command_tx.clone()
    }
    
    /// Subscribe to strategy updates
    pub fn subscribe_updates(&self) -> broadcast::Receiver<StrategyState> {
        self.update_tx.subscribe()
    }
    
    /// Initialize exchange connections
    pub async fn initialize_exchanges(
        &self,
        exchanges: Vec<(SupportedExchange, Option<String>, Option<String>)>,
    ) -> Result<()> {
        let mut exchange_map = self.exchanges.write();
        
        for (exchange, api_key, api_secret) in exchanges {
            info!("Initializing {} connector", exchange.as_str());
            
            let connector = create_connector(exchange, api_key, api_secret, false)?;
            exchange_map.insert(exchange, connector);
        }
        
        info!("Initialized {} exchange connectors", exchange_map.len());
        Ok(())
    }
    
    /// Start the execution engine
    pub async fn start(mut self) {
        info!("Starting strategy execution engine");
        
        // Start strategy executors
        self.start_strategy_executors().await;
        
        // Main command processing loop
        while let Some(command) = self.command_rx.recv().await {
            match command {
                ExecutionCommand::StartStrategy { strategy, symbol, exchange, response } => {
                    let result = self.start_strategy(strategy, symbol, exchange).await;
                    let _ = response.send(result);
                }
                
                ExecutionCommand::StopStrategy { strategy_id, response } => {
                    let result = self.stop_strategy(&strategy_id).await;
                    let _ = response.send(result);
                }
                
                ExecutionCommand::PauseStrategy { strategy_id, response } => {
                    let result = self.pause_strategy(&strategy_id).await;
                    let _ = response.send(result);
                }
                
                ExecutionCommand::ResumeStrategy { strategy_id, response } => {
                    let result = self.resume_strategy(&strategy_id).await;
                    let _ = response.send(result);
                }
                
                ExecutionCommand::GetStrategyState { strategy_id, response } => {
                    let strategies = self.strategies.read();
                    let result = strategies.get(&strategy_id)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Strategy not found"));
                    let _ = response.send(result);
                }
            }
        }
    }
    
    /// Start a new strategy
    async fn start_strategy(
        &self,
        strategy: StrategyType,
        symbol: String,
        exchange: SupportedExchange,
    ) -> Result<String> {
        let strategy_id = Uuid::new_v4().to_string();
        
        let state = StrategyState {
            id: strategy_id.clone(),
            strategy_type: strategy.clone(),
            symbol: symbol.clone(),
            exchange,
            status: StrategyStatus::Active,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            orders: Vec::new(),
            filled_quantity: 0.0,
            average_price: 0.0,
            pnl: 0.0,
        };
        
        // Store strategy
        {
            let mut strategies = self.strategies.write();
            strategies.insert(strategy_id.clone(), state.clone());
        }
        
        // Notify subscribers
        let _ = self.update_tx.send(state);
        
        info!("Started strategy {} ({:?}) for {} on {}", 
              strategy_id, strategy, symbol, exchange.as_str());
        
        Ok(strategy_id)
    }
    
    /// Stop a strategy
    async fn stop_strategy(&self, strategy_id: &str) -> Result<()> {
        let mut strategies = self.strategies.write();
        
        if let Some(mut strategy) = strategies.get_mut(strategy_id) {
            strategy.status = StrategyStatus::Cancelled;
            strategy.updated_at = chrono::Utc::now().timestamp_millis() as u64;
            
            // Cancel all open orders
            // TODO: Implement order cancellation
            
            info!("Stopped strategy {}", strategy_id);
            Ok(())
        } else {
            bail!("Strategy not found")
        }
    }
    
    /// Pause a strategy
    async fn pause_strategy(&self, strategy_id: &str) -> Result<()> {
        let mut strategies = self.strategies.write();
        
        if let Some(mut strategy) = strategies.get_mut(strategy_id) {
            if strategy.status == StrategyStatus::Active {
                strategy.status = StrategyStatus::Paused;
                strategy.updated_at = chrono::Utc::now().timestamp_millis() as u64;
                info!("Paused strategy {}", strategy_id);
                Ok(())
            } else {
                bail!("Strategy is not active")
            }
        } else {
            bail!("Strategy not found")
        }
    }
    
    /// Resume a strategy
    async fn resume_strategy(&self, strategy_id: &str) -> Result<()> {
        let mut strategies = self.strategies.write();
        
        if let Some(mut strategy) = strategies.get_mut(strategy_id) {
            if strategy.status == StrategyStatus::Paused {
                strategy.status = StrategyStatus::Active;
                strategy.updated_at = chrono::Utc::now().timestamp_millis() as u64;
                info!("Resumed strategy {}", strategy_id);
                Ok(())
            } else {
                bail!("Strategy is not paused")
            }
        } else {
            bail!("Strategy not found")
        }
    }
    
    /// Start strategy executor tasks
    async fn start_strategy_executors(&self) {
        // Market making executor
        self.start_market_making_executor();
        
        // Arbitrage executor
        self.start_arbitrage_executor();
        
        // Grid trading executor
        self.start_grid_trading_executor();
        
        // TWAP executor
        self.start_twap_executor();
        
        // Iceberg executor
        self.start_iceberg_executor();
    }
    
    /// Start market making executor
    fn start_market_making_executor(&self) {
        let strategies = Arc::clone(&self.strategies);
        let exchanges = Arc::clone(&self.exchanges);
        let order_book_aggregator = Arc::clone(&self.order_book_aggregator);
        let update_tx = self.update_tx.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            
            loop {
                interval.tick().await;
                
                let active_strategies: Vec<_> = {
                    let strats = strategies.read();
                    strats.values()
                        .filter(|s| {
                            s.status == StrategyStatus::Active &&
                            matches!(s.strategy_type, StrategyType::MarketMaking { .. })
                        })
                        .cloned()
                        .collect()
                };
                
                for strategy in active_strategies {
                    if let StrategyType::MarketMaking { spread_bps, order_size, max_position } = 
                        &strategy.strategy_type {
                        
                        // Get order book
                        if let Some(book) = order_book_aggregator.get_aggregated_book(&strategy.symbol) {
                            // Calculate bid/ask prices based on spread
                            if let (Some(best_bid), Some(best_ask)) = (book.best_bid, book.best_ask) {
                                let mid_price = (best_bid.price + best_ask.price) / 2.0;
                                let spread = mid_price * (*spread_bps as f64) / 10000.0;
                                
                                let bid_price = mid_price - spread / 2.0;
                                let ask_price = mid_price + spread / 2.0;
                                
                                // Place orders (simplified - would need proper order management)
                                debug!("Market making {} - Bid: {:.2} Ask: {:.2}", 
                                      strategy.symbol, bid_price, ask_price);
                            }
                        }
                    }
                }
            }
        });
    }
    
    /// Start arbitrage executor
    fn start_arbitrage_executor(&self) {
        let strategies = Arc::clone(&self.strategies);
        let exchanges = Arc::clone(&self.exchanges);
        let arbitrage_detector = Arc::clone(&self.arbitrage_detector);
        let update_tx = self.update_tx.clone();
        
        tokio::spawn(async move {
            let mut alerts = arbitrage_detector.subscribe_alerts();
            
            while let Ok(opportunity) = alerts.recv() {
                // Check if we have an active arbitrage strategy for this symbol
                let active_strategies: Vec<_> = {
                    let strats = strategies.read();
                    strats.values()
                        .filter(|s| {
                            s.status == StrategyStatus::Active &&
                            s.symbol == opportunity.symbol &&
                            matches!(s.strategy_type, StrategyType::Arbitrage { .. })
                        })
                        .cloned()
                        .collect()
                };
                
                for strategy in active_strategies {
                    if let StrategyType::Arbitrage { min_profit_bps, max_exposure } = 
                        &strategy.strategy_type {
                        
                        if opportunity.profit_percentage * 100.0 >= *min_profit_bps as f64 {
                            info!("Arbitrage opportunity detected: {} - Buy {} @ {:.2}, Sell {} @ {:.2}, Profit: {:.2}%",
                                  opportunity.symbol,
                                  opportunity.buy_exchange,
                                  opportunity.buy_price,
                                  opportunity.sell_exchange,
                                  opportunity.sell_price,
                                  opportunity.profit_percentage
                            );
                            
                            // Execute arbitrage (simplified - would need proper execution)
                            // 1. Buy on cheaper exchange
                            // 2. Transfer to expensive exchange
                            // 3. Sell on expensive exchange
                        }
                    }
                }
            }
        });
    }
    
    /// Start grid trading executor
    fn start_grid_trading_executor(&self) {
        // Similar pattern - monitor price and place grid orders
    }
    
    /// Start TWAP executor
    fn start_twap_executor(&self) {
        // Execute orders in time slices
    }
    
    /// Start iceberg executor
    fn start_iceberg_executor(&self) {
        // Execute large orders in small visible chunks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_strategy_creation() {
        let engine = StrategyExecutionEngine::new();
        let command_tx = engine.get_command_sender();
        
        // Start engine in background
        tokio::spawn(engine.start());
        
        // Create a limit order strategy
        let (tx, rx) = oneshot::channel();
        command_tx.send(ExecutionCommand::StartStrategy {
            strategy: StrategyType::LimitOrder {
                side: OrderSide::Buy,
                price: 42000.0,
                quantity: 0.1,
                time_in_force: TimeInForce::GTC,
            },
            symbol: "BTC/USDT".to_string(),
            exchange: SupportedExchange::Binance,
            response: tx,
        }).unwrap();
        
        let strategy_id = rx.await.unwrap().unwrap();
        assert!(!strategy_id.is_empty());
    }
    
    #[test]
    fn test_arbitrage_strategy() {
        let strategy = StrategyType::Arbitrage {
            min_profit_bps: 10, // 0.1%
            max_exposure: 10000.0, // $10k
        };
        
        match strategy {
            StrategyType::Arbitrage { min_profit_bps, max_exposure } => {
                assert_eq!(min_profit_bps, 10);
                assert_eq!(max_exposure, 10000.0);
            }
            _ => panic!("Wrong strategy type"),
        }
    }
}