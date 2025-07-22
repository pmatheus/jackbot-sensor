use crate::{
    engine::state::EngineState,
    execution::request::ExecutionRequest,
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
use jackbot_execution::order::id::ClientOrderId;
use jackbot_instrument::{ExchangeId, InstrumentIndex};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
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
        _state: &State,
        _instrument: InstrumentIndex,
    ) -> StateData
    {
        // Extract real state data - see API_IMPLEMENTATION_SPEC.md#grpc-state-extraction
        // For now, return placeholder data
        StateData {
            instrument: "".to_string(),
            account: Some(self.convert_account_state()),
            market_data: None,
            positions: vec![],
            state_vector: vec![0.0; 512],
            timestamp: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
        }
    }

    /// Extract account state
    fn convert_account_state(&self) -> AccountState {
        // Extract real account state - see API_IMPLEMENTATION_SPEC.md#account-state-integration
        AccountState {
            balance: 10000.0,
            equity: 10000.0,
            margin_used: 0.0,
            margin_available: 10000.0,
            total_pnl: 0.0,
        }
    }

    /// Convert gRPC order request to internal format
    fn convert_order_request(&self, _order: ProtoOrderRequest) -> ExecutionRequest {
        // Implement order request conversion properly - see API_IMPLEMENTATION_SPEC.md#grpc-order-conversion
        // For now, return a placeholder to allow compilation
        ExecutionRequest::Shutdown
    }
}

impl AlgoStrategy for GrpcStrategyPlugin {
    type State = EngineState<(), ()>;

    fn generate_algo_orders(
        &self,
        _state: &Self::State,
    ) -> (
        impl IntoIterator<Item = jackbot_execution::order::request::OrderRequestCancel<jackbot_instrument::exchange::ExchangeIndex, InstrumentIndex>>,
        impl IntoIterator<Item = jackbot_execution::order::request::OrderRequestOpen<jackbot_instrument::exchange::ExchangeIndex, InstrumentIndex>>,
    ) {
        // Implement gRPC strategy plugin properly - see API_IMPLEMENTATION_SPEC.md#grpc-plugin-implementation
        // For now, return empty iterators to allow compilation
        tracing::warn!("gRPC Strategy Plugin: Not implemented - returning empty orders");
        (std::iter::empty(), std::iter::empty())
    }
}

// Implement other required traits with no-op implementations
impl ClosePositionsStrategy for GrpcStrategyPlugin {
    type State = EngineState<(), ()>;

    fn close_positions_requests<'a>(
        &'a self,
        _state: &'a Self::State,
        _filter: &'a crate::engine::state::instrument::filter::InstrumentFilter<jackbot_instrument::exchange::ExchangeIndex, jackbot_instrument::asset::AssetIndex, InstrumentIndex>,
    ) -> (
        impl IntoIterator<Item = jackbot_execution::order::request::OrderRequestCancel<jackbot_instrument::exchange::ExchangeIndex, InstrumentIndex>> + 'a,
        impl IntoIterator<Item = jackbot_execution::order::request::OrderRequestOpen<jackbot_instrument::exchange::ExchangeIndex, InstrumentIndex>> + 'a,
    )
    where
        jackbot_instrument::exchange::ExchangeIndex: 'a,
        jackbot_instrument::asset::AssetIndex: 'a,
        InstrumentIndex: 'a,
    {
        (std::iter::empty(), std::iter::empty())
    }
}

// OnTradingDisabled trait implementation
impl<Clock, State, ExecutionTxs, Risk> OnTradingDisabled<Clock, State, ExecutionTxs, Risk> for GrpcStrategyPlugin {
    type OnTradingDisabled = ();

    fn on_trading_disabled(
        _engine: &mut crate::engine::Engine<Clock, State, ExecutionTxs, Self, Risk>,
    ) -> Self::OnTradingDisabled {
        ()
    }
}

// OnDisconnectStrategy trait implementation
impl<Clock, State, ExecutionTxs, Risk> OnDisconnectStrategy<Clock, State, ExecutionTxs, Risk> for GrpcStrategyPlugin {
    type OnDisconnect = ();

    fn on_disconnect(
        _engine: &mut crate::engine::Engine<Clock, State, ExecutionTxs, Self, Risk>,
        _exchange: ExchangeId,
    ) -> Self::OnDisconnect {
        ()
    }
}