use super::OrderBook;
use chrono::{DateTime, Utc};
use jackbot_instrument::exchange::ExchangeId;
use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// Advanced market microstructure analysis for high-frequency trading sensors
#[derive(Debug, Clone)]
pub struct MarketMicrostructureAnalyzer {
    /// Historical order book snapshots for analysis
    history: VecDeque<OrderBookSnapshot>,
    /// Trade flow analysis
    trade_flow: TradeFlowAnalyzer,
    /// Price impact modeling
    price_impact: PriceImpactAnalyzer,
    /// Market maker behavior tracking
    market_maker_behavior: MarketMakerBehaviorAnalyzer,
    /// Configuration parameters
    config: MicrostructureConfig,
}

#[derive(Debug, Clone)]
pub struct OrderBookSnapshot {
    pub timestamp: DateTime<Utc>,
    pub exchange: ExchangeId,
    pub book: OrderBook,
    pub trade_volume: Decimal,
    pub price_movement: Decimal,
}

#[derive(Debug, Clone)]
pub struct MicrostructureConfig {
    /// Maximum history size for analysis
    pub max_history_size: usize,
    /// Minimum time between snapshots (milliseconds)
    pub min_snapshot_interval_ms: u64,
    /// Depth levels to analyze
    pub analysis_depth: usize,
    /// Minimum volume for significant trades
    pub min_significant_volume: Decimal,
    /// Price impact threshold
    pub price_impact_threshold: Decimal,
}

impl Default for MicrostructureConfig {
    fn default() -> Self {
        Self {
            max_history_size: 1000,
            min_snapshot_interval_ms: 100,
            analysis_depth: 20,
            min_significant_volume: Decimal::from(1000),
            price_impact_threshold: Decimal::from_str("0.001").unwrap(),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are part of the trade flow analysis architecture
pub struct TradeFlowAnalyzer {
    /// Recent trade sizes and their frequency
    trade_size_distribution: HashMap<TradeSize, u64>,
    /// Order flow imbalance over time
    order_flow_imbalance: VecDeque<OrderFlowImbalance>,
    /// Aggressive vs passive trade ratio
    aggressive_ratio: f64,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TradeSize {
    Small,  // < 1000 USD
    Medium, // 1000-10000 USD
    Large,  // 10000-100000 USD
    Whale,  // > 100000 USD
}

#[derive(Debug, Clone)]
pub struct OrderFlowImbalance {
    pub timestamp: DateTime<Utc>,
    pub buy_volume: Decimal,
    pub sell_volume: Decimal,
    pub imbalance_ratio: f64,
    pub urgency_score: f64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are part of the price impact analysis architecture
pub struct PriceImpactAnalyzer {
    /// Historical price impacts by trade size
    impact_by_size: HashMap<TradeSize, VecDeque<Decimal>>,
    /// Liquidity depth analysis
    liquidity_depth: LiquidityDepthAnalysis,
    /// Temporary price impact decay
    temporary_impact_decay: f64,
    /// Permanent price impact
    permanent_impact: f64,
}

#[derive(Debug, Clone)]
pub struct LiquidityDepthAnalysis {
    /// Available liquidity at different price levels
    pub bid_depth: Vec<(Decimal, Decimal)>, // (price, cumulative_volume)
    pub ask_depth: Vec<(Decimal, Decimal)>,
    /// Liquidity holes detection
    pub liquidity_holes: Vec<LiquidityHole>,
    /// Average spread at different depths
    pub depth_spreads: Vec<(usize, Decimal)>,
}

#[derive(Debug, Clone)]
pub struct LiquidityHole {
    pub side: String, // "bid" or "ask"
    pub price_start: Decimal,
    pub price_end: Decimal,
    pub missing_volume: Decimal,
    pub severity: f64, // 0.0 to 1.0
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are part of the market maker behavior analysis architecture
pub struct MarketMakerBehaviorAnalyzer {
    /// Spread patterns over time
    spread_patterns: VecDeque<SpreadPattern>,
    /// Inventory management detection
    inventory_signals: VecDeque<InventorySignal>,
    /// Market maker identification
    market_maker_profiles: HashMap<String, MarketMakerProfile>,
}

#[derive(Debug, Clone)]
pub struct SpreadPattern {
    pub timestamp: DateTime<Utc>,
    pub spread: Decimal,
    pub bid_depth: Decimal,
    pub ask_depth: Decimal,
    pub pattern_type: SpreadPatternType,
}

#[derive(Debug, Clone)]
pub enum SpreadPatternType {
    Normal,
    WideSpreads,  // Potential low liquidity
    TightSpreads, // High competition
    Asymmetric,   // Directional bias
    Oscillating,  // Uncertain market
}

#[derive(Debug, Clone)]
pub struct InventorySignal {
    pub timestamp: DateTime<Utc>,
    pub side_bias: f64, // -1.0 (sell bias) to +1.0 (buy bias)
    pub confidence: f64,
    pub volume_pressure: Decimal,
}

#[derive(Debug, Clone)]
pub struct MarketMakerProfile {
    pub identifier: String,
    pub typical_spread: Decimal,
    pub average_depth: Decimal,
    pub response_time_ms: u64,
    pub inventory_management_style: InventoryStyle,
}

#[derive(Debug, Clone)]
pub enum InventoryStyle {
    Passive,     // Slowly adjusts positions
    Aggressive,  // Quickly adjusts positions
    Balanced,    // Maintains neutral inventory
    Directional, // Takes directional bets
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrostructureMetrics {
    /// Current order flow imbalance
    pub order_flow_imbalance: f64,
    /// Average trade size (USD)
    pub average_trade_size: Decimal,
    /// Price impact per $1M traded
    pub price_impact_per_million: Decimal,
    /// Market maker competition index
    pub market_maker_competition: f64,
    /// Liquidity depth score
    pub liquidity_depth_score: f64,
    /// Spread stability index
    pub spread_stability: f64,
    /// Urgency score for current market conditions
    pub urgency_score: f64,
    /// Predicted short-term price movement
    pub predicted_price_movement: f64,
}

impl Default for MarketMicrostructureAnalyzer {
    fn default() -> Self {
        Self::new(MicrostructureConfig::default())
    }
}

impl MarketMicrostructureAnalyzer {
    pub fn new(config: MicrostructureConfig) -> Self {
        Self {
            history: VecDeque::with_capacity(config.max_history_size),
            trade_flow: TradeFlowAnalyzer::new(),
            price_impact: PriceImpactAnalyzer::new(),
            market_maker_behavior: MarketMakerBehaviorAnalyzer::new(),
            config,
        }
    }

    /// Process a new order book snapshot for microstructure analysis
    pub fn process_snapshot(&mut self, snapshot: OrderBookSnapshot) {
        // Add to history with size limit
        if self.history.len() >= self.config.max_history_size {
            self.history.pop_front();
        }
        self.history.push_back(snapshot.clone());

        // Update various analyzers
        self.analyze_order_flow(&snapshot);
        self.analyze_price_impact(&snapshot);
        self.analyze_market_maker_behavior(&snapshot);
    }

    /// Analyze order flow imbalance
    fn analyze_order_flow(&mut self, snapshot: &OrderBookSnapshot) {
        let book = &snapshot.book;

        // Calculate order flow imbalance
        let bid_volume: Decimal = book
            .bids()
            .levels()
            .iter()
            .take(self.config.analysis_depth)
            .map(|level| level.amount)
            .sum();

        let ask_volume: Decimal = book
            .asks()
            .levels()
            .iter()
            .take(self.config.analysis_depth)
            .map(|level| level.amount)
            .sum();

        let total_volume = bid_volume + ask_volume;
        let imbalance_ratio = if total_volume > Decimal::ZERO {
            ((bid_volume - ask_volume) / total_volume)
                .to_f64()
                .unwrap_or(0.0)
        } else {
            0.0
        };

        // Calculate urgency score based on volume and price movement
        let urgency_score = self.calculate_urgency_score(snapshot, imbalance_ratio);

        let flow_imbalance = OrderFlowImbalance {
            timestamp: snapshot.timestamp,
            buy_volume: bid_volume,
            sell_volume: ask_volume,
            imbalance_ratio,
            urgency_score,
        };

        self.trade_flow
            .order_flow_imbalance
            .push_back(flow_imbalance);

        // Keep only recent data
        if self.trade_flow.order_flow_imbalance.len() > 100 {
            self.trade_flow.order_flow_imbalance.pop_front();
        }
    }

    /// Calculate urgency score for trading decisions
    fn calculate_urgency_score(&self, snapshot: &OrderBookSnapshot, imbalance_ratio: f64) -> f64 {
        let mut urgency = 0.0;

        // Factor 1: Order flow imbalance
        urgency += imbalance_ratio.abs() * 0.3;

        // Factor 2: Price movement
        let price_movement_factor = snapshot.price_movement.abs().to_f64().unwrap_or(0.0);
        urgency += price_movement_factor * 0.2;

        // Factor 3: Volume surge
        let volume_factor = if snapshot.trade_volume > self.config.min_significant_volume {
            (snapshot.trade_volume / self.config.min_significant_volume)
                .to_f64()
                .unwrap_or(1.0)
                .min(2.0)
        } else {
            0.0
        };
        urgency += volume_factor * 0.25;

        // Factor 4: Spread compression/expansion
        if let (Some(best_bid), Some(best_ask)) = (
            snapshot.book.bids().levels().first(),
            snapshot.book.asks().levels().first(),
        ) {
            let spread = best_ask.price - best_bid.price;
            let spread_factor = spread.to_f64().unwrap_or(0.0);
            urgency += (1.0 / (1.0 + spread_factor * 1000.0)) * 0.25;
        }

        urgency.min(1.0f64)
    }

    /// Analyze price impact patterns
    fn analyze_price_impact(&mut self, snapshot: &OrderBookSnapshot) {
        let book = &snapshot.book;

        // Calculate liquidity depth
        let mut bid_depth = Vec::new();
        let mut ask_depth = Vec::new();
        let mut cumulative_bid_volume = Decimal::ZERO;
        let mut cumulative_ask_volume = Decimal::ZERO;

        for level in book.bids().levels().iter().take(self.config.analysis_depth) {
            cumulative_bid_volume += level.amount;
            bid_depth.push((level.price, cumulative_bid_volume));
        }

        for level in book.asks().levels().iter().take(self.config.analysis_depth) {
            cumulative_ask_volume += level.amount;
            ask_depth.push((level.price, cumulative_ask_volume));
        }

        // Detect liquidity holes
        let liquidity_holes = Self::detect_liquidity_holes(&bid_depth, &ask_depth);

        // Calculate depth spreads
        let depth_spreads = self.calculate_depth_spreads(book);

        self.price_impact.liquidity_depth = LiquidityDepthAnalysis {
            bid_depth,
            ask_depth,
            liquidity_holes,
            depth_spreads,
        };
    }

    /// Detect liquidity holes in the order book
    fn detect_liquidity_holes(
        bid_depth: &[(Decimal, Decimal)],
        ask_depth: &[(Decimal, Decimal)],
    ) -> Vec<LiquidityHole> {
        let mut holes = Vec::new();

        // Analyze bid side
        for window in bid_depth.windows(2) {
            let (price1, volume1) = window[0];
            let (price2, volume2) = window[1];

            let volume_diff = volume2 - volume1;
            let price_diff = price1 - price2; // Bids are descending

            if volume_diff < Decimal::from(100) && price_diff > Decimal::from_str("0.01").unwrap() {
                holes.push(LiquidityHole {
                    side: "bid".to_string(),
                    price_start: price2,
                    price_end: price1,
                    missing_volume: Decimal::from(100) - volume_diff,
                    severity: (price_diff.to_f64().unwrap_or(0.0) * 100.0).min(1.0),
                });
            }
        }

        // Analyze ask side
        for window in ask_depth.windows(2) {
            let (price1, volume1) = window[0];
            let (price2, volume2) = window[1];

            let volume_diff = volume2 - volume1;
            let price_diff = price2 - price1; // Asks are ascending

            if volume_diff < Decimal::from(100) && price_diff > Decimal::from_str("0.01").unwrap() {
                holes.push(LiquidityHole {
                    side: "ask".to_string(),
                    price_start: price1,
                    price_end: price2,
                    missing_volume: Decimal::from(100) - volume_diff,
                    severity: (price_diff.to_f64().unwrap_or(0.0) * 100.0).min(1.0),
                });
            }
        }

        holes
    }

    /// Calculate spread at different depths
    fn calculate_depth_spreads(&self, book: &OrderBook) -> Vec<(usize, Decimal)> {
        let mut spreads = Vec::new();

        for depth in 1..=self
            .config
            .analysis_depth
            .min(book.bids().levels().len())
            .min(book.asks().levels().len())
        {
            if let (Some(bid), Some(ask)) = (
                book.bids().levels().get(depth - 1),
                book.asks().levels().get(depth - 1),
            ) {
                let spread = ask.price - bid.price;
                spreads.push((depth, spread));
            }
        }

        spreads
    }

    /// Analyze market maker behavior patterns
    fn analyze_market_maker_behavior(&mut self, snapshot: &OrderBookSnapshot) {
        let book = &snapshot.book;

        // Calculate current spread
        if let (Some(best_bid), Some(best_ask)) =
            (book.bids().levels().first(), book.asks().levels().first())
        {
            let spread = best_ask.price - best_bid.price;
            let bid_depth = best_bid.amount;
            let ask_depth = best_ask.amount;

            // Determine spread pattern type
            let pattern_type = Self::classify_spread_pattern(spread, bid_depth, ask_depth);

            let spread_pattern = SpreadPattern {
                timestamp: snapshot.timestamp,
                spread,
                bid_depth,
                ask_depth,
                pattern_type,
            };

            self.market_maker_behavior
                .spread_patterns
                .push_back(spread_pattern);

            // Keep only recent patterns
            if self.market_maker_behavior.spread_patterns.len() > 50 {
                self.market_maker_behavior.spread_patterns.pop_front();
            }

            // Analyze inventory signals
            let inventory_signal = self.detect_inventory_signal(snapshot);
            self.market_maker_behavior
                .inventory_signals
                .push_back(inventory_signal);

            if self.market_maker_behavior.inventory_signals.len() > 50 {
                self.market_maker_behavior.inventory_signals.pop_front();
            }
        }
    }

    /// Classify the current spread pattern
    fn classify_spread_pattern(
        spread: Decimal,
        bid_depth: Decimal,
        ask_depth: Decimal,
    ) -> SpreadPatternType {
        let spread_f64 = spread.to_f64().unwrap_or(0.0);
        let bid_f64 = bid_depth.to_f64().unwrap_or(0.0);
        let ask_f64 = ask_depth.to_f64().unwrap_or(0.0);

        // Historical average spread (simplified - would use actual historical data)
        let avg_spread = 0.001; // Example: 0.1%

        if spread_f64 > avg_spread * 2.0 {
            SpreadPatternType::WideSpreads
        } else if spread_f64 < avg_spread * 0.5 {
            SpreadPatternType::TightSpreads
        } else if (bid_f64 - ask_f64).abs() > (bid_f64 + ask_f64) * 0.3 {
            SpreadPatternType::Asymmetric
        } else {
            SpreadPatternType::Normal
        }
    }

    /// Detect inventory management signals
    fn detect_inventory_signal(&self, snapshot: &OrderBookSnapshot) -> InventorySignal {
        let book = &snapshot.book;

        // Calculate side bias based on depth asymmetry
        let bid_total: Decimal = book.bids().levels().iter().take(5).map(|l| l.amount).sum();

        let ask_total: Decimal = book.asks().levels().iter().take(5).map(|l| l.amount).sum();

        let total = bid_total + ask_total;
        let side_bias = if total > Decimal::ZERO {
            ((bid_total - ask_total) / total).to_f64().unwrap_or(0.0)
        } else {
            0.0
        };

        // Calculate confidence based on volume and consistency
        let confidence = if total > self.config.min_significant_volume {
            (total / self.config.min_significant_volume)
                .to_f64()
                .unwrap_or(1.0)
                .min(1.0)
        } else {
            0.0
        };

        InventorySignal {
            timestamp: snapshot.timestamp,
            side_bias,
            confidence,
            volume_pressure: total,
        }
    }

    /// Generate comprehensive microstructure metrics
    pub fn get_microstructure_metrics(&self) -> MicrostructureMetrics {
        let order_flow_imbalance = self
            .trade_flow
            .order_flow_imbalance
            .back()
            .map(|ofi| ofi.imbalance_ratio)
            .unwrap_or(0.0);

        let urgency_score = self
            .trade_flow
            .order_flow_imbalance
            .back()
            .map(|ofi| ofi.urgency_score)
            .unwrap_or(0.0);

        // Calculate other metrics
        let average_trade_size = Self::calculate_average_trade_size();
        let price_impact_per_million = Self::calculate_price_impact_per_million();
        let market_maker_competition = self.calculate_market_maker_competition();
        let liquidity_depth_score = self.calculate_liquidity_depth_score();
        let spread_stability = self.calculate_spread_stability();
        let predicted_price_movement = self.predict_price_movement();

        MicrostructureMetrics {
            order_flow_imbalance,
            average_trade_size,
            price_impact_per_million,
            market_maker_competition,
            liquidity_depth_score,
            spread_stability,
            urgency_score,
            predicted_price_movement,
        }
    }

    fn calculate_average_trade_size() -> Decimal {
        // Simplified calculation - would use actual trade data
        Decimal::from(10000)
    }

    fn calculate_price_impact_per_million() -> Decimal {
        // Simplified calculation - would use historical price impact data
        Decimal::from_str("0.001").unwrap()
    }

    fn calculate_market_maker_competition(&self) -> f64 {
        // Based on spread tightness and depth
        let recent_spreads: Vec<f64> = self
            .market_maker_behavior
            .spread_patterns
            .iter()
            .take(10)
            .map(|sp| sp.spread.to_f64().unwrap_or(0.0))
            .collect();

        if recent_spreads.is_empty() {
            return 0.5; // Neutral
        }

        let avg_spread = recent_spreads.iter().sum::<f64>() / recent_spreads.len() as f64;
        (1.0 / (1.0 + avg_spread * 1000.0)).min(1.0)
    }

    fn calculate_liquidity_depth_score(&self) -> f64 {
        let total_bid_depth: f64 = self
            .price_impact
            .liquidity_depth
            .bid_depth
            .iter()
            .map(|(_, volume)| volume.to_f64().unwrap_or(0.0))
            .sum();

        let total_ask_depth: f64 = self
            .price_impact
            .liquidity_depth
            .ask_depth
            .iter()
            .map(|(_, volume)| volume.to_f64().unwrap_or(0.0))
            .sum();

        let total_depth = total_bid_depth + total_ask_depth;
        (total_depth / 100000.0).min(1.0f64) // Normalize to 0-1 scale
    }

    fn calculate_spread_stability(&self) -> f64 {
        let spreads: Vec<f64> = self
            .market_maker_behavior
            .spread_patterns
            .iter()
            .map(|sp| sp.spread.to_f64().unwrap_or(0.0))
            .collect();

        if spreads.len() < 2 {
            return 0.5; // Neutral
        }

        // Calculate coefficient of variation
        let mean = spreads.iter().sum::<f64>() / spreads.len() as f64;
        let variance =
            spreads.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / spreads.len() as f64;

        let std_dev = variance.sqrt();
        let cv = if mean > 0.0 { std_dev / mean } else { 0.0 };

        (1.0 / (1.0 + cv)).min(1.0f64)
    }

    fn predict_price_movement(&self) -> f64 {
        // Simple prediction based on order flow imbalance and recent patterns
        let imbalance = self
            .trade_flow
            .order_flow_imbalance
            .back()
            .map(|ofi| ofi.imbalance_ratio)
            .unwrap_or(0.0);

        let inventory_bias = self
            .market_maker_behavior
            .inventory_signals
            .back()
            .map(|is| is.side_bias)
            .unwrap_or(0.0);

        // Combine signals with weights
        let prediction = imbalance * 0.6 + inventory_bias * 0.4;
        prediction.clamp(-1.0f64, 1.0f64)
    }
}

impl Default for TradeFlowAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl TradeFlowAnalyzer {
    pub fn new() -> Self {
        Self {
            trade_size_distribution: HashMap::new(),
            order_flow_imbalance: VecDeque::new(),
            aggressive_ratio: 0.5,
        }
    }
}

impl Default for PriceImpactAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl PriceImpactAnalyzer {
    pub fn new() -> Self {
        Self {
            impact_by_size: HashMap::new(),
            liquidity_depth: LiquidityDepthAnalysis {
                bid_depth: Vec::new(),
                ask_depth: Vec::new(),
                liquidity_holes: Vec::new(),
                depth_spreads: Vec::new(),
            },
            temporary_impact_decay: 0.95,
            permanent_impact: 0.01,
        }
    }
}

impl Default for MarketMakerBehaviorAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl MarketMakerBehaviorAnalyzer {
    pub fn new() -> Self {
        Self {
            spread_patterns: VecDeque::new(),
            inventory_signals: VecDeque::new(),
            market_maker_profiles: HashMap::new(),
        }
    }
}

use rust_decimal::prelude::FromStr;
