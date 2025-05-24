use rust_decimal::Decimal;
use crate::smart_trade::{SmartTradeSignal, SmartTradeStrategy};

#[derive(Debug, Clone)]
pub struct TrailingStop {
    trailing: Decimal,
    highest: Option<Decimal>,
    triggered: bool,
}

impl TrailingStop {
    pub fn new(trailing: Decimal) -> Self {
        Self { trailing, highest: None, triggered: false }
    }

    /// Evaluate the trailing stop with the provided price.
    pub fn update(&mut self, price: Decimal) -> Option<SmartTradeSignal> {
        SmartTradeStrategy::evaluate(self, price)
    }
}

impl SmartTradeStrategy for TrailingStop {
    fn evaluate(&mut self, price: Decimal) -> Option<SmartTradeSignal> {
        if self.triggered {
            return None;
        }
        match self.highest {
            Some(high) => {
                if price > high {
                    self.highest = Some(price);
                }
                if price <= high - self.trailing {
                    self.triggered = true;
                    return Some(SmartTradeSignal::StopLoss(price));
                }
            }
            None => self.highest = Some(price),
        }
        None
    }
}
