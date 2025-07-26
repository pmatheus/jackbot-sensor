//! Market data instrument kind definitions

use serde::{Deserialize, Serialize};

/// Market data instrument kind enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum MarketDataInstrumentKind {
    #[serde(rename = "spot")]
    Spot,
    #[serde(rename = "perpetual")]
    Perpetual,
    #[serde(rename = "future")]
    Future,
    #[serde(rename = "option")]
    Option,
}

impl std::fmt::Display for MarketDataInstrumentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarketDataInstrumentKind::Spot => write!(f, "spot"),
            MarketDataInstrumentKind::Perpetual => write!(f, "perpetual"),
            MarketDataInstrumentKind::Future => write!(f, "future"),
            MarketDataInstrumentKind::Option => write!(f, "option"),
        }
    }
}