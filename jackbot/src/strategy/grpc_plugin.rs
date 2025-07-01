use crate::{
    engine::state::EngineState,
    execution::ExecutionRequest,
    risk::RiskManager,
    strategy::{
        algo::AlgoStrategy,
        close_positions::ClosePositionsStrategy,
        on_disconnect::OnDisconnectStrategy,
        on_trading_disabled::OnTradingDisabled,
    },
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use jackbot_execution::{
    order::{OrderBuilder, OrderSide, OrderType},
    trade::ClientOrderId,
};
use jackbot_instrument::{ExchangeId, InstrumentIndex};
use rust_decimal::Decimal;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use tonic::{transport::Channel, Request, Status};

// Include the generated proto code
pub mod strategy {
    tonic::include_proto!("jackbot.strategy");
}

use strategy::{
    strategy_plugin_client::StrategyPluginClient, AccountState, Candle, InitializeRequest,
    MarketData, OrderRequest as ProtoOrderRequest, OrderSide as ProtoOrderSide,
    OrderType as ProtoOrderType, Position, PriceLevel, StateData, Trade,
};

/// Configuration for gRPC strategy plugin
#[derive(Debug, Clone)]
pub struct GrpcStrategyConfig {
    /// gRPC endpoint (e.g., "http://localhost:50051")
    pub endpoint: String,
    /// Strategy identifier
    pub strategy_id: String,
    /// Additional configuration parameters
    pub config: HashMap<String, String>,
    /// Instruments to trade
    pub instruments: Vec<String>,
    /// Initial capital for backtesting
    pub initial_capital: Decimal,
}

/// gRPC-based strategy plugin that connects to external strategy services
pub struct GrpcStrategyPlugin {
    client: Arc<Mutex<StrategyPluginClient<Channel>>>,
    config: GrpcStrategyConfig,
    initialized: bool,
}

impl GrpcStrategyPlugin {
    /// Create a new gRPC strategy plugin
    pub async fn new(config: GrpcStrategyConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let client = StrategyPluginClient::connect(config.endpoint.clone()).await?;

        Ok(Self {
            client: Arc::new(Mutex::new(client)),
            config,
            initialized: false,
        })
    }

    /// Initialize the strategy
    pub async fn initialize(
        &mut self,
        is_backtest: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut client = self.client.lock().await;

        let request = Request::new(InitializeRequest {
            strategy_id: self.config.strategy_id.clone(),
            config: self.config.config.clone(),
            instruments: self.config.instruments.clone(),
            initial_capital: self.config.initial_capital.to_f64().unwrap_or(10000.0),
            is_backtest,
            start_time: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
        });

        let response = client.initialize(request).await?.into_inner();

        if !response.success {
            return Err(format!("Strategy initialization failed: {}", response.message).into());
        }

        self.initialized = true;
        Ok(())
    }

    /// Convert internal market state to gRPC StateData
    fn convert_to_state_data<State>(
        &self,
        state: &State,
        instrument: InstrumentIndex,
    ) -> StateData
    where
        State: EngineState,
    {
        // Get instrument state
        let instrument_state = state.instruments().get(&instrument);
        
        // Encode state vector (this is where wave experiments with different dimensions)
        let state_vector = self.encode_state_vector(state, instrument);

        // Build market data
        let market_data = if let Some(inst_state) = instrument_state {
            Some(self.build_market_data(inst_state))
        } else {
            None
        };

        // Get positions
        let positions = self.extract_positions(state, instrument);

        // Get account state
        let account = Some(self.extract_account_state(state));

        StateData {
            instrument: instrument.to_string(),
            timestamp: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            state_vector,
            market_data,
            positions,
            account,
        }
    }

    /// Encode state into vector - this is where different waves experiment with dimensions
    fn encode_state_vector<State>(&self, _state: &State, _instrument: InstrumentIndex) -> Vec<f64>
    where
        State: EngineState,
    {
        // This will be dynamically configured based on wave experiments
        // For now, return a placeholder vector
        vec![0.0; 512] // Default 512 dimensions, but waves will experiment with different sizes
    }

    fn build_market_data<State>(&self, _instrument_state: &State) -> MarketData {
        // TODO: Extract real market data from instrument state
        MarketData {
            bid: 100.0,
            ask: 100.1,
            mid: 100.05,
            last: 100.0,
            bid_levels: vec![],
            ask_levels: vec![],
            recent_trades: vec![],
            indicators: HashMap::new(),
            candles: vec![],
        }
    }

    fn extract_positions<State>(
        &self,
        _state: &State,
        _instrument: InstrumentIndex,
    ) -> Vec<Position>
    where
        State: EngineState,
    {
        // TODO: Extract real positions from state
        vec![]
    }

    fn extract_account_state<State>(&self, _state: &State) -> AccountState
    where
        State: EngineState,
    {
        // TODO: Extract real account state
        AccountState {
            balance: 10000.0,
            equity: 10000.0,
            margin_used: 0.0,
            margin_available: 10000.0,
            total_pnl: 0.0,
        }
    }

    /// Convert gRPC order request to internal format
    fn convert_order_request(&self, order: ProtoOrderRequest) -> ExecutionRequest {
        let order_type = match ProtoOrderType::try_from(order.order_type) {
            Ok(ProtoOrderType::Market) => OrderType::Market,
            Ok(ProtoOrderType::Limit) => OrderType::Limit,
            _ => OrderType::Market,
        };

        let side = match ProtoOrderSide::try_from(order.side) {
            Ok(ProtoOrderSide::Buy) => OrderSide::Buy,
            Ok(ProtoOrderSide::Sell) => OrderSide::Sell,
            _ => OrderSide::Buy,
        };

        let mut builder = OrderBuilder::new(side, order.quantity.into());
        
        if order_type == OrderType::Limit {
            builder = builder.limit(order.price.into());
        }

        if order.stop_loss > 0.0 {
            builder = builder.stop_loss(order.stop_loss.into());
        }

        if order.take_profit > 0.0 {
            builder = builder.take_profit(order.take_profit.into());
        }

        ExecutionRequest::OpenPosition {
            order: builder.build(),
        }
    }
}

#[async_trait]
impl<Clock, State, Risk> AlgoStrategy<ExchangeId, InstrumentIndex>
    for GrpcStrategyPlugin
where
    Clock: Send + Sync,
    State: EngineState + Send + Sync,
    Risk: RiskManager<State = State> + Send + Sync,
{
    type Clock = Clock;
    type State = State;
    type ExecutionTxMap = ();
    type Risk = Risk;

    async fn generate_algo_orders(
        &self,
        _clock: &Self::Clock,
        state: &Self::State,
        _execution_tx: &Self::ExecutionTxMap,
        _risk: &Self::Risk,
    ) -> HashMap<InstrumentIndex, Vec<ExecutionRequest>> {
        if !self.initialized {
            return HashMap::new();
        }

        let mut all_orders = HashMap::new();

        // Process each instrument
        for instrument in &self.config.instruments {
            if let Ok(instrument_index) = instrument.parse::<InstrumentIndex>() {
                // Convert state to gRPC format
                let state_data = self.convert_to_state_data(state, instrument_index);

                // Call gRPC service
                let mut client = self.client.lock().await;
                let request = Request::new(state_data);

                match client.process_state(request).await {
                    Ok(response) => {
                        let signal = response.into_inner();
                        
                        // Convert orders
                        let orders: Vec<ExecutionRequest> = signal
                            .orders
                            .into_iter()
                            .map(|o| self.convert_order_request(o))
                            .collect();

                        if !orders.is_empty() {
                            all_orders.insert(instrument_index, orders);
                        }
                    }
                    Err(e) => {
                        tracing::error!("gRPC strategy error: {}", e);
                    }
                }
            }
        }

        all_orders
    }
}

// Implement other required traits with no-op implementations
impl<Clock, State, Risk> ClosePositionsStrategy<ExchangeId, InstrumentIndex, InstrumentIndex>
    for GrpcStrategyPlugin
where
    Clock: Send + Sync,
    State: EngineState + Send + Sync,
    Risk: RiskManager<State = State> + Send + Sync,
{
    type State = State;

    fn close_positions_requests<'a>(
        &'a self,
        _state: &'a Self::State,
        _filter: &'a crate::engine::state::instrument::filter::InstrumentFilter<
            ExchangeId,
            InstrumentIndex,
            InstrumentIndex,
        >,
    ) -> Box<
        dyn Iterator<
                Item = (
                    InstrumentIndex,
                    crate::engine::action::close_positions::ClosePositionRequest,
                ),
            > + 'a,
    > {
        Box::new(std::iter::empty())
    }
}

// OnTradingDisabled trait implementation
impl OnTradingDisabled for GrpcStrategyPlugin {
    type Engine = ();
    type OnTradingDisabled = ();

    fn on_trading_disabled(
        _engine: &mut Self::Engine,
    ) -> Self::OnTradingDisabled {
        ()
    }
}

// OnDisconnectStrategy trait implementation
impl OnDisconnectStrategy for GrpcStrategyPlugin {
    type Engine = ();
    type OnDisconnect = ();

    fn on_disconnect(
        _engine: &mut Self::Engine,
        _exchange: ExchangeId,
    ) -> Self::OnDisconnect {
        ()
    }
}