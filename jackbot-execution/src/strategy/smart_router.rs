use rust_decimal::Decimal;

/// Simple smart routing state with exposure tracking.
#[derive(Debug)]
pub struct SmartRouter {
    max_exposure: Decimal,
    current_exposure: Decimal,
}

impl SmartRouter {
    /// Create a new router with the specified maximum exposure.
    pub fn new(max_exposure: Decimal) -> Self {
        Self {
            max_exposure,
            current_exposure: Decimal::ZERO,
        }
    }

    /// Returns true if a trade of `quantity` would exceed exposure limits.
    pub fn can_execute(&self, quantity: Decimal) -> bool {
        self.current_exposure + quantity <= self.max_exposure
    }

    /// Record a new executed quantity, increasing exposure. Returns Err if the
    /// exposure would exceed the configured limit.
    pub fn record_execution(&mut self, quantity: Decimal) -> Result<(), Decimal> {
        if self.can_execute(quantity) {
            self.current_exposure += quantity;
            Ok(())
        } else {
            Err(self.current_exposure + quantity - self.max_exposure)
        }
    }

    /// Reduce exposure after a position is closed or filled.
    pub fn reduce_exposure(&mut self, quantity: Decimal) {
        self.current_exposure -= quantity;
        if self.current_exposure < Decimal::ZERO {
            self.current_exposure = Decimal::ZERO;
        }
    }

    /// Current exposure value.
    pub fn exposure(&self) -> Decimal {
        self.current_exposure
    }
}
