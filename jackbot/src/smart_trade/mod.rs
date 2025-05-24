use rust_decimal::Decimal;

pub mod trailing_take_profit;
pub mod profit_target;
pub mod trailing_stop;
pub mod multi_level_stop;
pub mod multi_level_take_profit;


pub use trailing_take_profit::TrailingTakeProfit;
pub use profit_target::ProfitTarget;
pub use trailing_stop::TrailingStop;
pub use multi_level_stop::MultiLevelStop;
pub use multi_level_take_profit::MultiLevelTakeProfit;

/// Unified interface for smart trade strategies such as trailing take profit or
/// multi-level stop loss. Implementations evaluate incoming prices and
/// optionally emit a [`SmartTradeSignal`].
pub trait SmartTradeStrategy {
    /// Process a new price tick and return a signal if the strategy conditions
    /// are met.
    fn evaluate(&mut self, price: Decimal) -> Option<SmartTradeSignal>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum SmartTradeSignal {
    TakeProfit(Decimal),
    StopLoss(Decimal),
    StopLevel(usize, Decimal),
}
