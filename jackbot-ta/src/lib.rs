//! Technical Analysis indicators for Jackbot trading systems
//! 
//! This crate provides commonly used technical analysis indicators.

use rust_decimal::Decimal;

/// Simple Moving Average (SMA) indicator
pub struct SimpleMovingAverage {
    period: usize,
    values: Vec<Decimal>,
}

impl SimpleMovingAverage {
    /// Create a new Simple Moving Average indicator
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::with_capacity(period),
        }
    }
    
    /// Add a new value and return the current SMA if available
    pub fn next(&mut self, value: Decimal) -> Option<Decimal> {
        self.values.push(value);
        
        if self.values.len() > self.period {
            self.values.remove(0);
        }
        
        if self.values.len() == self.period {
            let sum: Decimal = self.values.iter().sum();
            Some(sum / Decimal::from(self.period))
        } else {
            None
        }
    }
}