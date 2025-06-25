use crate::strategy::events::{
    current_timestamp_ms, EventDrivenStrategy, EventFilter, MarketEvent, MarketEventType,
    SignalUrgency, StrategyContext, StrategyError, StrategyMetrics, StrategyParameters,
    StrategySignal, TradeSide,
};
use async_trait::async_trait;
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange};
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};
use tracing::{debug, info};

/// Event-driven TWAP (Time-Weighted Average Price) strategy for sensor operations
#[derive(Debug)]
pub struct SensorTwapStrategy {
    id: String,
    exchange: ExchangeId,
    instrument: InstrumentNameExchange,
    target_quantity: Decimal,
    remaining_quantity: Decimal,
    duration: Duration,
    slice_count: usize,
    slice_interval: Duration,
    start_time: Option<Instant>,
    price_tolerance: Decimal,
    last_execution: Option<Instant>,
    metrics: StrategyMetrics,
    // Sensor-specific enhancements
    market_volatility_threshold: Decimal,
    adaptive_sizing: bool,
    order_book_depth_threshold: Decimal,
    min_slice_size: Decimal,
    market_state_cache: MarketStateCache,
}

/// Enhanced VWAP strategy for real-time volume tracking
#[derive(Debug)]
pub struct SensorVwapStrategy {
    id: String,
    exchange: ExchangeId,
    instrument: InstrumentNameExchange,
    target_quantity: Decimal,
    remaining_quantity: Decimal,
    volume_profile: VolumeProfile,
    adaptive_participation: bool,
    target_participation_rate: f64,
    max_participation_rate: f64,
    metrics: StrategyMetrics,
    market_state_cache: MarketStateCache,
}

/// Event-driven Iceberg strategy with real-time adaptation
#[derive(Debug)]
pub struct SensorIcebergStrategy {
    id: String,
    exchange: ExchangeId,
    instrument: InstrumentNameExchange,
    total_quantity: Decimal,
    remaining_quantity: Decimal,
    base_chunk_size: Decimal,
    active_orders: Vec<String>,
    max_concurrent_orders: usize,
    adaptive_chunk_sizing: bool,
    metrics: StrategyMetrics,
    market_state_cache: MarketStateCache,
}

/// POV strategy with real-time volume adaptation
#[derive(Debug)]
pub struct SensorPovStrategy {
    id: String,
    exchange: ExchangeId,
    instrument: InstrumentNameExchange,
    target_quantity: Decimal,
    remaining_quantity: Decimal,
    target_participation_rate: f64,
    volume_tracker: VolumeTracker,
    assessment_interval: Duration,
    last_assessment: Option<Instant>,
    metrics: StrategyMetrics,
    market_state_cache: MarketStateCache,
}

/// Real-time market state cache for sensor strategies
#[derive(Debug, Clone)]
struct MarketStateCache {
    last_price: Option<Decimal>,
    best_bid: Option<Decimal>,
    best_ask: Option<Decimal>,
    spread: Option<Decimal>,
    recent_trades: VecDeque<TradeInfo>,
    volume_spike_detected: bool,
    last_update: Instant,
}

#[derive(Debug, Clone)]
struct TradeInfo {
    price: Decimal,
    volume: Decimal,
    timestamp: Instant,
    side: TradeSide,
}

#[derive(Debug, Clone)]
struct VolumeProfile {
    total_volume: Decimal,
    volume_buckets: VecDeque<(Instant, Decimal)>,
    avg_volume_per_minute: Decimal,
    window_duration: Duration,
}

#[derive(Debug, Clone)]
struct VolumeTracker {
    recent_volume: VecDeque<(Instant, Decimal)>,
    window_duration: Duration,
    volume_rate: Decimal,
}

impl MarketStateCache {
    fn new() -> Self {
        Self {
            last_price: None,
            best_bid: None,
            best_ask: None,
            spread: None,
            recent_trades: VecDeque::new(),
            volume_spike_detected: false,
            last_update: Instant::now(),
        }
    }

    fn update_from_order_book(&mut self, bids: &[(Decimal, Decimal)], asks: &[(Decimal, Decimal)]) {
        if let Some((best_bid_price, _)) = bids.first() {
            self.best_bid = Some(*best_bid_price);
        }
        if let Some((best_ask_price, _)) = asks.first() {
            self.best_ask = Some(*best_ask_price);
        }

        if let (Some(bid), Some(ask)) = (self.best_bid, self.best_ask) {
            self.spread = Some(ask - bid);
            self.last_price = Some((bid + ask) / Decimal::TWO);
        }

        self.last_update = Instant::now();
    }

    fn update_from_trade(&mut self, price: Decimal, volume: Decimal, side: TradeSide) {
        self.last_price = Some(price);

        let trade_info = TradeInfo {
            price,
            volume,
            timestamp: Instant::now(),
            side,
        };

        self.recent_trades.push_back(trade_info);

        // Keep only recent trades (last 5 minutes)
        let cutoff = Instant::now() - Duration::from_secs(300);
        self.recent_trades.retain(|trade| trade.timestamp >= cutoff);

        self.last_update = Instant::now();
    }

    fn is_volatile(&self, threshold: Decimal) -> bool {
        if let Some(spread) = self.spread {
            if let Some(price) = self.last_price {
                let spread_bps = (spread / price) * Decimal::from(10000);
                return spread_bps > threshold;
            }
        }
        false
    }

    fn get_recent_volume(&self, window: Duration) -> Decimal {
        let cutoff = Instant::now() - window;
        self.recent_trades
            .iter()
            .filter(|trade| trade.timestamp >= cutoff)
            .map(|trade| trade.volume)
            .sum()
    }
}

impl VolumeTracker {
    fn new(window_duration: Duration) -> Self {
        Self {
            recent_volume: VecDeque::new(),
            window_duration,
            volume_rate: Decimal::ZERO,
        }
    }

    fn add_volume(&mut self, volume: Decimal) {
        let now = Instant::now();
        self.recent_volume.push_back((now, volume));

        // Clean up old entries
        let cutoff = now - self.window_duration;
        self.recent_volume
            .retain(|(timestamp, _)| *timestamp >= cutoff);

        // Update volume rate
        let total_volume: Decimal = self.recent_volume.iter().map(|(_, vol)| *vol).sum();
        let window_seconds = self.window_duration.as_secs() as f64;
        self.volume_rate = total_volume / Decimal::from_f64(window_seconds).unwrap_or(Decimal::ONE);
    }

    fn get_volume_rate(&self) -> Decimal {
        self.volume_rate
    }
}

// TWAP Strategy Implementation
impl SensorTwapStrategy {
    pub fn new(
        id: String,
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        target_quantity: Decimal,
        duration: Duration,
        slice_count: usize,
    ) -> Self {
        let slice_interval = duration / slice_count as u32;
        let min_slice_size = target_quantity / Decimal::from(slice_count * 10); // 10% of average slice

        Self {
            id,
            exchange,
            instrument,
            target_quantity,
            remaining_quantity: target_quantity,
            duration,
            slice_count,
            slice_interval,
            start_time: None,
            price_tolerance: Decimal::from_str("0.0020").unwrap(), // 20 bps
            last_execution: None,
            metrics: StrategyMetrics {
                evaluation_time_us: 0,
                events_processed: 0,
                signals_generated: 0,
                errors: 0,
                last_update: Instant::now(),
            },
            market_volatility_threshold: Decimal::from_str("0.0050").unwrap(), // 50 bps
            adaptive_sizing: true,
            order_book_depth_threshold: Decimal::from_str("10000").unwrap(),
            min_slice_size,
            market_state_cache: MarketStateCache::new(),
        }
    }

    fn should_execute_slice(&self) -> bool {
        let Some(start_time) = self.start_time else {
            return true; // First execution
        };

        let _elapsed = start_time.elapsed();
        let Some(last_execution) = self.last_execution else {
            return true;
        };

        // Check if enough time has passed since last execution
        last_execution.elapsed() >= self.slice_interval
    }

    fn calculate_next_slice_size(&self) -> Decimal {
        if !self.adaptive_sizing {
            return self.target_quantity / Decimal::from(self.slice_count);
        }

        let base_slice_size = self.remaining_quantity / Decimal::from(self.slice_count);

        // Adjust based on market conditions
        let mut adjustment_factor = Decimal::ONE;

        // Reduce size in volatile markets
        if self
            .market_state_cache
            .is_volatile(self.market_volatility_threshold)
        {
            adjustment_factor *= Decimal::from_str("0.7").unwrap();
        }

        // Check order book depth
        if let Some(spread) = self.market_state_cache.spread {
            if spread > self.price_tolerance {
                adjustment_factor *= Decimal::from_str("0.8").unwrap();
            }
        }

        // Recent volume check
        let recent_volume = self
            .market_state_cache
            .get_recent_volume(Duration::from_secs(60));
        if recent_volume < self.order_book_depth_threshold {
            adjustment_factor *= Decimal::from_str("0.6").unwrap();
        }

        let adjusted_size = base_slice_size * adjustment_factor;
        adjusted_size
            .max(self.min_slice_size)
            .min(self.remaining_quantity)
    }
}

#[async_trait]
impl EventDrivenStrategy for SensorTwapStrategy {
    fn id(&self) -> &str {
        &self.id
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            exchanges: Some(vec![self.exchange.clone()]),
            instruments: Some(vec![self.instrument.clone()]),
            event_types: vec![
                MarketEventType::OrderBookUpdate,
                MarketEventType::Trade,
                MarketEventType::PriceTick,
            ],
            min_volume: Some(Decimal::from_str("100").unwrap()),
            max_latency_ms: Some(25), // 25ms max for high-frequency
        }
    }

    async fn process_event(
        &mut self,
        event: &MarketEvent,
        _context: &StrategyContext,
    ) -> Result<Vec<StrategySignal>, StrategyError> {
        let start_time = Instant::now();
        self.metrics.events_processed += 1;

        // Update market state cache
        match event {
            MarketEvent::OrderBookUpdate { bids, asks, .. } => {
                self.market_state_cache.update_from_order_book(bids, asks);
            }
            MarketEvent::Trade {
                price,
                volume,
                side,
                ..
            } => {
                let trade_side = match side {
                    TradeSide::Buy => TradeSide::Buy,
                    TradeSide::Sell => TradeSide::Sell,
                };
                self.market_state_cache
                    .update_from_trade(*price, *volume, trade_side);
            }
            _ => {}
        }

        let mut signals = Vec::new();

        // Check if we should execute a slice
        if self.remaining_quantity > Decimal::ZERO && self.should_execute_slice() {
            if self.start_time.is_none() {
                self.start_time = Some(Instant::now());
            }

            let slice_size = self.calculate_next_slice_size();

            if slice_size > Decimal::ZERO {
                let signal = StrategySignal::Execute {
                    strategy_id: self.id.clone(),
                    exchange: self.exchange.clone(),
                    instrument: self.instrument.clone(),
                    parameters: StrategyParameters {
                        quantity: slice_size,
                        price: self.market_state_cache.last_price,
                        side: if self.target_quantity > Decimal::ZERO {
                            TradeSide::Buy
                        } else {
                            TradeSide::Sell
                        },
                        strategy_type: "TWAP".to_string(),
                        custom_params: HashMap::new(),
                    },
                    urgency: if self
                        .market_state_cache
                        .is_volatile(self.market_volatility_threshold)
                    {
                        SignalUrgency::High
                    } else {
                        SignalUrgency::Medium
                    },
                    timestamp: current_timestamp_ms(),
                };

                signals.push(signal);
                self.remaining_quantity -= slice_size;
                self.last_execution = Some(Instant::now());
                self.metrics.signals_generated += 1;

                debug!(
                    strategy_id = %self.id,
                    slice_size = %slice_size,
                    remaining = %self.remaining_quantity,
                    "Generated TWAP slice signal"
                );
            }
        }

        // Update metrics
        self.metrics.evaluation_time_us = start_time.elapsed().as_micros() as u64;
        self.metrics.last_update = Instant::now();

        Ok(signals)
    }

    async fn initialize(&mut self, _context: &StrategyContext) -> Result<(), StrategyError> {
        info!(
            strategy_id = %self.id,
            exchange = ?self.exchange,
            instrument = %self.instrument,
            target_quantity = %self.target_quantity,
            slice_count = self.slice_count,
            "Initialized sensor TWAP strategy"
        );
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), StrategyError> {
        info!(strategy_id = %self.id, "Shutting down sensor TWAP strategy");
        Ok(())
    }

    fn metrics(&self) -> StrategyMetrics {
        self.metrics.clone()
    }

    async fn health_check(&self) -> bool {
        // Strategy is healthy if it has processed events recently
        self.metrics.last_update.elapsed() < Duration::from_secs(60)
    }
}

// VWAP Strategy Implementation
impl SensorVwapStrategy {
    pub fn new(
        id: String,
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        target_quantity: Decimal,
        target_participation_rate: f64,
    ) -> Self {
        Self {
            id,
            exchange,
            instrument,
            target_quantity,
            remaining_quantity: target_quantity,
            volume_profile: VolumeProfile {
                total_volume: Decimal::ZERO,
                volume_buckets: VecDeque::new(),
                avg_volume_per_minute: Decimal::ZERO,
                window_duration: Duration::from_secs(300), // 5 minutes
            },
            adaptive_participation: true,
            target_participation_rate,
            max_participation_rate: target_participation_rate * 2.0,
            metrics: StrategyMetrics {
                evaluation_time_us: 0,
                events_processed: 0,
                signals_generated: 0,
                errors: 0,
                last_update: Instant::now(),
            },
            market_state_cache: MarketStateCache::new(),
        }
    }

    fn calculate_participation_quantity(&self, recent_volume: Decimal) -> Decimal {
        let base_quantity =
            recent_volume * Decimal::from_f64(self.target_participation_rate).unwrap();

        if self.adaptive_participation {
            // Adjust based on market conditions
            let mut adjustment = Decimal::ONE;

            // Increase participation in favorable conditions
            if !self
                .market_state_cache
                .is_volatile(Decimal::from_str("0.0030").unwrap())
            {
                adjustment *= Decimal::from_str("1.2").unwrap();
            }

            // Cap at max participation rate
            let max_quantity =
                recent_volume * Decimal::from_f64(self.max_participation_rate).unwrap();
            (base_quantity * adjustment)
                .min(max_quantity)
                .min(self.remaining_quantity)
        } else {
            base_quantity.min(self.remaining_quantity)
        }
    }
}

#[async_trait]
impl EventDrivenStrategy for SensorVwapStrategy {
    fn id(&self) -> &str {
        &self.id
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            exchanges: Some(vec![self.exchange.clone()]),
            instruments: Some(vec![self.instrument.clone()]),
            event_types: vec![
                MarketEventType::Trade,
                MarketEventType::VolumeSpike,
                MarketEventType::OrderBookUpdate,
            ],
            min_volume: Some(Decimal::from_str("50").unwrap()),
            max_latency_ms: Some(30),
        }
    }

    async fn process_event(
        &mut self,
        event: &MarketEvent,
        _context: &StrategyContext,
    ) -> Result<Vec<StrategySignal>, StrategyError> {
        let start_time = Instant::now();
        self.metrics.events_processed += 1;

        let mut signals = Vec::new();

        match event {
            MarketEvent::Trade {
                price,
                volume,
                side,
                ..
            } => {
                let trade_side = match side {
                    TradeSide::Buy => TradeSide::Buy,
                    TradeSide::Sell => TradeSide::Sell,
                };
                self.market_state_cache
                    .update_from_trade(*price, *volume, trade_side);

                // Update volume profile
                let now = Instant::now();
                self.volume_profile.volume_buckets.push_back((now, *volume));
                self.volume_profile.total_volume += *volume;

                // Clean up old volume data
                let cutoff = now - self.volume_profile.window_duration;
                while let Some(&(timestamp, old_volume)) =
                    self.volume_profile.volume_buckets.front()
                {
                    if timestamp < cutoff {
                        self.volume_profile.volume_buckets.pop_front();
                        self.volume_profile.total_volume -= old_volume;
                    } else {
                        break;
                    }
                }

                // Calculate recent volume and determine order quantity
                let recent_volume = self.volume_profile.total_volume;
                let order_quantity = self.calculate_participation_quantity(recent_volume);

                if order_quantity > Decimal::ZERO && self.remaining_quantity > Decimal::ZERO {
                    let signal = StrategySignal::Execute {
                        strategy_id: self.id.clone(),
                        exchange: self.exchange.clone(),
                        instrument: self.instrument.clone(),
                        parameters: StrategyParameters {
                            quantity: order_quantity,
                            price: Some(*price),
                            side: if self.target_quantity > Decimal::ZERO {
                                TradeSide::Buy
                            } else {
                                TradeSide::Sell
                            },
                            strategy_type: "VWAP".to_string(),
                            custom_params: {
                                let mut params = HashMap::new();
                                params.insert(
                                    "recent_volume".to_string(),
                                    serde_json::json!(recent_volume),
                                );
                                params.insert(
                                    "participation_rate".to_string(),
                                    serde_json::json!(self.target_participation_rate),
                                );
                                params
                            },
                        },
                        urgency: SignalUrgency::Medium,
                        timestamp: current_timestamp_ms(),
                    };

                    signals.push(signal);
                    self.remaining_quantity -= order_quantity;
                    self.metrics.signals_generated += 1;

                    debug!(
                        strategy_id = %self.id,
                        order_quantity = %order_quantity,
                        recent_volume = %recent_volume,
                        remaining = %self.remaining_quantity,
                        "Generated VWAP order signal"
                    );
                }
            }
            MarketEvent::OrderBookUpdate { bids, asks, .. } => {
                self.market_state_cache.update_from_order_book(bids, asks);
            }
            MarketEvent::VolumeSpike { volume, .. } => {
                // Respond to volume spikes with increased urgency
                if self.remaining_quantity > Decimal::ZERO {
                    let spike_quantity =
                        *volume * Decimal::from_f64(self.target_participation_rate * 1.5).unwrap();
                    let order_quantity = spike_quantity.min(self.remaining_quantity);

                    if order_quantity > Decimal::ZERO {
                        let signal = StrategySignal::Execute {
                            strategy_id: self.id.clone(),
                            exchange: self.exchange.clone(),
                            instrument: self.instrument.clone(),
                            parameters: StrategyParameters {
                                quantity: order_quantity,
                                price: self.market_state_cache.last_price,
                                side: if self.target_quantity > Decimal::ZERO {
                                    TradeSide::Buy
                                } else {
                                    TradeSide::Sell
                                },
                                strategy_type: "VWAP_SPIKE".to_string(),
                                custom_params: HashMap::new(),
                            },
                            urgency: SignalUrgency::High,
                            timestamp: current_timestamp_ms(),
                        };

                        signals.push(signal);
                        self.remaining_quantity -= order_quantity;
                        self.metrics.signals_generated += 1;
                    }
                }
            }
            _ => {}
        }

        self.metrics.evaluation_time_us = start_time.elapsed().as_micros() as u64;
        self.metrics.last_update = Instant::now();

        Ok(signals)
    }

    async fn initialize(&mut self, _context: &StrategyContext) -> Result<(), StrategyError> {
        info!(
            strategy_id = %self.id,
            exchange = ?self.exchange,
            instrument = %self.instrument,
            target_quantity = %self.target_quantity,
            target_participation_rate = self.target_participation_rate,
            "Initialized sensor VWAP strategy"
        );
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), StrategyError> {
        info!(strategy_id = %self.id, "Shutting down sensor VWAP strategy");
        Ok(())
    }

    fn metrics(&self) -> StrategyMetrics {
        self.metrics.clone()
    }
}

// Iceberg Strategy Implementation
impl SensorIcebergStrategy {
    pub fn new(
        id: String,
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        total_quantity: Decimal,
        base_chunk_size: Decimal,
        max_concurrent_orders: usize,
    ) -> Self {
        Self {
            id,
            exchange,
            instrument,
            total_quantity,
            remaining_quantity: total_quantity,
            base_chunk_size,
            active_orders: Vec::new(),
            max_concurrent_orders,
            adaptive_chunk_sizing: true,
            metrics: StrategyMetrics {
                evaluation_time_us: 0,
                events_processed: 0,
                signals_generated: 0,
                errors: 0,
                last_update: Instant::now(),
            },
            market_state_cache: MarketStateCache::new(),
        }
    }

    fn calculate_chunk_size(&self) -> Decimal {
        if !self.adaptive_chunk_sizing {
            return self.base_chunk_size.min(self.remaining_quantity);
        }

        let mut chunk_size = self.base_chunk_size;

        // Adjust based on market conditions
        if self
            .market_state_cache
            .is_volatile(Decimal::from_str("0.0040").unwrap())
        {
            chunk_size *= Decimal::from_str("0.6").unwrap(); // Smaller chunks in volatile markets
        }

        // Consider recent volume
        let recent_volume = self
            .market_state_cache
            .get_recent_volume(Duration::from_secs(120));
        if recent_volume > Decimal::from_str("50000").unwrap() {
            chunk_size *= Decimal::from_str("1.3").unwrap(); // Larger chunks in high-volume periods
        }

        chunk_size.min(self.remaining_quantity)
    }

    fn should_place_chunk(&self) -> bool {
        self.active_orders.len() < self.max_concurrent_orders
            && self.remaining_quantity > Decimal::ZERO
    }
}

#[async_trait]
impl EventDrivenStrategy for SensorIcebergStrategy {
    fn id(&self) -> &str {
        &self.id
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            exchanges: Some(vec![self.exchange.clone()]),
            instruments: Some(vec![self.instrument.clone()]),
            event_types: vec![MarketEventType::OrderBookUpdate, MarketEventType::Trade],
            min_volume: None,
            max_latency_ms: Some(20),
        }
    }

    async fn process_event(
        &mut self,
        event: &MarketEvent,
        _context: &StrategyContext,
    ) -> Result<Vec<StrategySignal>, StrategyError> {
        let start_time = Instant::now();
        self.metrics.events_processed += 1;

        let mut signals = Vec::new();

        match event {
            MarketEvent::OrderBookUpdate { bids, asks, .. } => {
                self.market_state_cache.update_from_order_book(bids, asks);

                // Check if we should place a new chunk
                if self.should_place_chunk() {
                    let chunk_size = self.calculate_chunk_size();

                    if chunk_size > Decimal::ZERO {
                        let signal = StrategySignal::Execute {
                            strategy_id: self.id.clone(),
                            exchange: self.exchange.clone(),
                            instrument: self.instrument.clone(),
                            parameters: StrategyParameters {
                                quantity: chunk_size,
                                price: if self.total_quantity > Decimal::ZERO {
                                    self.market_state_cache.best_bid
                                } else {
                                    self.market_state_cache.best_ask
                                },
                                side: if self.total_quantity > Decimal::ZERO {
                                    TradeSide::Buy
                                } else {
                                    TradeSide::Sell
                                },
                                strategy_type: "ICEBERG".to_string(),
                                custom_params: {
                                    let mut params = HashMap::new();
                                    params.insert(
                                        "chunk_number".to_string(),
                                        serde_json::json!(self.active_orders.len() + 1),
                                    );
                                    params.insert(
                                        "adaptive_sizing".to_string(),
                                        serde_json::json!(self.adaptive_chunk_sizing),
                                    );
                                    params
                                },
                            },
                            urgency: SignalUrgency::Low, // Iceberg orders are typically passive
                            timestamp: current_timestamp_ms(),
                        };

                        signals.push(signal);
                        self.remaining_quantity -= chunk_size;
                        self.active_orders
                            .push(format!("chunk_{}", self.active_orders.len()));
                        self.metrics.signals_generated += 1;

                        debug!(
                            strategy_id = %self.id,
                            chunk_size = %chunk_size,
                            active_orders = self.active_orders.len(),
                            remaining = %self.remaining_quantity,
                            "Generated iceberg chunk signal"
                        );
                    }
                }
            }
            MarketEvent::Trade {
                price,
                volume,
                side,
                ..
            } => {
                let trade_side = match side {
                    TradeSide::Buy => TradeSide::Buy,
                    TradeSide::Sell => TradeSide::Sell,
                };
                self.market_state_cache
                    .update_from_trade(*price, *volume, trade_side);
            }
            _ => {}
        }

        self.metrics.evaluation_time_us = start_time.elapsed().as_micros() as u64;
        self.metrics.last_update = Instant::now();

        Ok(signals)
    }

    async fn initialize(&mut self, _context: &StrategyContext) -> Result<(), StrategyError> {
        info!(
            strategy_id = %self.id,
            exchange = ?self.exchange,
            instrument = %self.instrument,
            total_quantity = %self.total_quantity,
            base_chunk_size = %self.base_chunk_size,
            max_concurrent_orders = self.max_concurrent_orders,
            "Initialized sensor iceberg strategy"
        );
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), StrategyError> {
        info!(strategy_id = %self.id, "Shutting down sensor iceberg strategy");
        Ok(())
    }

    fn metrics(&self) -> StrategyMetrics {
        self.metrics.clone()
    }
}

// POV Strategy Implementation
impl SensorPovStrategy {
    pub fn new(
        id: String,
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        target_quantity: Decimal,
        target_participation_rate: f64,
        assessment_interval: Duration,
    ) -> Self {
        Self {
            id,
            exchange,
            instrument,
            target_quantity,
            remaining_quantity: target_quantity,
            target_participation_rate,
            volume_tracker: VolumeTracker::new(Duration::from_secs(300)), // 5-minute window
            assessment_interval,
            last_assessment: None,
            metrics: StrategyMetrics {
                evaluation_time_us: 0,
                events_processed: 0,
                signals_generated: 0,
                errors: 0,
                last_update: Instant::now(),
            },
            market_state_cache: MarketStateCache::new(),
        }
    }

    fn should_assess(&self) -> bool {
        match self.last_assessment {
            None => true,
            Some(last) => last.elapsed() >= self.assessment_interval,
        }
    }
}

#[async_trait]
impl EventDrivenStrategy for SensorPovStrategy {
    fn id(&self) -> &str {
        &self.id
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            exchanges: Some(vec![self.exchange.clone()]),
            instruments: Some(vec![self.instrument.clone()]),
            event_types: vec![MarketEventType::Trade, MarketEventType::VolumeSpike],
            min_volume: Some(Decimal::from_str("10").unwrap()),
            max_latency_ms: Some(25),
        }
    }

    async fn process_event(
        &mut self,
        event: &MarketEvent,
        _context: &StrategyContext,
    ) -> Result<Vec<StrategySignal>, StrategyError> {
        let start_time = Instant::now();
        self.metrics.events_processed += 1;

        let mut signals = Vec::new();

        match event {
            MarketEvent::Trade {
                price,
                volume,
                side,
                ..
            } => {
                let trade_side = match side {
                    TradeSide::Buy => TradeSide::Buy,
                    TradeSide::Sell => TradeSide::Sell,
                };
                self.market_state_cache
                    .update_from_trade(*price, *volume, trade_side);
                self.volume_tracker.add_volume(*volume);

                // Assess participation if interval has passed
                if self.should_assess() && self.remaining_quantity > Decimal::ZERO {
                    let volume_rate = self.volume_tracker.get_volume_rate();
                    let target_volume =
                        volume_rate * Decimal::from_f64(self.target_participation_rate).unwrap();
                    let order_quantity = target_volume.min(self.remaining_quantity);

                    if order_quantity > Decimal::ZERO {
                        let signal = StrategySignal::Execute {
                            strategy_id: self.id.clone(),
                            exchange: self.exchange.clone(),
                            instrument: self.instrument.clone(),
                            parameters: StrategyParameters {
                                quantity: order_quantity,
                                price: Some(*price),
                                side: if self.target_quantity > Decimal::ZERO {
                                    TradeSide::Buy
                                } else {
                                    TradeSide::Sell
                                },
                                strategy_type: "POV".to_string(),
                                custom_params: {
                                    let mut params = HashMap::new();
                                    params.insert(
                                        "volume_rate".to_string(),
                                        serde_json::json!(volume_rate),
                                    );
                                    params.insert(
                                        "participation_rate".to_string(),
                                        serde_json::json!(self.target_participation_rate),
                                    );
                                    params
                                },
                            },
                            urgency: SignalUrgency::Medium,
                            timestamp: current_timestamp_ms(),
                        };

                        signals.push(signal);
                        self.remaining_quantity -= order_quantity;
                        self.last_assessment = Some(Instant::now());
                        self.metrics.signals_generated += 1;

                        debug!(
                            strategy_id = %self.id,
                            order_quantity = %order_quantity,
                            volume_rate = %volume_rate,
                            participation_rate = self.target_participation_rate,
                            remaining = %self.remaining_quantity,
                            "Generated POV order signal"
                        );
                    }
                }
            }
            MarketEvent::VolumeSpike { volume, .. } => {
                // Respond to volume spikes
                self.volume_tracker.add_volume(*volume);

                if self.remaining_quantity > Decimal::ZERO {
                    let spike_order_quantity =
                        *volume * Decimal::from_f64(self.target_participation_rate * 2.0).unwrap();
                    let order_quantity = spike_order_quantity.min(self.remaining_quantity);

                    if order_quantity > Decimal::ZERO {
                        let signal = StrategySignal::Execute {
                            strategy_id: self.id.clone(),
                            exchange: self.exchange.clone(),
                            instrument: self.instrument.clone(),
                            parameters: StrategyParameters {
                                quantity: order_quantity,
                                price: self.market_state_cache.last_price,
                                side: if self.target_quantity > Decimal::ZERO {
                                    TradeSide::Buy
                                } else {
                                    TradeSide::Sell
                                },
                                strategy_type: "POV_SPIKE".to_string(),
                                custom_params: HashMap::new(),
                            },
                            urgency: SignalUrgency::High,
                            timestamp: current_timestamp_ms(),
                        };

                        signals.push(signal);
                        self.remaining_quantity -= order_quantity;
                        self.metrics.signals_generated += 1;
                    }
                }
            }
            _ => {}
        }

        self.metrics.evaluation_time_us = start_time.elapsed().as_micros() as u64;
        self.metrics.last_update = Instant::now();

        Ok(signals)
    }

    async fn initialize(&mut self, _context: &StrategyContext) -> Result<(), StrategyError> {
        info!(
            strategy_id = %self.id,
            exchange = ?self.exchange,
            instrument = %self.instrument,
            target_quantity = %self.target_quantity,
            target_participation_rate = self.target_participation_rate,
            assessment_interval = ?self.assessment_interval,
            "Initialized sensor POV strategy"
        );
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), StrategyError> {
        info!(strategy_id = %self.id, "Shutting down sensor POV strategy");
        Ok(())
    }

    fn metrics(&self) -> StrategyMetrics {
        self.metrics.clone()
    }
}
