//! Instrument definitions and market data types

pub mod market_data;
pub mod kind;
pub mod name;

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use crate::exchange::ExchangeId;

/// Base currency identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct BaseCurrency(pub SmolStr);

/// Quote currency identifier  
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct QuoteCurrency(pub SmolStr);

/// Trading instrument representing a currency pair
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Instrument {
    pub base: BaseCurrency,
    pub quote: QuoteCurrency,
    pub kind: market_data::kind::MarketDataInstrumentKind,
}

impl Instrument {
    pub fn new(base: &str, quote: &str, kind: market_data::kind::MarketDataInstrumentKind) -> Self {
        Self {
            base: BaseCurrency(SmolStr::new(base)),
            quote: QuoteCurrency(SmolStr::new(quote)),
            kind,
        }
    }
    
    pub fn spot(base: &str, quote: &str) -> Self {
        Self::new(base, quote, market_data::kind::MarketDataInstrumentKind::Spot)
    }
    
    pub fn perpetual(base: &str, quote: &str) -> Self {
        Self::new(base, quote, market_data::kind::MarketDataInstrumentKind::Perpetual)
    }
}

impl std::fmt::Display for Instrument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{} ({})", self.base.0, self.quote.0, self.kind)
    }
}

/// Instrument index combining exchange and instrument information
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct InstrumentIndex {
    pub exchange: ExchangeId,
    pub instrument: Instrument,
}

impl InstrumentIndex {
    pub fn new(exchange: ExchangeId, instrument: Instrument) -> Self {
        Self { exchange, instrument }
    }
}

impl std::fmt::Display for InstrumentIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.exchange, self.instrument)
    }
}