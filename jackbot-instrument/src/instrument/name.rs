//! Instrument name definitions

use serde::{Deserialize, Serialize};
use crate::exchange::ExchangeId;

/// Instrument name exchange mapping
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct InstrumentNameExchange {
    pub exchange: ExchangeId,
    pub name: String,
}

impl InstrumentNameExchange {
    pub fn new(exchange: ExchangeId, name: impl Into<String>) -> Self {
        Self {
            exchange,
            name: name.into(),
        }
    }
    
    pub fn name(&self) -> &String {
        &self.name
    }
}

impl AsRef<str> for InstrumentNameExchange {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Display for InstrumentNameExchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.exchange, self.name)
    }
}