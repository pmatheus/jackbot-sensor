use rust_decimal::Decimal;
use jackbot_instrument::instrument::InstrumentIndex;
use serde::{Deserialize, Serialize};
use parking_lot::Mutex;

/// Enum describing various risk violations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RiskViolation<InstrumentKey = InstrumentIndex> {
    ExposureLimit {
        instrument: InstrumentKey,
        exposure: Decimal,
        limit: Decimal,
    },
    DrawdownLimit {
        instrument: InstrumentKey,
        drawdown: Decimal,
        limit: Decimal,
    },
    CorrelationLimit {
        instruments: (InstrumentKey, InstrumentKey),
        combined_exposure: Decimal,
        limit: Decimal,
    },
}

/// Trait allowing consumers to receive risk alerts.
pub trait RiskAlertHook<InstrumentKey = InstrumentIndex> {
    fn alert(&self, violation: RiskViolation<InstrumentKey>);
}

/// Simple alert hook that stores alerts in a vector.
pub struct VecAlertHook<InstrumentKey = InstrumentIndex> {
    pub alerts: Mutex<Vec<RiskViolation<InstrumentKey>>>,
}

impl<InstrumentKey> Default for VecAlertHook<InstrumentKey> {
    fn default() -> Self {
        Self {
            alerts: Mutex::new(Vec::new()),
        }
    }
}

impl<InstrumentKey> RiskAlertHook<InstrumentKey> for VecAlertHook<InstrumentKey>
where
    InstrumentKey: Clone,
{
    fn alert(&self, violation: RiskViolation<InstrumentKey>) {
        self.alerts.lock().push(violation);
    }
}
