use rust_decimal::Decimal;
use crate::smart_trade::{SmartTradeSignal, SmartTradeStrategy};

#[derive(Debug, Clone)]
pub struct ProfitTarget {
    target: Decimal,
    triggered: bool,
}

impl ProfitTarget {
    pub fn new(target: Decimal) -> Self {
        Self { target, triggered: false }
    }

    /// Evaluate the profit target with the provided price.
    pub fn update(&mut self, price: Decimal) -> Option<SmartTradeSignal> {
        SmartTradeStrategy::evaluate(self, price)
    }
}

impl SmartTradeStrategy for ProfitTarget {
    fn evaluate(&mut self, price: Decimal) -> Option<SmartTradeSignal> {
        if !self.triggered && price >= self.target {
            self.triggered = true;
            Some(SmartTradeSignal::TakeProfit(price))
        } else {
            None
        }
    }
}
