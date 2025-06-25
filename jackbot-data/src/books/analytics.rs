use super::{Level, OrderBook};
use chrono::{DateTime, Utc};
use jackbot_instrument::exchange::ExchangeId;
use rust_decimal::{
    Decimal,
    prelude::{FromPrimitive, ToPrimitive},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// Advanced order book analytics engine for depth analysis and market insights
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are part of the analytics engine architecture
pub struct OrderBookAnalytics {
    /// Historical order book snapshots for analysis
    history: VecDeque<OrderBookAnalyticsSnapshot>,
    /// Depth analysis engine
    depth_analyzer: DepthAnalyzer,
    /// Volume analysis engine
    volume_analyzer: VolumeAnalyzer,
    /// Price clustering analyzer
    clustering_analyzer: PriceClusteringAnalyzer,
    /// Liquidity holes detector
    liquidity_holes_detector: LiquidityHolesDetector,
    /// Configuration
    config: AnalyticsConfig,
}

#[derive(Debug, Clone)]
pub struct AnalyticsConfig {
    /// Maximum history size
    pub max_history_size: usize,
    /// Analysis depth levels
    pub analysis_depth: usize,
    /// Minimum significant volume
    pub min_significant_volume: Decimal,
    /// Price clustering threshold
    pub clustering_threshold: Decimal,
    /// Liquidity hole threshold
    pub liquidity_hole_threshold: Decimal,
    /// Time window for analysis
    pub analysis_window: std::time::Duration,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            max_history_size: 1000,
            analysis_depth: 50,
            min_significant_volume: Decimal::from(1000),
            clustering_threshold: Decimal::from_str("0.0001").unwrap(),
            liquidity_hole_threshold: Decimal::from(500),
            analysis_window: std::time::Duration::from_secs(300),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrderBookAnalyticsSnapshot {
    pub timestamp: DateTime<Utc>,
    pub exchange: ExchangeId,
    pub book: OrderBook,
    pub analytics: BookAnalyticsData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookAnalyticsData {
    /// Market depth metrics
    pub depth_metrics: DepthMetrics,
    /// Volume profile analysis
    pub volume_profile: VolumeProfile,
    /// Price level clustering
    pub price_clustering: PriceClustering,
    /// Liquidity distribution
    pub liquidity_distribution: LiquidityDistribution,
    /// Market quality indicators
    pub market_quality: MarketQuality,
    /// Order book pressure indicators
    pub order_book_pressure: OrderBookPressure,
}

/// Market depth analysis
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DepthAnalyzer {
    /// Depth calculations cache
    depth_cache: HashMap<String, DepthMetrics>,
    /// Historical depth data
    depth_history: VecDeque<(DateTime<Utc>, DepthMetrics)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthMetrics {
    /// Total liquidity available at different price levels
    pub liquidity_at_levels: Vec<(usize, Decimal, Decimal)>, // (level, bid_liquidity, ask_liquidity)
    /// Cumulative volume by depth
    pub cumulative_volume: Vec<(Decimal, Decimal)>, // (price_distance, cumulative_volume)
    /// Effective spread at different sizes
    pub effective_spreads: Vec<(Decimal, Decimal)>, // (trade_size, effective_spread)
    /// Market depth imbalance
    pub depth_imbalance: f64, // -1 (sell pressure) to +1 (buy pressure)
    /// Weighted average price by depth
    pub weighted_mid_price: Decimal,
    /// Depth-adjusted volatility
    pub depth_volatility: f64,
    /// Liquidity concentration ratio
    pub liquidity_concentration: f64,
}

/// Volume analysis engine
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are part of the volume analysis architecture
pub struct VolumeAnalyzer {
    /// Volume distribution analysis
    volume_distributions: HashMap<String, VolumeDistribution>,
    /// Volume-weighted metrics
    vw_metrics: VolumeWeightedMetrics,
    /// Volume clustering detection
    volume_clusters: Vec<VolumeCluster>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeProfile {
    /// Volume distribution by price level
    pub price_volume_histogram: Vec<(Decimal, Decimal)>, // (price, volume)
    /// Point of control (highest volume price)
    pub point_of_control: Decimal,
    /// Value area high/low (70% of volume)
    pub value_area_high: Decimal,
    pub value_area_low: Decimal,
    /// Volume weighted average price
    pub vwap: Decimal,
    /// Volume profile skewness
    pub skewness: f64,
    /// Volume concentration score
    pub concentration_score: f64,
}

#[derive(Debug, Clone)]
pub struct VolumeDistribution {
    /// Volume buckets by size
    pub small_orders: Decimal, // < $1k
    pub medium_orders: Decimal, // $1k - $10k
    pub large_orders: Decimal,  // $10k - $100k
    pub whale_orders: Decimal,  // > $100k
    /// Order count by size
    pub order_counts: [u64; 4],
    /// Average order size
    pub average_order_size: Decimal,
}

#[derive(Debug, Clone)]
pub struct VolumeWeightedMetrics {
    /// Volume-weighted bid-ask spread
    pub vw_spread: Decimal,
    /// Volume-weighted price
    pub vw_price: Decimal,
    /// Volume-weighted volatility
    pub vw_volatility: f64,
    /// Volume-weighted depth
    pub vw_depth: Decimal,
}

#[derive(Debug, Clone)]
pub struct VolumeCluster {
    /// Center price of cluster
    pub center_price: Decimal,
    /// Price range of cluster
    pub price_range: (Decimal, Decimal),
    /// Total volume in cluster
    pub total_volume: Decimal,
    /// Number of orders in cluster
    pub order_count: u64,
    /// Cluster strength score
    pub strength_score: f64,
}

/// Price clustering analysis
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are part of the price clustering analysis architecture
pub struct PriceClusteringAnalyzer {
    /// Detected price clusters
    clusters: Vec<PriceCluster>,
    /// Support and resistance levels
    support_resistance: SupportResistanceLevels,
    /// Price magnetism analysis
    price_magnetism: PriceMagnetismAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceClustering {
    /// Major price clusters
    pub major_clusters: Vec<PriceCluster>,
    /// Support levels (significant buying interest)
    pub support_levels: Vec<SupportResistanceLevel>,
    /// Resistance levels (significant selling interest)
    pub resistance_levels: Vec<SupportResistanceLevel>,
    /// Price magnetism zones
    pub magnetism_zones: Vec<MagnetismZone>,
    /// Round number effects
    pub round_number_effects: RoundNumberEffects,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceCluster {
    /// Center price of the cluster
    pub center_price: Decimal,
    /// Price range [min, max]
    pub price_range: (Decimal, Decimal),
    /// Aggregate volume at this cluster
    pub volume: Decimal,
    /// Number of price levels in cluster
    pub level_count: usize,
    /// Cluster density score
    pub density_score: f64,
    /// Time persistence of cluster
    pub persistence_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportResistanceLevel {
    /// Price level
    pub price: Decimal,
    /// Strength of the level (0.0 to 1.0)
    pub strength: f64,
    /// Number of times level has been tested
    pub test_count: u32,
    /// Volume accumulated at this level
    pub accumulated_volume: Decimal,
    /// Time of last test
    pub last_test_time: DateTime<Utc>,
    /// Level type
    pub level_type: LevelType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LevelType {
    Support,
    Resistance,
    Pivot,
}

#[derive(Debug, Clone)]
pub struct SupportResistanceLevels {
    /// Active support levels
    pub support_levels: Vec<SupportResistanceLevel>,
    /// Active resistance levels
    pub resistance_levels: Vec<SupportResistanceLevel>,
    /// Pivot points
    pub pivot_points: Vec<SupportResistanceLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MagnetismZone {
    /// Price range that attracts orders
    pub price_range: (Decimal, Decimal),
    /// Magnetism strength
    pub magnetism_strength: f64,
    /// Direction of magnetism
    pub direction: MagnetismDirection,
    /// Volume attracted to zone
    pub attracted_volume: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MagnetismDirection {
    Bullish, // Attracts buy orders
    Bearish, // Attracts sell orders
    Neutral, // Attracts both
}

#[derive(Debug, Clone)]
pub struct PriceMagnetismAnalysis {
    /// Active magnetism zones
    pub magnetism_zones: Vec<MagnetismZone>,
    /// Round number attraction
    pub round_number_magnetism: HashMap<Decimal, f64>,
    /// Historical price magnetism
    pub historical_magnetism: VecDeque<(DateTime<Utc>, Vec<MagnetismZone>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundNumberEffects {
    /// Effects at round numbers
    pub round_effects: HashMap<String, RoundNumberEffect>, // e.g., "50000", "100000"
    /// Psychological levels
    pub psychological_levels: Vec<Decimal>,
    /// Round number clustering coefficient
    pub clustering_coefficient: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundNumberEffect {
    /// Round number price
    pub price: Decimal,
    /// Volume concentration at this level
    pub volume_concentration: f64,
    /// Price clustering around this level
    pub price_clustering: f64,
    /// Bounce frequency from this level
    pub bounce_frequency: f64,
}

/// Liquidity holes detection
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are part of the liquidity holes detection architecture
pub struct LiquidityHolesDetector {
    /// Detected liquidity holes
    detected_holes: Vec<LiquidityHole>,
    /// Hole analysis parameters
    detection_params: HoleDetectionParams,
    /// Historical hole patterns
    hole_history: VecDeque<(DateTime<Utc>, Vec<LiquidityHole>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityDistribution {
    /// Liquidity density by price level
    pub density_profile: Vec<(Decimal, f64)>, // (price, density)
    /// Identified liquidity holes
    pub liquidity_holes: Vec<LiquidityHole>,
    /// Liquidity concentration areas
    pub concentration_areas: Vec<LiquidityConcentration>,
    /// Total available liquidity
    pub total_liquidity: Decimal,
    /// Liquidity asymmetry ratio
    pub asymmetry_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityHole {
    /// Price range of the hole
    pub price_range: (Decimal, Decimal),
    /// Expected liquidity vs actual liquidity
    pub liquidity_deficit: Decimal,
    /// Hole severity (0.0 to 1.0)
    pub severity: f64,
    /// Side of the book (bid/ask/both)
    pub side: HoleSide,
    /// Potential price impact if hole is hit
    pub price_impact: Decimal,
    /// Time the hole was detected
    pub detection_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HoleSide {
    Bid,
    Ask,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityConcentration {
    /// Price range of concentration
    pub price_range: (Decimal, Decimal),
    /// Concentration factor (multiple of average)
    pub concentration_factor: f64,
    /// Volume concentrated in this area
    pub concentrated_volume: Decimal,
    /// Type of concentration
    pub concentration_type: ConcentrationType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConcentrationType {
    IcebergOrders,     // Hidden large orders
    MarketMaking,      // Consistent liquidity provision
    SupportResistance, // Technical level clustering
    RandomClustering,  // No clear pattern
}

#[derive(Debug, Clone)]
pub struct HoleDetectionParams {
    /// Minimum hole size to detect
    pub min_hole_size: Decimal,
    /// Minimum severity threshold
    pub min_severity: f64,
    /// Lookback window for analysis
    pub lookback_window: std::time::Duration,
    /// Expected liquidity model
    pub expected_liquidity_model: ExpectedLiquidityModel,
}

#[derive(Debug, Clone)]
pub enum ExpectedLiquidityModel {
    /// Linear decay from best bid/ask
    LinearDecay(f64),
    /// Exponential decay model
    ExponentialDecay(f64),
    /// Historical average model
    HistoricalAverage,
    /// Machine learning model
    MLModel(String),
}

/// Market quality indicators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketQuality {
    /// Market efficiency ratio
    pub efficiency_ratio: f64,
    /// Liquidity provision quality
    pub liquidity_quality: f64,
    /// Price discovery efficiency
    pub price_discovery: f64,
    /// Market resilience score
    pub resilience_score: f64,
    /// Information asymmetry indicator
    pub information_asymmetry: f64,
    /// Market maker competition index
    pub competition_index: f64,
}

/// Order book pressure analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookPressure {
    /// Buying pressure score (-1 to +1)
    pub buying_pressure: f64,
    /// Selling pressure score (-1 to +1)
    pub selling_pressure: f64,
    /// Net pressure (buying - selling)
    pub net_pressure: f64,
    /// Pressure momentum
    pub pressure_momentum: f64,
    /// Pressure volatility
    pub pressure_volatility: f64,
    /// Pressure divergence from price
    pub price_pressure_divergence: f64,
}

impl OrderBookAnalytics {
    pub fn new(config: AnalyticsConfig) -> Self {
        Self {
            history: VecDeque::with_capacity(config.max_history_size),
            depth_analyzer: DepthAnalyzer::new(),
            volume_analyzer: VolumeAnalyzer::new(),
            clustering_analyzer: PriceClusteringAnalyzer::new(),
            liquidity_holes_detector: LiquidityHolesDetector::new(),
            config,
        }
    }

    /// Analyze order book and generate comprehensive analytics
    pub fn analyze_order_book(
        &mut self,
        exchange: ExchangeId,
        book: &OrderBook,
    ) -> BookAnalyticsData {
        let timestamp = Utc::now();

        // Perform depth analysis
        let depth_metrics = self.depth_analyzer.analyze_depth(book, &self.config);

        // Perform volume analysis
        let volume_profile = VolumeAnalyzer::analyze_volume(book, &self.config);

        // Perform price clustering analysis
        let price_clustering = PriceClusteringAnalyzer::analyze_clustering(book, &self.config);

        // Perform liquidity distribution analysis
        let liquidity_distribution = self
            .liquidity_holes_detector
            .analyze_liquidity(book, &self.config);

        // Calculate market quality indicators
        let market_quality = Self::calculate_market_quality(book, &depth_metrics, &volume_profile);

        // Calculate order book pressure
        let order_book_pressure = self.calculate_order_book_pressure(book, &depth_metrics);

        let analytics = BookAnalyticsData {
            depth_metrics,
            volume_profile,
            price_clustering,
            liquidity_distribution,
            market_quality,
            order_book_pressure,
        };

        // Store snapshot
        let snapshot = OrderBookAnalyticsSnapshot {
            timestamp,
            exchange,
            book: book.clone(),
            analytics: analytics.clone(),
        };

        if self.history.len() >= self.config.max_history_size {
            self.history.pop_front();
        }
        self.history.push_back(snapshot);

        analytics
    }

    /// Calculate market quality indicators
    fn calculate_market_quality(
        book: &OrderBook,
        depth_metrics: &DepthMetrics,
        volume_profile: &VolumeProfile,
    ) -> MarketQuality {
        // Market efficiency based on spread tightness and depth
        let efficiency_ratio = Self::calculate_efficiency_ratio(book, depth_metrics);

        // Liquidity quality based on depth and consistency
        let liquidity_quality = Self::calculate_liquidity_quality(depth_metrics);

        // Price discovery efficiency
        let price_discovery = Self::calculate_price_discovery_efficiency(volume_profile);

        // Market resilience (ability to absorb large orders)
        let resilience_score = Self::calculate_resilience_score(depth_metrics);

        // Information asymmetry
        let information_asymmetry = Self::calculate_information_asymmetry(book);

        // Market maker competition
        let competition_index = Self::calculate_competition_index(depth_metrics);

        MarketQuality {
            efficiency_ratio,
            liquidity_quality,
            price_discovery,
            resilience_score,
            information_asymmetry,
            competition_index,
        }
    }

    fn calculate_efficiency_ratio(book: &OrderBook, depth_metrics: &DepthMetrics) -> f64 {
        if let (Some(best_bid), Some(best_ask)) =
            (book.bids().levels().first(), book.asks().levels().first())
        {
            let spread = best_ask.price - best_bid.price;
            let mid_price = (best_bid.price + best_ask.price) / Decimal::TWO;
            let relative_spread = (spread / mid_price).to_f64().unwrap_or(0.0);

            // Adjust for depth - tighter spreads with good depth = higher efficiency
            let depth_adjustment = depth_metrics.liquidity_concentration.min(1.0);

            (1.0 / (1.0 + relative_spread * 10000.0)) * depth_adjustment
        } else {
            0.0
        }
    }

    fn calculate_liquidity_quality(depth_metrics: &DepthMetrics) -> f64 {
        // Based on depth consistency and concentration
        let consistency_score = 1.0 - depth_metrics.depth_volatility;
        let concentration_penalty = if depth_metrics.liquidity_concentration > 0.8 {
            0.8 // Penalize over-concentration
        } else {
            depth_metrics.liquidity_concentration
        };

        (consistency_score + concentration_penalty) / 2.0
    }

    fn calculate_price_discovery_efficiency(volume_profile: &VolumeProfile) -> f64 {
        // Efficient price discovery should have balanced volume distribution
        let skewness_penalty = volume_profile.skewness.abs().min(1.0);
        let concentration_bonus = volume_profile.concentration_score;

        (1.0 - skewness_penalty * 0.5 + concentration_bonus * 0.3).min(1.0)
    }

    fn calculate_resilience_score(depth_metrics: &DepthMetrics) -> f64 {
        // Higher resilience = more liquidity at further price levels
        let total_deep_liquidity: f64 = depth_metrics
            .liquidity_at_levels
            .iter()
            .skip(5) // Skip first 5 levels
            .map(|(_, bid_liq, ask_liq)| (bid_liq + ask_liq).to_f64().unwrap_or(0.0))
            .sum();

        (total_deep_liquidity / 1000000.0).min(1.0) // Normalize to millions
    }

    fn calculate_information_asymmetry(book: &OrderBook) -> f64 {
        // Higher asymmetry = larger differences between bid and ask depths
        let bid_depth: Decimal = book.bids().levels().iter().take(10).map(|l| l.amount).sum();
        let ask_depth: Decimal = book.asks().levels().iter().take(10).map(|l| l.amount).sum();

        let total_depth = bid_depth + ask_depth;
        if total_depth > Decimal::ZERO {
            ((bid_depth - ask_depth).abs() / total_depth)
                .to_f64()
                .unwrap_or(0.0)
        } else {
            0.0
        }
    }

    fn calculate_competition_index(depth_metrics: &DepthMetrics) -> f64 {
        // Higher competition = more consistent liquidity across levels
        let level_consistency = 1.0 - depth_metrics.depth_volatility;
        let spread_tightness = 1.0 / (1.0 + depth_metrics.depth_imbalance.abs());

        (level_consistency + spread_tightness) / 2.0
    }

    /// Calculate order book pressure indicators
    fn calculate_order_book_pressure(
        &self,
        book: &OrderBook,
        depth_metrics: &DepthMetrics,
    ) -> OrderBookPressure {
        // Calculate pressure based on volume imbalances and depth
        let buying_pressure = Self::calculate_buying_pressure(book, depth_metrics);
        let selling_pressure = Self::calculate_selling_pressure(book, depth_metrics);
        let net_pressure = buying_pressure - selling_pressure;

        // Calculate momentum from historical data
        let pressure_momentum = self.calculate_pressure_momentum();

        // Calculate pressure volatility
        let pressure_volatility = self.calculate_pressure_volatility();

        // Calculate divergence from price movement
        let price_pressure_divergence = Self::calculate_price_pressure_divergence();

        OrderBookPressure {
            buying_pressure,
            selling_pressure,
            net_pressure,
            pressure_momentum,
            pressure_volatility,
            price_pressure_divergence,
        }
    }

    fn calculate_buying_pressure(book: &OrderBook, depth_metrics: &DepthMetrics) -> f64 {
        let bid_volume: Decimal = book.bids().levels().iter().take(10).map(|l| l.amount).sum();
        let ask_volume: Decimal = book.asks().levels().iter().take(10).map(|l| l.amount).sum();
        let total_volume = bid_volume + ask_volume;

        if total_volume > Decimal::ZERO {
            let basic_pressure = (bid_volume / total_volume).to_f64().unwrap_or(0.5);
            // Adjust for depth imbalance
            let depth_adjustment = if depth_metrics.depth_imbalance > 0.0 {
                1.0 + depth_metrics.depth_imbalance * 0.3
            } else {
                1.0
            };
            (basic_pressure * depth_adjustment).min(1.0)
        } else {
            0.5
        }
    }

    fn calculate_selling_pressure(book: &OrderBook, depth_metrics: &DepthMetrics) -> f64 {
        let bid_volume: Decimal = book.bids().levels().iter().take(10).map(|l| l.amount).sum();
        let ask_volume: Decimal = book.asks().levels().iter().take(10).map(|l| l.amount).sum();
        let total_volume = bid_volume + ask_volume;

        if total_volume > Decimal::ZERO {
            let basic_pressure = (ask_volume / total_volume).to_f64().unwrap_or(0.5);
            // Adjust for depth imbalance
            let depth_adjustment = if depth_metrics.depth_imbalance < 0.0 {
                1.0 + depth_metrics.depth_imbalance.abs() * 0.3
            } else {
                1.0
            };
            (basic_pressure * depth_adjustment).min(1.0)
        } else {
            0.5
        }
    }

    fn calculate_pressure_momentum(&self) -> f64 {
        // Calculate momentum from recent pressure changes
        if self.history.len() < 5 {
            return 0.0;
        }

        let recent_pressures: Vec<f64> = self
            .history
            .iter()
            .rev()
            .take(5)
            .map(|snapshot| snapshot.analytics.order_book_pressure.net_pressure)
            .collect();

        if recent_pressures.len() >= 2 {
            let current = recent_pressures[0];
            let previous = recent_pressures[1];
            current - previous
        } else {
            0.0
        }
    }

    fn calculate_pressure_volatility(&self) -> f64 {
        // Calculate volatility of pressure over recent history
        if self.history.len() < 10 {
            return 0.0;
        }

        let pressures: Vec<f64> = self
            .history
            .iter()
            .rev()
            .take(10)
            .map(|snapshot| snapshot.analytics.order_book_pressure.net_pressure)
            .collect();

        let mean = pressures.iter().sum::<f64>() / pressures.len() as f64;
        let variance =
            pressures.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / pressures.len() as f64;

        variance.sqrt()
    }

    fn calculate_price_pressure_divergence() -> f64 {
        // Placeholder for price-pressure divergence calculation
        // This would compare price movements with pressure changes
        0.0
    }

    /// Get recent analytics history
    pub fn get_analytics_history(&self, lookback_minutes: u64) -> Vec<&OrderBookAnalyticsSnapshot> {
        let cutoff = Utc::now() - chrono::Duration::minutes(lookback_minutes as i64);
        self.history
            .iter()
            .filter(|snapshot| snapshot.timestamp >= cutoff)
            .collect()
    }

    /// Get summary statistics
    pub fn get_summary_statistics(&self) -> AnalyticsSummary {
        if self.history.is_empty() {
            return AnalyticsSummary::default();
        }

        let recent_analytics: Vec<&BookAnalyticsData> = self
            .history
            .iter()
            .rev()
            .take(100)
            .map(|snapshot| &snapshot.analytics)
            .collect();

        let avg_efficiency = recent_analytics
            .iter()
            .map(|a| a.market_quality.efficiency_ratio)
            .sum::<f64>()
            / recent_analytics.len() as f64;

        let avg_liquidity_quality = recent_analytics
            .iter()
            .map(|a| a.market_quality.liquidity_quality)
            .sum::<f64>()
            / recent_analytics.len() as f64;

        let avg_buying_pressure = recent_analytics
            .iter()
            .map(|a| a.order_book_pressure.buying_pressure)
            .sum::<f64>()
            / recent_analytics.len() as f64;

        let total_liquidity_holes = recent_analytics
            .iter()
            .map(|a| a.liquidity_distribution.liquidity_holes.len())
            .sum::<usize>();

        AnalyticsSummary {
            total_snapshots: self.history.len(),
            average_efficiency_ratio: avg_efficiency,
            average_liquidity_quality: avg_liquidity_quality,
            average_buying_pressure: avg_buying_pressure,
            total_liquidity_holes_detected: total_liquidity_holes,
            analysis_window_minutes: self.config.analysis_window.as_secs() / 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalyticsSummary {
    pub total_snapshots: usize,
    pub average_efficiency_ratio: f64,
    pub average_liquidity_quality: f64,
    pub average_buying_pressure: f64,
    pub total_liquidity_holes_detected: usize,
    pub analysis_window_minutes: u64,
}

// Implementation for helper structs
impl DepthAnalyzer {
    fn new() -> Self {
        Self {
            depth_cache: HashMap::new(),
            depth_history: VecDeque::new(),
        }
    }

    fn analyze_depth(&mut self, book: &OrderBook, config: &AnalyticsConfig) -> DepthMetrics {
        let mut liquidity_at_levels = Vec::new();
        let mut cumulative_volume = Vec::new();
        let mut effective_spreads = Vec::new();

        // Analyze liquidity at each depth level
        for level in 1..=config
            .analysis_depth
            .min(book.bids().levels().len())
            .min(book.asks().levels().len())
        {
            let bid_liquidity: Decimal = book
                .bids()
                .levels()
                .iter()
                .take(level)
                .map(|l| l.amount)
                .sum();
            let ask_liquidity: Decimal = book
                .asks()
                .levels()
                .iter()
                .take(level)
                .map(|l| l.amount)
                .sum();

            liquidity_at_levels.push((level, bid_liquidity, ask_liquidity));

            // Calculate cumulative volume and price distance
            if let (Some(best_bid), Some(best_ask)) =
                (book.bids().levels().first(), book.asks().levels().first())
            {
                let mid_price = (best_bid.price + best_ask.price) / Decimal::TWO;
                if let (Some(level_bid), Some(level_ask)) = (
                    book.bids().levels().get(level - 1),
                    book.asks().levels().get(level - 1),
                ) {
                    let bid_distance = (mid_price - level_bid.price).abs();
                    let ask_distance = (level_ask.price - mid_price).abs();
                    cumulative_volume.push((bid_distance, bid_liquidity));
                    cumulative_volume.push((ask_distance, ask_liquidity));
                }
            }
        }

        // Calculate effective spreads for different trade sizes
        let trade_sizes = vec![
            Decimal::from(1000),   // $1k
            Decimal::from(10000),  // $10k
            Decimal::from(100000), // $100k
        ];

        for trade_size in trade_sizes {
            if let Some(spread) = Self::calculate_effective_spread(book, trade_size) {
                effective_spreads.push((trade_size, spread));
            }
        }

        // Calculate depth imbalance
        let depth_imbalance = Self::calculate_depth_imbalance(book);

        // Calculate weighted mid price
        let weighted_mid_price = Self::calculate_weighted_mid_price(book);

        // Calculate depth volatility
        let depth_volatility = self.calculate_depth_volatility();

        // Calculate liquidity concentration
        let liquidity_concentration = Self::calculate_liquidity_concentration(&liquidity_at_levels);

        DepthMetrics {
            liquidity_at_levels,
            cumulative_volume,
            effective_spreads,
            depth_imbalance,
            weighted_mid_price,
            depth_volatility,
            liquidity_concentration,
        }
    }

    fn calculate_effective_spread(book: &OrderBook, trade_size: Decimal) -> Option<Decimal> {
        // Calculate the effective spread for a given trade size
        let mut remaining_size = trade_size;
        let mut weighted_price = Decimal::ZERO;
        let mut total_volume = Decimal::ZERO;

        // Simulate buying (consuming ask side)
        for level in book.asks().levels() {
            if remaining_size <= Decimal::ZERO {
                break;
            }

            let volume_to_take = remaining_size.min(level.amount);
            weighted_price += level.price * volume_to_take;
            total_volume += volume_to_take;
            remaining_size -= volume_to_take;
        }

        if total_volume > Decimal::ZERO {
            let avg_buy_price = weighted_price / total_volume;

            // Simulate selling (consuming bid side)
            remaining_size = trade_size;
            weighted_price = Decimal::ZERO;
            total_volume = Decimal::ZERO;

            for level in book.bids().levels() {
                if remaining_size <= Decimal::ZERO {
                    break;
                }

                let volume_to_take = remaining_size.min(level.amount);
                weighted_price += level.price * volume_to_take;
                total_volume += volume_to_take;
                remaining_size -= volume_to_take;
            }

            if total_volume > Decimal::ZERO {
                let avg_sell_price = weighted_price / total_volume;
                Some(avg_buy_price - avg_sell_price)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn calculate_depth_imbalance(book: &OrderBook) -> f64 {
        let bid_volume: Decimal = book.bids().levels().iter().take(10).map(|l| l.amount).sum();
        let ask_volume: Decimal = book.asks().levels().iter().take(10).map(|l| l.amount).sum();
        let total_volume = bid_volume + ask_volume;

        if total_volume > Decimal::ZERO {
            ((bid_volume - ask_volume) / total_volume)
                .to_f64()
                .unwrap_or(0.0)
        } else {
            0.0
        }
    }

    fn calculate_weighted_mid_price(book: &OrderBook) -> Decimal {
        if let (Some(best_bid), Some(best_ask)) =
            (book.bids().levels().first(), book.asks().levels().first())
        {
            let bid_weight = best_ask.amount;
            let ask_weight = best_bid.amount;
            let total_weight = bid_weight + ask_weight;

            if total_weight > Decimal::ZERO {
                ((best_bid.price * bid_weight) + (best_ask.price * ask_weight)) / total_weight
            } else {
                (best_bid.price + best_ask.price) / Decimal::TWO
            }
        } else {
            Decimal::ZERO
        }
    }

    fn calculate_depth_volatility(&self) -> f64 {
        // Calculate volatility from historical depth measurements
        if self.depth_history.len() < 5 {
            return 0.0;
        }

        let imbalances: Vec<f64> = self
            .depth_history
            .iter()
            .map(|(_, metrics)| metrics.depth_imbalance)
            .collect();

        let mean = imbalances.iter().sum::<f64>() / imbalances.len() as f64;
        let variance =
            imbalances.iter().map(|i| (i - mean).powi(2)).sum::<f64>() / imbalances.len() as f64;

        variance.sqrt()
    }

    fn calculate_liquidity_concentration(liquidity_at_levels: &[(usize, Decimal, Decimal)]) -> f64 {
        if liquidity_at_levels.len() < 5 {
            return 0.0;
        }

        // Calculate how much of total liquidity is in first 5 levels vs deeper levels
        let first_5_liquidity: Decimal = liquidity_at_levels
            .iter()
            .take(5)
            .map(|(_, bid, ask)| bid + ask)
            .sum();

        let total_liquidity: Decimal = liquidity_at_levels
            .iter()
            .map(|(_, bid, ask)| bid + ask)
            .sum();

        if total_liquidity > Decimal::ZERO {
            (first_5_liquidity / total_liquidity)
                .to_f64()
                .unwrap_or(0.0)
        } else {
            0.0
        }
    }
}

impl VolumeAnalyzer {
    fn new() -> Self {
        Self {
            volume_distributions: HashMap::new(),
            vw_metrics: VolumeWeightedMetrics {
                vw_spread: Decimal::ZERO,
                vw_price: Decimal::ZERO,
                vw_volatility: 0.0,
                vw_depth: Decimal::ZERO,
            },
            volume_clusters: Vec::new(),
        }
    }

    fn analyze_volume(book: &OrderBook, _config: &AnalyticsConfig) -> VolumeProfile {
        // Build price-volume histogram
        let mut price_volume_histogram = Vec::new();
        let mut total_volume = Decimal::ZERO;
        let mut volume_weighted_price = Decimal::ZERO;

        // Aggregate bid side
        for level in book.bids().levels() {
            price_volume_histogram.push((level.price, level.amount));
            total_volume += level.amount;
            volume_weighted_price += level.price * level.amount;
        }

        // Aggregate ask side
        for level in book.asks().levels() {
            price_volume_histogram.push((level.price, level.amount));
            total_volume += level.amount;
            volume_weighted_price += level.price * level.amount;
        }

        // Sort by price
        price_volume_histogram.sort_by(|a, b| a.0.cmp(&b.0));

        // Find point of control (highest volume price)
        let point_of_control = price_volume_histogram
            .iter()
            .max_by(|a, b| a.1.cmp(&b.1))
            .map(|(price, _)| *price)
            .unwrap_or(Decimal::ZERO);

        // Calculate VWAP
        let vwap = if total_volume > Decimal::ZERO {
            volume_weighted_price / total_volume
        } else {
            Decimal::ZERO
        };

        // Calculate value area (70% of volume)
        let (value_area_low, value_area_high) =
            Self::calculate_value_area(&price_volume_histogram, total_volume);

        // Calculate skewness
        let skewness = Self::calculate_volume_skewness(&price_volume_histogram, vwap);

        // Calculate concentration score
        let concentration_score = Self::calculate_volume_concentration(&price_volume_histogram);

        VolumeProfile {
            price_volume_histogram,
            point_of_control,
            value_area_high,
            value_area_low,
            vwap,
            skewness,
            concentration_score,
        }
    }

    fn calculate_value_area(
        histogram: &[(Decimal, Decimal)],
        total_volume: Decimal,
    ) -> (Decimal, Decimal) {
        if histogram.is_empty() {
            return (Decimal::ZERO, Decimal::ZERO);
        }

        let target_volume = total_volume * Decimal::from_str("0.7").unwrap(); // 70%
        let mut accumulated_volume = Decimal::ZERO;
        let start_index = 0;
        let mut end_index = histogram.len() - 1;

        // Find the range that contains 70% of volume around the POC
        for (i, (_, volume)) in histogram.iter().enumerate() {
            accumulated_volume += *volume;
            if accumulated_volume >= target_volume {
                end_index = i;
                break;
            }
        }

        (histogram[start_index].0, histogram[end_index].0)
    }

    fn calculate_volume_skewness(histogram: &[(Decimal, Decimal)], vwap: Decimal) -> f64 {
        let total_volume: Decimal = histogram.iter().map(|(_, vol)| *vol).sum();

        if total_volume == Decimal::ZERO {
            return 0.0;
        }

        let mut skewness_sum = 0.0;
        for (price, volume) in histogram {
            let weight = (*volume / total_volume).to_f64().unwrap_or(0.0);
            let price_diff = (*price - vwap).to_f64().unwrap_or(0.0);
            skewness_sum += weight * price_diff.powi(3);
        }

        skewness_sum
    }

    fn calculate_volume_concentration(histogram: &[(Decimal, Decimal)]) -> f64 {
        if histogram.len() < 5 {
            return 0.0;
        }

        // Calculate concentration using Gini coefficient
        let mut sorted_volumes: Vec<Decimal> = histogram.iter().map(|(_, vol)| *vol).collect();
        sorted_volumes.sort();

        let n = sorted_volumes.len() as f64;
        let total_volume: Decimal = sorted_volumes.iter().sum();

        if total_volume == Decimal::ZERO {
            return 0.0;
        }

        let mut gini_sum = 0.0;
        for (i, volume) in sorted_volumes.iter().enumerate() {
            let rank = (i + 1) as f64;
            let volume_f64 = volume.to_f64().unwrap_or(0.0);
            gini_sum += rank * volume_f64;
        }

        let total_volume_f64 = total_volume.to_f64().unwrap_or(0.0);
        let gini = (2.0 * gini_sum) / (n * total_volume_f64) - (n + 1.0) / n;

        gini.clamp(0.0, 1.0)
    }
}

impl PriceClusteringAnalyzer {
    fn new() -> Self {
        Self {
            clusters: Vec::new(),
            support_resistance: SupportResistanceLevels {
                support_levels: Vec::new(),
                resistance_levels: Vec::new(),
                pivot_points: Vec::new(),
            },
            price_magnetism: PriceMagnetismAnalysis {
                magnetism_zones: Vec::new(),
                round_number_magnetism: HashMap::new(),
                historical_magnetism: VecDeque::new(),
            },
        }
    }

    fn analyze_clustering(book: &OrderBook, config: &AnalyticsConfig) -> PriceClustering {
        // Detect price clusters
        let major_clusters = Self::detect_price_clusters(book, config);

        // Identify support and resistance levels
        let (support_levels, resistance_levels) = Self::identify_support_resistance(book);

        // Analyze price magnetism
        let magnetism_zones = Self::analyze_price_magnetism(book);

        // Analyze round number effects
        let round_number_effects = Self::analyze_round_number_effects(book);

        PriceClustering {
            major_clusters,
            support_levels,
            resistance_levels,
            magnetism_zones,
            round_number_effects,
        }
    }

    fn detect_price_clusters(book: &OrderBook, config: &AnalyticsConfig) -> Vec<PriceCluster> {
        let mut clusters = Vec::new();
        let all_levels: Vec<&Level> = book
            .bids()
            .levels()
            .iter()
            .chain(book.asks().levels().iter())
            .collect();

        if all_levels.is_empty() {
            return clusters;
        }

        // Sort levels by price
        let mut sorted_levels = all_levels.clone();
        sorted_levels.sort_by(|a, b| a.price.cmp(&b.price));

        let mut current_cluster_levels = Vec::new();
        let mut current_cluster_volume = Decimal::ZERO;

        for level in sorted_levels.iter() {
            if current_cluster_levels.is_empty() {
                current_cluster_levels.push(*level);
                current_cluster_volume = level.amount;
            } else {
                let last_price = current_cluster_levels.last().unwrap().price;
                let price_diff = level.price - last_price;

                if price_diff <= config.clustering_threshold {
                    // Add to current cluster
                    current_cluster_levels.push(*level);
                    current_cluster_volume += level.amount;
                } else {
                    // Finalize current cluster if it's significant
                    if current_cluster_levels.len() >= 2
                        && current_cluster_volume >= config.min_significant_volume
                    {
                        let cluster = Self::create_price_cluster(
                            current_cluster_levels.clone(),
                            current_cluster_volume,
                        );
                        clusters.push(cluster);
                    }

                    // Start new cluster
                    current_cluster_levels = vec![*level];
                    current_cluster_volume = level.amount;
                }
            }
        }

        // Handle last cluster
        if current_cluster_levels.len() >= 2
            && current_cluster_volume >= config.min_significant_volume
        {
            let cluster =
                Self::create_price_cluster(current_cluster_levels, current_cluster_volume);
            clusters.push(cluster);
        }

        clusters
    }

    fn create_price_cluster(levels: Vec<&Level>, total_volume: Decimal) -> PriceCluster {
        let min_price = levels.iter().map(|l| l.price).min().unwrap();
        let max_price = levels.iter().map(|l| l.price).max().unwrap();
        let center_price = (min_price + max_price) / Decimal::TWO;

        // Calculate density score
        let price_range = max_price - min_price;
        let density_score = if price_range > Decimal::ZERO {
            (total_volume / price_range).to_f64().unwrap_or(0.0)
        } else {
            total_volume.to_f64().unwrap_or(0.0)
        };

        PriceCluster {
            center_price,
            price_range: (min_price, max_price),
            volume: total_volume,
            level_count: levels.len(),
            density_score: density_score.min(1000.0), // Cap at reasonable value
            persistence_score: 1.0,                   // Would be calculated from historical data
        }
    }

    fn identify_support_resistance(
        book: &OrderBook,
    ) -> (Vec<SupportResistanceLevel>, Vec<SupportResistanceLevel>) {
        let mut support_levels = Vec::new();
        let mut resistance_levels = Vec::new();

        // Identify potential support levels from bid side
        for (i, level) in book.bids().levels().iter().enumerate() {
            if level.amount > Decimal::from(1000) {
                // Significant volume threshold
                let strength = Self::calculate_level_strength(level, i, true);
                if strength > 0.3 {
                    support_levels.push(SupportResistanceLevel {
                        price: level.price,
                        strength,
                        test_count: 1, // Would track from historical data
                        accumulated_volume: level.amount,
                        last_test_time: Utc::now(),
                        level_type: LevelType::Support,
                    });
                }
            }
        }

        // Identify potential resistance levels from ask side
        for (i, level) in book.asks().levels().iter().enumerate() {
            if level.amount > Decimal::from(1000) {
                // Significant volume threshold
                let strength = Self::calculate_level_strength(level, i, false);
                if strength > 0.3 {
                    resistance_levels.push(SupportResistanceLevel {
                        price: level.price,
                        strength,
                        test_count: 1, // Would track from historical data
                        accumulated_volume: level.amount,
                        last_test_time: Utc::now(),
                        level_type: LevelType::Resistance,
                    });
                }
            }
        }

        (support_levels, resistance_levels)
    }

    fn calculate_level_strength(level: &Level, position: usize, _is_bid: bool) -> f64 {
        // Calculate strength based on volume and position in book
        let volume_factor = (level.amount.to_f64().unwrap_or(0.0) / 10000.0).min(1.0);
        let position_factor = 1.0 / (1.0 + position as f64 * 0.1);

        volume_factor * position_factor
    }

    fn analyze_price_magnetism(_book: &OrderBook) -> Vec<MagnetismZone> {
        // Placeholder for magnetism analysis
        Vec::new()
    }

    fn analyze_round_number_effects(book: &OrderBook) -> RoundNumberEffects {
        let mut round_effects = HashMap::new();
        let mut psychological_levels = Vec::new();

        // Check for clustering around round numbers
        let all_prices: Vec<Decimal> = book
            .bids()
            .levels()
            .iter()
            .chain(book.asks().levels().iter())
            .map(|l| l.price)
            .collect();

        // Identify round numbers in the price range
        if let (Some(&min_price), Some(&max_price)) =
            (all_prices.iter().min(), all_prices.iter().max())
        {
            let min_rounded = (min_price.to_f64().unwrap_or(0.0) / 1000.0).floor() * 1000.0;
            let max_rounded = (max_price.to_f64().unwrap_or(0.0) / 1000.0).ceil() * 1000.0;

            let mut current = min_rounded;
            while current <= max_rounded {
                let round_price = Decimal::from_f64(current).unwrap_or(Decimal::ZERO);
                psychological_levels.push(round_price);

                // Calculate effects at this round number
                let effect = Self::calculate_round_number_effect(round_price, &all_prices);
                round_effects.insert(current.to_string(), effect);

                current += 1000.0; // Check every $1000
            }
        }

        RoundNumberEffects {
            round_effects,
            psychological_levels,
            clustering_coefficient: Self::calculate_clustering_coefficient(&all_prices),
        }
    }

    fn calculate_round_number_effect(
        round_price: Decimal,
        all_prices: &[Decimal],
    ) -> RoundNumberEffect {
        let tolerance = Decimal::from_str("10.0").unwrap(); // $10 tolerance

        let nearby_prices: Vec<&Decimal> = all_prices
            .iter()
            .filter(|&&price| (price - round_price).abs() <= tolerance)
            .collect();

        let volume_concentration = nearby_prices.len() as f64 / all_prices.len() as f64;

        RoundNumberEffect {
            price: round_price,
            volume_concentration,
            price_clustering: volume_concentration, // Simplified
            bounce_frequency: 0.0,                  // Would calculate from historical data
        }
    }

    fn calculate_clustering_coefficient(prices: &[Decimal]) -> f64 {
        if prices.len() < 3 {
            return 0.0;
        }

        // Calculate how much prices cluster around round numbers
        let mut clustering_score = 0.0;
        let total_prices = prices.len() as f64;

        for &price in prices {
            let price_f64 = price.to_f64().unwrap_or(0.0);
            let nearest_100 = (price_f64 / 100.0).round() * 100.0;
            let distance_to_round = (price_f64 - nearest_100).abs();

            // Closer to round number = higher clustering
            let clustering_contribution = 1.0 / (1.0 + distance_to_round / 10.0);
            clustering_score += clustering_contribution;
        }

        clustering_score / total_prices
    }
}

impl LiquidityHolesDetector {
    fn new() -> Self {
        Self {
            detected_holes: Vec::new(),
            detection_params: HoleDetectionParams {
                min_hole_size: Decimal::from(100),
                min_severity: 0.3,
                lookback_window: std::time::Duration::from_secs(300),
                expected_liquidity_model: ExpectedLiquidityModel::LinearDecay(0.1),
            },
            hole_history: VecDeque::new(),
        }
    }

    fn analyze_liquidity(
        &mut self,
        book: &OrderBook,
        _config: &AnalyticsConfig,
    ) -> LiquidityDistribution {
        // Build liquidity density profile
        let density_profile = Self::build_density_profile(book);

        // Detect liquidity holes
        let liquidity_holes = self.detect_holes(book);

        // Identify concentration areas
        let concentration_areas = Self::identify_concentration_areas(book);

        // Calculate total liquidity
        let total_liquidity = Self::calculate_total_liquidity(book);

        // Calculate asymmetry ratio
        let asymmetry_ratio = Self::calculate_asymmetry_ratio(book);

        LiquidityDistribution {
            density_profile,
            liquidity_holes,
            concentration_areas,
            total_liquidity,
            asymmetry_ratio,
        }
    }

    fn build_density_profile(book: &OrderBook) -> Vec<(Decimal, f64)> {
        let mut profile = Vec::new();

        // Analyze bid side density
        for level in book.bids().levels() {
            let density = level.amount.to_f64().unwrap_or(0.0);
            profile.push((level.price, density));
        }

        // Analyze ask side density
        for level in book.asks().levels() {
            let density = level.amount.to_f64().unwrap_or(0.0);
            profile.push((level.price, density));
        }

        // Sort by price
        profile.sort_by(|a, b| a.0.cmp(&b.0));
        profile
    }

    fn detect_holes(&self, book: &OrderBook) -> Vec<LiquidityHole> {
        let mut holes = Vec::new();

        // Detect holes in bid side
        holes.extend(self.detect_side_holes(book.bids().levels(), true));

        // Detect holes in ask side
        holes.extend(self.detect_side_holes(book.asks().levels(), false));

        holes
    }

    fn detect_side_holes(&self, levels: &[Level], is_bid_side: bool) -> Vec<LiquidityHole> {
        let mut holes = Vec::new();

        if levels.len() < 3 {
            return holes;
        }

        // Look for gaps in liquidity
        for window in levels.windows(3) {
            let prev_level = &window[0];
            let current_level = &window[1];
            let next_level = &window[2];

            // Calculate expected liquidity based on surrounding levels
            let expected_liquidity = (prev_level.amount + next_level.amount) / Decimal::TWO;
            let actual_liquidity = current_level.amount;

            if actual_liquidity < expected_liquidity / Decimal::TWO {
                let deficit = expected_liquidity - actual_liquidity;
                let severity = (deficit / expected_liquidity)
                    .to_f64()
                    .unwrap_or(0.0)
                    .min(1.0);

                if severity >= self.detection_params.min_severity {
                    holes.push(LiquidityHole {
                        price_range: (prev_level.price, next_level.price),
                        liquidity_deficit: deficit,
                        severity,
                        side: if is_bid_side {
                            HoleSide::Bid
                        } else {
                            HoleSide::Ask
                        },
                        price_impact: Self::estimate_price_impact(deficit),
                        detection_time: Utc::now(),
                    });
                }
            }
        }

        holes
    }

    fn estimate_price_impact(liquidity_deficit: Decimal) -> Decimal {
        // Simple price impact estimation
        liquidity_deficit / Decimal::from(10000) // 1 bps per $10k deficit
    }

    fn identify_concentration_areas(book: &OrderBook) -> Vec<LiquidityConcentration> {
        let mut concentrations = Vec::new();

        // Analyze bid side concentrations
        concentrations.extend(Self::find_concentrations(book.bids().levels(), true));

        // Analyze ask side concentrations
        concentrations.extend(Self::find_concentrations(book.asks().levels(), false));

        concentrations
    }

    fn find_concentrations(levels: &[Level], _is_bid_side: bool) -> Vec<LiquidityConcentration> {
        let mut concentrations = Vec::new();

        if levels.is_empty() {
            return concentrations;
        }

        let average_liquidity: Decimal =
            levels.iter().map(|l| l.amount).sum::<Decimal>() / Decimal::from(levels.len());

        // Find areas with significantly higher than average liquidity
        for window in levels.windows(3) {
            let window_liquidity: Decimal = window.iter().map(|l| l.amount).sum();
            let window_average = window_liquidity / Decimal::from(3);

            if window_average > average_liquidity * Decimal::from(2) {
                let concentration_factor =
                    (window_average / average_liquidity).to_f64().unwrap_or(1.0);

                concentrations.push(LiquidityConcentration {
                    price_range: (window[0].price, window[2].price),
                    concentration_factor,
                    concentrated_volume: window_liquidity,
                    concentration_type: ConcentrationType::MarketMaking, // Simplified classification
                });
            }
        }

        concentrations
    }

    fn calculate_total_liquidity(book: &OrderBook) -> Decimal {
        let bid_liquidity: Decimal = book.bids().levels().iter().map(|l| l.amount).sum();
        let ask_liquidity: Decimal = book.asks().levels().iter().map(|l| l.amount).sum();
        bid_liquidity + ask_liquidity
    }

    fn calculate_asymmetry_ratio(book: &OrderBook) -> f64 {
        let bid_liquidity: Decimal = book.bids().levels().iter().map(|l| l.amount).sum();
        let ask_liquidity: Decimal = book.asks().levels().iter().map(|l| l.amount).sum();
        let total_liquidity = bid_liquidity + ask_liquidity;

        if total_liquidity > Decimal::ZERO {
            ((bid_liquidity - ask_liquidity) / total_liquidity)
                .to_f64()
                .unwrap_or(0.0)
        } else {
            0.0
        }
    }
}

use rust_decimal::prelude::FromStr;
