use crate::{
    engine::{execution_tx::MultiExchangeTxMap, state::EngineState},
    execution::request::ExecutionRequest,
    ml_api::{AsyncModel, RemoteModel},
    risk::RiskManager,
    strategy::{
        algo::AlgoStrategy, close_positions::ClosePositionsStrategy,
        on_disconnect::OnDisconnectStrategy, on_trading_disabled::OnTradingDisabled,
    },
};
use async_trait::async_trait;
use jackbot_execution::order::{
    OrderKind, TimeInForce,
    id::ClientOrderId,
    request::{OrderRequestOpen, RequestOpen},
};
use jackbot_instrument::{Side, exchange::ExchangeId, instrument::InstrumentIndex};
use jackbot_ta::indicators::RelativeStrengthIndex;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tokio::sync::Mutex;

/// ML-based trading strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlTradingConfig {
    /// ML API URL for model inference
    pub api_url: String,
    /// Model ID to use (e.g., "ensemble" or "qr_dqn")
    pub model_id: String,
    /// Position size as fraction of capital
    pub position_size: Decimal,
    /// Minimum confidence threshold for trades
    pub min_confidence: f64,
    /// Stop loss percentage
    pub stop_loss_pct: Decimal,
    /// Take profit percentage
    pub take_profit_pct: Decimal,
    /// Enable neural activation logging
    pub log_activations: bool,
}

/// State encoder for converting market data to 512-dim vector
pub struct StateEncoder {
    /// Historical price buffer
    price_history: VecDeque<Decimal>,
    /// RSI calculator
    rsi: RelativeStrengthIndex,
    /// Bollinger Bands calculator
    bb: BollingerBands,
}

impl StateEncoder {
    pub fn new() -> Self {
        Self {
            price_history: VecDeque::with_capacity(512),
            rsi: RelativeStrengthIndex::new(14),
            bb: BollingerBands::new(20, dec!(2.0)),
        }
    }

    /// Update with new price data
    pub fn update(&mut self, price: Decimal) {
        self.price_history.push_back(price);
        if self.price_history.len() > 512 {
            self.price_history.pop_front();
        }

        self.rsi.update(price.to_f64().unwrap_or_default());
        self.bb.update(price.to_f64().unwrap_or_default());
    }

    /// Encode current state as 512-dimensional vector
    pub fn encode(&self) -> Vec<f64> {
        let mut state = vec![0.0; 512];

        // Fill with normalized price history
        let prices: Vec<f64> = self
            .price_history
            .iter()
            .map(|p| p.to_f64().unwrap_or_default())
            .collect();

        if !prices.is_empty() {
            let mean = prices.iter().sum::<f64>() / prices.len() as f64;
            let std = (prices.iter().map(|p| (p - mean).powi(2)).sum::<f64>()
                / prices.len() as f64)
                .sqrt();

            for (i, price) in prices.iter().enumerate() {
                if i < 509 {
                    // Leave room for indicators
                    state[i] = if std > 0.0 { (price - mean) / std } else { 0.0 };
                }
            }
        }

        // Add technical indicators in last 3 positions
        state[509] = self.rsi.value().unwrap_or(50.0) / 100.0; // Normalize RSI to 0-1

        if let Some((upper, middle, lower)) = self.bb.value() {
            state[510] = (upper - middle) / middle; // Upper band distance
            state[511] = (middle - lower) / middle; // Lower band distance
        }

        state
    }
}

/// ML-based trading strategy
pub struct MlTradingStrategy<Clock, State, ExecutionTxs, Risk> {
    config: MlTradingConfig,
    model: Arc<dyn AsyncModel>,
    state_encoders: Arc<Mutex<HashMap<InstrumentIndex, StateEncoder>>>,
    positions: Arc<Mutex<HashMap<InstrumentIndex, Position>>>,
    _phantom: std::marker::PhantomData<(Clock, State, ExecutionTxs, Risk)>,
}

/// Position tracking
#[derive(Debug, Clone)]
struct Position {
    side: OrderSide,
    entry_price: Decimal,
    size: Decimal,
    stop_loss: Decimal,
    take_profit: Decimal,
}

impl<Clock, State, ExecutionTxs, Risk> MlTradingStrategy<Clock, State, ExecutionTxs, Risk> {
    pub fn new(config: MlTradingConfig) -> Self {
        let model = RemoteModel::new(&config.api_url, &config.model_id)
            .with_activations(config.log_activations);

        Self {
            config,
            model: Arc::new(model),
            state_encoders: Arc::new(Mutex::new(HashMap::new())),
            positions: Arc::new(Mutex::new(HashMap::new())),
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<Clock, GlobalData, InstrumentData, Risk> AlgoStrategy
    for MlTradingStrategy<Clock, EngineState<GlobalData, InstrumentData>, MultiExchangeTxMap, Risk>
where
    Clock: Send + Sync,
    GlobalData: Send + Sync,
    InstrumentData: Send + Sync,
    Risk: RiskManager<State = EngineState<GlobalData, InstrumentData>> + Send + Sync,
{
    type State = EngineState<GlobalData, InstrumentData>;

    async fn generate_algo_orders(
        &self,
        _clock: &Clock,
        state: &Self::State,
        _execution_txs: &MultiExchangeTxMap,
        _risk: &Risk,
    ) -> Vec<AlgoOrderRequest> {
        let mut orders = Vec::new();

        // Process each instrument
        for (instrument_index, instrument_state) in state.instruments.iter() {
            if let Some(last_trade) = instrument_state.last_trade() {
                let price = last_trade.price;

                // Update state encoder
                let mut encoders = self.state_encoders.lock().await;
                let encoder = encoders
                    .entry(instrument_index)
                    .or_insert_with(StateEncoder::new);
                encoder.update(price);

                // Get current state encoding
                let state_vector = encoder.encode();

                // Get ML prediction
                match self.model.predict(&state_vector).await {
                    Ok(prediction) => {
                        // Log prediction details
                        tracing::info!(
                            "ML Prediction - Instrument: {}, Action: {}, Confidence: {:.3}, Q-values: {:?}",
                            instrument_index,
                            prediction.action,
                            prediction.confidence,
                            prediction.q_values
                        );

                        // Only trade if confidence exceeds threshold
                        if prediction.confidence >= self.config.min_confidence {
                            let positions = self.positions.lock().await;
                            let has_position = positions.contains_key(&instrument_index);

                            match prediction.action {
                                0 if !has_position => {
                                    // Open long
                                    let size = self.config.position_size;
                                    let stop_loss = price * (dec!(1) - self.config.stop_loss_pct);
                                    let take_profit =
                                        price * (dec!(1) + self.config.take_profit_pct);

                                    let order = OrderBuilder::market(OrderSide::Buy)
                                        .quantity(size)
                                        .reduce_only(false)
                                        .build();

                                    orders.push(AlgoOrderRequest {
                                        instrument_index,
                                        exchange_id: ExchangeId::Binance, // Configure per instrument
                                        order,
                                    });

                                    tracing::info!(
                                        "Opening LONG position - Price: {}, SL: {}, TP: {}",
                                        price,
                                        stop_loss,
                                        take_profit
                                    );
                                }
                                1 if !has_position => {
                                    // Open short
                                    let size = self.config.position_size;
                                    let stop_loss = price * (dec!(1) + self.config.stop_loss_pct);
                                    let take_profit =
                                        price * (dec!(1) - self.config.take_profit_pct);

                                    let order = OrderBuilder::market(OrderSide::Sell)
                                        .quantity(size)
                                        .reduce_only(false)
                                        .build();

                                    orders.push(AlgoOrderRequest {
                                        instrument_index,
                                        exchange_id: ExchangeId::Binance,
                                        order,
                                    });

                                    tracing::info!(
                                        "Opening SHORT position - Price: {}, SL: {}, TP: {}",
                                        price,
                                        stop_loss,
                                        take_profit
                                    );
                                }
                                2 if has_position => {
                                    // Close long
                                    if let Some(position) = positions.get(&instrument_index) {
                                        if position.side == OrderSide::Buy {
                                            let order = OrderBuilder::market(OrderSide::Sell)
                                                .quantity(position.size)
                                                .reduce_only(true)
                                                .build();

                                            orders.push(AlgoOrderRequest {
                                                instrument_index,
                                                exchange_id: ExchangeId::Binance,
                                                order,
                                            });

                                            tracing::info!("Closing LONG position");
                                        }
                                    }
                                }
                                3 if has_position => {
                                    // Close short
                                    if let Some(position) = positions.get(&instrument_index) {
                                        if position.side == OrderSide::Sell {
                                            let order = OrderBuilder::market(OrderSide::Buy)
                                                .quantity(position.size)
                                                .reduce_only(true)
                                                .build();

                                            orders.push(AlgoOrderRequest {
                                                instrument_index,
                                                exchange_id: ExchangeId::Binance,
                                                order,
                                            });

                                            tracing::info!("Closing SHORT position");
                                        }
                                    }
                                }
                                _ => {
                                    // Hold - do nothing
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("ML prediction error: {}", e);
                    }
                }
            }
        }

        orders
    }
}

// Implement other required traits with default/empty implementations
#[async_trait]
impl<Clock, GlobalData, InstrumentData, Risk> ClosePositionsStrategy
    for MlTradingStrategy<Clock, EngineState<GlobalData, InstrumentData>, MultiExchangeTxMap, Risk>
where
    Clock: Send + Sync,
    GlobalData: Send + Sync,
    InstrumentData: Send + Sync,
    Risk: RiskManager<State = EngineState<GlobalData, InstrumentData>> + Send + Sync,
{
    type State = EngineState<GlobalData, InstrumentData>;

    async fn close_positions(
        &self,
        _clock: &Clock,
        _state: &Self::State,
        _execution_txs: &MultiExchangeTxMap,
        _risk: &Risk,
    ) -> (Vec<CancelRequest>, Vec<ExecutionRequest>) {
        // Implement position closing logic if needed
        (vec![], vec![])
    }
}

impl<Clock, GlobalData, InstrumentData, Risk>
    OnTradingDisabled<Clock, EngineState<GlobalData, InstrumentData>, MultiExchangeTxMap, Risk>
    for MlTradingStrategy<Clock, EngineState<GlobalData, InstrumentData>, MultiExchangeTxMap, Risk>
where
    Clock: Send + Sync,
    GlobalData: Send + Sync,
    InstrumentData: Send + Sync,
    Risk: RiskManager<State = EngineState<GlobalData, InstrumentData>> + Send + Sync,
{
    type OnTradingDisabled = Self;

    fn on_trading_disabled(&self) -> Self::OnTradingDisabled {
        self.clone()
    }
}

impl<Clock, GlobalData, InstrumentData, Risk>
    OnDisconnectStrategy<Clock, EngineState<GlobalData, InstrumentData>, MultiExchangeTxMap, Risk>
    for MlTradingStrategy<Clock, EngineState<GlobalData, InstrumentData>, MultiExchangeTxMap, Risk>
where
    Clock: Send + Sync,
    GlobalData: Send + Sync,
    InstrumentData: Send + Sync,
    Risk: RiskManager<State = EngineState<GlobalData, InstrumentData>> + Send + Sync,
{
    type OnDisconnect = Self;

    fn on_disconnect(&self) -> Self::OnDisconnect {
        self.clone()
    }
}

// Implement Clone manually due to Arc<dyn AsyncModel>
impl<Clock, State, ExecutionTxs, Risk> Clone
    for MlTradingStrategy<Clock, State, ExecutionTxs, Risk>
{
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            model: Arc::clone(&self.model),
            state_encoders: Arc::clone(&self.state_encoders),
            positions: Arc::clone(&self.positions),
            _phantom: std::marker::PhantomData,
        }
    }
}
