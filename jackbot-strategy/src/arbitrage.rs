use crate::{Strategy, StrategyConfig};
use jackbot_data::books::aggregator::{ArbitrageOpportunity, OrderBookAggregator};
use jackbot_instrument::{exchange::ExchangeId, instrument::InstrumentIndex};
use jackbot_risk::position_tracker::PositionTracker;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

/// Simple performance metrics for arbitrage strategies.
#[derive(Debug, Clone)]
pub struct ArbitrageMetrics {
    /// Number of opportunities detected.
    pub opportunities_detected: usize,
    /// Number of opportunities executed.
    pub opportunities_executed: usize,
    /// Sum of spreads from all detected opportunities.
    pub cumulative_spread: Decimal,
}

impl Default for ArbitrageMetrics {
    fn default() -> Self {
        Self {
            opportunities_detected: 0,
            opportunities_executed: 0,
            cumulative_spread: Decimal::ZERO,
        }
    }
}

impl ArbitrageMetrics {
    fn record(&mut self, opp: &ArbitrageOpportunity, executed: bool) {
        self.opportunities_detected += 1;
        self.cumulative_spread += opp.spread;
        if executed {
            self.opportunities_executed += 1;
        }
    }
}

/// Basic cross-exchange arbitrage strategy using an [`OrderBookAggregator`].
#[derive(Debug)]
pub struct ArbitrageStrategy {
    /// Aggregated order books across exchanges.
    pub aggregator: OrderBookAggregator,
    /// Tracks current positions per instrument.
    pub position_tracker: PositionTracker<InstrumentIndex>,
    /// Minimum spread required to trigger execution.
    pub threshold: Decimal,
    /// Collected performance metrics.
    pub metrics: ArbitrageMetrics,
}

impl ArbitrageStrategy {
    /// Create a new strategy with the given aggregator.
    pub fn new(aggregator: OrderBookAggregator) -> Self {
        Self {
            aggregator,
            position_tracker: PositionTracker::new(),
            threshold: Decimal::ZERO,
            metrics: ArbitrageMetrics::default(),
        }
    }
}

impl Strategy<()> for ArbitrageStrategy {
    fn on_start(&mut self, config: &StrategyConfig) {
        if let Some(t) = config.get("threshold") {
            if let Some(d) = Decimal::from_f64(t) {
                self.threshold = d;
            }
        }
    }

    fn on_event(&mut self, _event: &()) {
        if let Some(opp) = self.aggregator.monitor_and_detect(self.threshold) {
            // In a real implementation we would send orders here. For this
            // example we simply record the opportunity as executed.
            self.position_tracker.update(opp.buy_exchange, InstrumentIndex(0), Decimal::ONE);
            self.position_tracker.update(opp.sell_exchange, InstrumentIndex(0), Decimal::NEG_ONE);
            self.metrics.record(&opp, true);
        }
    }
}
