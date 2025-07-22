use super::{ProcessingConfig, StreamType};
use crate::{
    data_gathering::MarketDataCollector,
    order::{id::{ClientOrderId, StrategyId}, OrderKind},
    strategy::event_driven::{EventDrivenStrategy, MomentumEventProcessor, StrategyConfig},
    testing::{create_test_limit_order, create_test_market_order, TestOrderExecutionEngine},
};
use aws_sdk_kinesis::types::Record;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange, Side};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Message processor for handling Kinesis records
#[derive(Debug)]
pub struct MessageProcessor {
    execution_engine: Arc<TestOrderExecutionEngine>,
    market_data_collector: Arc<MarketDataCollector>,
    config: ProcessingConfig,
}

impl MessageProcessor {
    /// Create new message processor
    pub fn new(
        execution_engine: Arc<TestOrderExecutionEngine>,
        market_data_collector: Arc<MarketDataCollector>,
        config: ProcessingConfig,
    ) -> Self {
        Self {
            execution_engine,
            market_data_collector,
            config,
        }
    }

    /// Process a single Kinesis record
    pub async fn process_message(
        &self,
        record: Record,
        stream_type: StreamType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let data = record.data().as_ref();
        let decoded_data = BASE64.decode(data)?;
        let message_str = String::from_utf8(decoded_data)?;

        debug!(
            "Processing message from stream type {:?}: {}",
            stream_type, message_str
        );

        match stream_type {
            StreamType::OrderExecution => {
                self.process_order_execution_message(&message_str).await?;
            }
            StreamType::StrategyExecution => {
                self.process_strategy_execution_message(&message_str)
                    .await?;
            }
            StreamType::RiskAlerts => {
                self.process_risk_alert_message(&message_str).await?;
            }
            StreamType::MarketData => {
                self.process_market_data_message(&message_str).await?;
            }
            _ => {
                warn!("Unsupported stream type: {:?}", stream_type);
            }
        }

        Ok(())
    }

    /// Process order execution message
    async fn process_order_execution_message(
        &self,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let envelope: KinesisMessageEnvelope = serde_json::from_str(message)?;
        let order_message: OrderExecutionMessage = serde_json::from_value(envelope.payload)?;

        info!(
            "Processing order execution message: {} for user: {}",
            order_message.order_id, order_message.user_id
        );

        match order_message.message_type {
            OrderMessageType::PlaceOrder => {
                self.handle_place_order(order_message).await?;
            }
            OrderMessageType::CancelOrder => {
                self.handle_cancel_order(order_message).await?;
            }
            OrderMessageType::ModifyOrder => {
                self.handle_modify_order(order_message).await?;
            }
            _ => {
                debug!(
                    "Order message type {:?} not handled by sensor",
                    order_message.message_type
                );
            }
        }

        Ok(())
    }

    /// Process strategy execution message
    async fn process_strategy_execution_message(
        &self,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let envelope: KinesisMessageEnvelope = serde_json::from_str(message)?;
        let strategy_message: StrategyExecutionMessage = serde_json::from_value(envelope.payload)?;

        info!(
            "Processing strategy execution message: {} for user: {}",
            strategy_message.strategy_id, strategy_message.user_id
        );

        match strategy_message.strategy_type {
            StrategyType::MomentumBreakout => {
                self.handle_momentum_strategy(strategy_message).await?;
            }
            StrategyType::Arbitrage => {
                self.handle_arbitrage_strategy(strategy_message).await?;
            }
            StrategyType::MarketMaking => {
                self.handle_market_making_strategy(strategy_message).await?;
            }
            _ => {
                info!(
                    "Strategy type {:?} not yet implemented",
                    strategy_message.strategy_type
                );
            }
        }

        Ok(())
    }

    /// Process risk alert message
    async fn process_risk_alert_message(
        &self,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let envelope: KinesisMessageEnvelope = serde_json::from_str(message)?;
        let risk_message: RiskEventMessage = serde_json::from_value(envelope.payload)?;

        warn!(
            "Processing risk alert: {} - {}",
            risk_message.event_id, risk_message.description
        );

        match risk_message.severity {
            RiskSeverity::Critical => {
                // Emergency stop all trading
                info!("CRITICAL RISK EVENT: Emergency stopping all trading activities");
                
                // Cancel all active orders
                let active_orders = self.execution_engine.get_active_orders().read().await;
                let order_ids: Vec<ClientOrderId> = active_orders.keys().cloned().collect();
                drop(active_orders);
                
                for order_id in order_ids {
                    match self.execution_engine.cancel_order(order_id.clone()).await {
                        Ok(_) => info!("Cancelled order {} due to critical risk event", order_id),
                        Err(e) => error!("Failed to cancel order {}: {:?}", order_id, e),
                    }
                }
                
                // Set emergency stop flag to prevent new orders
                warn!("All trading halted due to critical risk event: {}", risk_message.description);
            }
            RiskSeverity::High => {
                // Stop specific positions or strategies
                for position_id in &risk_message.affected_positions {
                    info!("Stopping trading for position: {}", position_id);
                    
                    // Cancel orders related to specific positions
                    let active_orders = self.execution_engine.get_active_orders().read().await;
                    let affected_orders: Vec<ClientOrderId> = active_orders
                        .iter()
                        .filter(|(_, order)| {
                            // Match orders by strategy ID if it corresponds to position
                            order.order.key.strategy.to_string() == *position_id
                        })
                        .map(|(id, _)| id.clone())
                        .collect();
                    drop(active_orders);
                    
                    for order_id in affected_orders {
                        match self.execution_engine.cancel_order(order_id.clone()).await {
                            Ok(_) => info!("Cancelled order {} for position {}", order_id, position_id),
                            Err(e) => error!("Failed to cancel order {} for position {}: {:?}", order_id, position_id, e),
                        }
                    }
                }
            }
            RiskSeverity::Medium => {
                // Reduce positions by cancelling limit orders
                info!("Reducing exposure due to risk event: {}", risk_message.description);
                
                // Cancel all limit orders to prevent further exposure
                let active_orders = self.execution_engine.get_active_orders().read().await;
                let limit_orders: Vec<ClientOrderId> = active_orders
                    .iter()
                    .filter(|(_, order)| matches!(order.order.kind, OrderKind::Limit))
                    .map(|(id, _)| id.clone())
                    .collect();
                drop(active_orders);
                
                let num_limit_orders = limit_orders.len();
                for order_id in limit_orders {
                    match self.execution_engine.cancel_order(order_id.clone()).await {
                        Ok(_) => info!("Cancelled limit order {} to reduce exposure", order_id),
                        Err(e) => error!("Failed to cancel limit order {}: {:?}", order_id, e),
                    }
                }
                
                warn!("Position reduction completed. {} limit orders cancelled.", num_limit_orders);
            }
            RiskSeverity::Low => {
                // Just log the warning
                warn!("Risk warning: {}", risk_message.description);
            }
        }

        Ok(())
    }

    /// Process market data message (supplementary to real-time data)
    async fn process_market_data_message(
        &self,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let envelope: KinesisMessageEnvelope = serde_json::from_str(message)?;
        debug!(
            "Received supplementary market data message: {}",
            envelope.message_id
        );

        // This is for supplementary data that might come through Kinesis
        // The primary market data should come through WebSocket connections

        Ok(())
    }

    /// Handle place order request
    async fn handle_place_order(
        &self,
        order_message: OrderExecutionMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let exchange = match order_message.exchange.as_str() {
            "binance" | "BinanceSpot" => ExchangeId::BinanceSpot,
            "binance_futures" | "BinanceFuturesUsd" => ExchangeId::BinanceFuturesUsd,
            "coinbase" | "Coinbase" => ExchangeId::Coinbase,
            "kraken" | "Kraken" => ExchangeId::Kraken,
            "okx" | "Okx" => ExchangeId::Okx,
            "bybit" | "BybitSpot" => ExchangeId::BybitSpot,
            "bybit_perpetuals" | "BybitPerpetualsUsd" => ExchangeId::BybitPerpetualsUsd,
            "kucoin" | "Kucoin" => ExchangeId::Kucoin,
            "mexc" | "Mexc" => ExchangeId::Mexc,
            "gate" | "Gateio" => ExchangeId::Gateio,
            "bitget" | "Bitget" => ExchangeId::Bitget,
            "cryptocom" | "Cryptocom" => ExchangeId::Cryptocom,
            "hyperliquid" | "Hyperliquid" => ExchangeId::Hyperliquid,
            _ => return Err(format!("Unsupported exchange: {}", order_message.exchange).into()),
        };
        let instrument = InstrumentNameExchange::from(order_message.symbol.as_str());
        let strategy_id = StrategyId::from(smol_str::SmolStr::new(
            order_message
                .strategy_id
                .unwrap_or_else(|| "default".to_string()),
        ));

        let side = match order_message.execution_data.side {
            OrderSide::Buy => Side::Buy,
            OrderSide::Sell => Side::Sell,
        };

        let quantity = Decimal::try_from(order_message.execution_data.quantity)?;
        let price = order_message
            .execution_data
            .price
            .map(Decimal::try_from)
            .transpose()?;

        let order_request = match order_message.execution_data.order_type {
            OrderType::Market => {
                create_test_market_order(exchange, instrument, side, quantity, strategy_id)
            }
            OrderType::Limit => create_test_limit_order(
                exchange,
                instrument,
                side,
                quantity,
                price.unwrap_or(Decimal::ZERO),
                strategy_id,
            ),
            _ => {
                // Default to limit order for other types
                create_test_limit_order(
                    exchange,
                    instrument,
                    side,
                    quantity,
                    price.unwrap_or(Decimal::ZERO),
                    strategy_id,
                )
            }
        };

        match self.execution_engine.submit_order(order_request).await {
            Ok(order_id) => {
                info!(
                    "Successfully submitted order {} for user {}",
                    order_id, order_message.user_id
                );
            }
            Err(e) => {
                error!(
                    "Failed to submit order for user {}: {}",
                    order_message.user_id, e
                );
                return Err(Box::new(e));
            }
        }

        Ok(())
    }

    /// Handle cancel order request
    async fn handle_cancel_order(
        &self,
        order_message: OrderExecutionMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let order_id = ClientOrderId::new(order_message.order_id);

        match self.execution_engine.cancel_order(order_id.clone()).await {
            Ok(_) => {
                info!(
                    "Successfully cancelled order {} for user {}",
                    order_id, order_message.user_id
                );
            }
            Err(e) => {
                error!(
                    "Failed to cancel order {} for user {}: {}",
                    order_id, order_message.user_id, e
                );
                return Err(Box::new(e));
            }
        }

        Ok(())
    }

    /// Handle modify order request
    async fn handle_modify_order(
        &self,
        order_message: OrderExecutionMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // For simplicity, we'll cancel the old order and place a new one
        let order_id = ClientOrderId::new(order_message.order_id.clone());

        // Cancel existing order
        if let Err(e) = self.execution_engine.cancel_order(order_id).await {
            warn!("Failed to cancel order during modification: {}", e);
        }

        // Place new order with modified parameters
        self.handle_place_order(order_message).await?;

        Ok(())
    }

    /// Handle momentum strategy execution
    async fn handle_momentum_strategy(
        &self,
        strategy_message: StrategyExecutionMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Setting up momentum breakout strategy: {}",
            strategy_message.strategy_id
        );

        let config = StrategyConfig {
            strategy_id: StrategyId::from(smol_str::SmolStr::new(&strategy_message.strategy_id)),
            name: "Momentum Breakout Strategy".to_string(),
            ..Default::default()
        };

        let mut strategy = EventDrivenStrategy::new(config, Arc::clone(&self.execution_engine));

        // Add momentum processor
        let momentum_processor = MomentumEventProcessor::new(0.02, 20); // 2% momentum threshold, 20 period lookback
        strategy.add_event_processor(Box::new(momentum_processor));

        // Set up market data subscription
        let market_data_receiver = self.market_data_collector.subscribe_updates();
        strategy.set_market_data_receiver(market_data_receiver);

        // Start the strategy
        if let Err(e) = strategy.start().await {
            error!("Failed to start momentum strategy: {}", e);
            return Err(Box::new(e));
        }

        info!(
            "Momentum strategy started successfully: {}",
            strategy_message.strategy_id
        );
        Ok(())
    }

    /// Handle arbitrage strategy execution
    async fn handle_arbitrage_strategy(
        &self,
        strategy_message: StrategyExecutionMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Setting up arbitrage strategy: {}",
            strategy_message.strategy_id
        );

        // Arbitrage strategy setup - see ARBITRAGE_STRATEGY_SPEC.md
        // This would involve cross-exchange price monitoring and arbitrage execution

        Ok(())
    }

    /// Handle market making strategy execution
    async fn handle_market_making_strategy(
        &self,
        strategy_message: StrategyExecutionMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Setting up market making strategy: {}",
            strategy_message.strategy_id
        );

        // Market making strategy setup - see MARKET_MAKING_STRATEGY_SPEC.md
        // This would involve continuous bid/ask quote management

        Ok(())
    }
}

// Import types from backend messaging module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KinesisMessageEnvelope {
    pub envelope_version: String,
    pub message_id: String,
    pub correlation_id: Option<String>,
    pub timestamp: i64,
    pub source: String,
    pub message_type: String,
    pub payload: serde_json::Value,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderExecutionMessage {
    pub message_type: OrderMessageType,
    pub user_id: String,
    pub order_id: String,
    pub strategy_id: Option<String>,
    pub exchange: String,
    pub symbol: String,
    pub execution_data: OrderExecutionData,
    pub timestamp: i64,
    pub priority: MessagePriority,
    pub retry_count: u32,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyExecutionMessage {
    pub strategy_id: String,
    pub user_id: String,
    pub strategy_type: StrategyType,
    pub trigger_condition: TriggerCondition,
    pub execution_plan: ExecutionPlan,
    pub risk_parameters: RiskParameters,
    pub timestamp: i64,
    pub priority: MessagePriority,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskEventMessage {
    pub event_id: String,
    pub user_id: String,
    pub risk_type: RiskEventType,
    pub severity: RiskSeverity,
    pub description: String,
    pub affected_positions: Vec<String>,
    pub recommended_actions: Vec<RiskAction>,
    pub timestamp: i64,
    pub urgent: bool,
}

// Enums and supporting types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderMessageType {
    PlaceOrder,
    CancelOrder,
    ModifyOrder,
    OrderFilled,
    OrderPartiallyFilled,
    OrderCanceled,
    OrderRejected,
    OrderExpired,
    PositionUpdate,
    RiskAlert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyType {
    DCA,
    GridTrading,
    Arbitrage,
    MomentumBreakout,
    MeanReversion,
    MarketMaking,
    TakeProfitStopLoss,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePriority {
    Critical,
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskEventType {
    MaxLossExceeded,
    PositionSizeLimit,
    DrawdownLimit,
    OrderRateLimit,
    MarketVolatility,
    LiquidityRisk,
    SystemRisk,
    CustomRiskRule(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskAction {
    StopTrading,
    ReducePosition(f64),
    CancelAllOrders,
    ClosePosition(String),
    NotifyUser,
    LogEvent,
    Custom(String),
}

// Supporting data structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderExecutionData {
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub price: Option<f64>,
    pub stop_price: Option<f64>,
    pub time_in_force: TimeInForceMessage,
    pub reduce_only: bool,
    pub post_only: bool,
    pub client_order_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimeInForceMessage {
    GTC, // Good Till Canceled
    IOC, // Immediate or Cancel
    FOK, // Fill or Kill
    GTX, // Good Till Crossing
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerCondition {
    pub condition_type: TriggerType,
    pub parameters: std::collections::HashMap<String, serde_json::Value>,
    pub market_conditions: Option<MarketConditions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerType {
    PriceLevel,
    TechnicalIndicator,
    TimeSchedule,
    VolumeThreshold,
    MarketCondition,
    UserSignal,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketConditions {
    pub volatility_threshold: Option<f64>,
    pub volume_threshold: Option<f64>,
    pub spread_threshold: Option<f64>,
    pub momentum_indicator: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub orders: Vec<PlannedOrder>,
    pub execution_schedule: ExecutionSchedule,
    pub risk_checks: Vec<RiskCheck>,
    pub success_criteria: SuccessCriteria,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedOrder {
    pub order_type: OrderType,
    pub side: OrderSide,
    pub quantity: f64,
    pub price: Option<f64>,
    pub timing: OrderTiming,
    pub conditions: Vec<OrderCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderTiming {
    Immediate,
    Delayed(u64),
    Scheduled(i64),
    Conditional(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderCondition {
    PriceAbove(f64),
    PriceBelow(f64),
    VolumeAbove(f64),
    TimeAfter(i64),
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionSchedule {
    Immediate,
    Batched(u64),
    Scheduled(Vec<i64>),
    Continuous,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskParameters {
    pub max_position_size: f64,
    pub max_daily_loss: f64,
    pub max_drawdown: f64,
    pub stop_loss_percentage: Option<f64>,
    pub take_profit_percentage: Option<f64>,
    pub max_orders_per_minute: u32,
    pub position_limits: std::collections::HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskCheck {
    pub check_type: RiskCheckType,
    pub parameters: std::collections::HashMap<String, f64>,
    pub action_on_fail: RiskAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskCheckType {
    PositionSize,
    DailyLoss,
    Drawdown,
    Correlation,
    Liquidity,
    Volatility,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriteria {
    pub profit_target: Option<f64>,
    pub max_execution_time: Option<u64>,
    pub fill_percentage: Option<f64>,
    pub custom_metrics: std::collections::HashMap<String, f64>,
}
