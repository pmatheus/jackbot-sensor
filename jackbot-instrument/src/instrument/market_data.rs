//! Market data instrument types

pub mod kind;

use crate::instrument::Instrument;
use serde::{Deserialize, Serialize};

/// Market data instrument with extended metadata
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketDataInstrument {
    pub instrument: Instrument,
    pub symbol: String,
    pub tick_size: Option<rust_decimal::Decimal>,
    pub lot_size: Option<rust_decimal::Decimal>,
    pub name_exchange: String,
    pub kind: kind::MarketDataInstrumentKind,
}

impl MarketDataInstrument {
    pub fn new(
        base: &str,
        quote: &str,
        kind: kind::MarketDataInstrumentKind,
        symbol: String,
    ) -> Self {
        let name_exchange = symbol.clone();
        Self {
            instrument: Instrument::new(base, quote, kind),
            symbol,
            tick_size: None,
            lot_size: None,
            name_exchange,
            kind,
        }
    }
    
    pub fn with_precision(mut self, tick_size: rust_decimal::Decimal, lot_size: rust_decimal::Decimal) -> Self {
        self.tick_size = Some(tick_size);
        self.lot_size = Some(lot_size);
        self
    }
    
    pub fn with_exchange_name(mut self, name: String) -> Self {
        self.name_exchange = name;
        self
    }
}

impl std::fmt::Display for MarketDataInstrument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} [{}]", self.instrument, self.symbol)
    }
}