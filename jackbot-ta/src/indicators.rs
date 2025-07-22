use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::collections::VecDeque;

/// Simple moving average indicator.
#[derive(Debug, Clone)]
pub struct SimpleMovingAverage {
    period: usize,
    values: VecDeque<Decimal>,
    sum: Decimal,
}

impl SimpleMovingAverage {
    /// Create a new SMA with the given period.
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: VecDeque::new(),
            sum: Decimal::ZERO,
        }
    }

    /// Update the SMA with a new value and return the latest average.
    pub fn update(&mut self, value: Decimal) -> Decimal {
        self.values.push_back(value);
        self.sum += value;
        if self.values.len() > self.period {
            if let Some(old) = self.values.pop_front() {
                self.sum -= old;
            }
        }
        self.average()
    }

    /// Current average value.
    pub fn average(&self) -> Decimal {
        if self.values.is_empty() {
            Decimal::ZERO
        } else {
            self.sum / Decimal::from(self.values.len() as u64)
        }
    }
}

/// Exponential moving average indicator.
#[derive(Debug, Clone)]
pub struct ExponentialMovingAverage {
    multiplier: Decimal,
    value: Option<Decimal>,
}

impl ExponentialMovingAverage {
    /// Create a new EMA with the given period.
    pub fn new(period: usize) -> Self {
        let multiplier = Decimal::from(2u64) / Decimal::from(period as u64 + 1);
        Self {
            multiplier,
            value: None,
        }
    }

    /// Update the EMA with a new price and return the latest value.
    pub fn update(&mut self, price: Decimal) -> Decimal {
        match self.value {
            Some(val) => {
                let next = (price - val) * self.multiplier + val;
                self.value = Some(next);
                next
            }
            None => {
                self.value = Some(price);
                price
            }
        }
    }

    /// Current EMA value if initialised.
    pub fn value(&self) -> Option<Decimal> {
        self.value
    }
}

/// Relative Strength Index (RSI) indicator.
#[derive(Debug, Clone)]
pub struct RelativeStrengthIndex {
    period: usize,
    gains: VecDeque<f64>,
    losses: VecDeque<f64>,
    previous_close: Option<f64>,
    avg_gain: f64,
    avg_loss: f64,
}

impl RelativeStrengthIndex {
    /// Create a new RSI with the given period.
    pub fn new(period: usize) -> Self {
        Self {
            period,
            gains: VecDeque::new(),
            losses: VecDeque::new(),
            previous_close: None,
            avg_gain: 0.0,
            avg_loss: 0.0,
        }
    }

    /// Update the RSI with a new price value.
    pub fn update(&mut self, price: f64) {
        if let Some(prev_close) = self.previous_close {
            let change = price - prev_close;
            let gain = if change > 0.0 { change } else { 0.0 };
            let loss = if change < 0.0 { -change } else { 0.0 };

            self.gains.push_back(gain);
            self.losses.push_back(loss);

            if self.gains.len() > self.period {
                self.gains.pop_front();
                self.losses.pop_front();
            }

            if self.gains.len() == self.period {
                self.avg_gain = self.gains.iter().sum::<f64>() / self.period as f64;
                self.avg_loss = self.losses.iter().sum::<f64>() / self.period as f64;
            }
        }

        self.previous_close = Some(price);
    }

    /// Current RSI value.
    pub fn value(&self) -> Option<f64> {
        if self.avg_loss == 0.0 {
            Some(100.0)
        } else if self.gains.len() == self.period {
            let rs = self.avg_gain / self.avg_loss;
            Some(100.0 - (100.0 / (1.0 + rs)))
        } else {
            None
        }
    }
}

/// Bollinger Bands indicator.
#[derive(Debug, Clone)]
pub struct BollingerBands {
    period: usize,
    multiplier: Decimal,
    values: VecDeque<f64>,
    sma: SimpleMovingAverage,
}

impl BollingerBands {
    /// Create a new Bollinger Bands indicator with the given period and multiplier.
    pub fn new(period: usize, multiplier: Decimal) -> Self {
        Self {
            period,
            multiplier,
            values: VecDeque::new(),
            sma: SimpleMovingAverage::new(period),
        }
    }

    /// Update the Bollinger Bands with a new price value.
    pub fn update(&mut self, price: f64) {
        self.values.push_back(price);
        if self.values.len() > self.period {
            self.values.pop_front();
        }

        let price_decimal = Decimal::from_f64_retain(price).unwrap_or(Decimal::ZERO);
        self.sma.update(price_decimal);
    }

    /// Current Bollinger Bands values (upper, middle, lower).
    pub fn value(&self) -> Option<(f64, f64, f64)> {
        if self.values.len() < self.period {
            return None;
        }

        let middle = self.sma.average().to_f64().unwrap_or(0.0);
        let variance = self.values.iter()
            .map(|v| (v - middle).powi(2))
            .sum::<f64>() / self.period as f64;
        let std_dev = variance.sqrt();
        let multiplier_f64 = self.multiplier.to_f64().unwrap_or(2.0);

        let upper = middle + (std_dev * multiplier_f64);
        let lower = middle - (std_dev * multiplier_f64);

        Some((upper, middle, lower))
    }
}
