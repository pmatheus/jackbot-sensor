use rust_decimal::Decimal;
use crate::smart_trade::{SmartTradeSignal, SmartTradeStrategy};

#[derive(Debug, Clone)]
pub struct MultiLevelTakeProfit {
    levels: Vec<Decimal>,
    current: usize,
}

impl MultiLevelTakeProfit {
    pub fn new(mut levels: Vec<Decimal>) -> Self {
        levels.sort_by(|a, b| a.cmp(b));
        Self { levels, current: 0 }
    }

    /// Evaluate multi-level take profit with the provided price.
    pub fn update(&mut self, price: Decimal) -> Option<SmartTradeSignal> {
        SmartTradeStrategy::evaluate(self, price)
    }
}

impl SmartTradeStrategy for MultiLevelTakeProfit {
    fn evaluate(&mut self, price: Decimal) -> Option<SmartTradeSignal> {
        if self.current >= self.levels.len() {
            return None;
        }
        while self.current < self.levels.len() && price >= self.levels[self.current] {
            self.current += 1;
            return Some(SmartTradeSignal::TakeProfit(price));
        }
        None
    }
}
