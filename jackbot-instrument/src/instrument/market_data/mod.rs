use crate::asset::name::AssetNameInternal;
use kind::MarketDataInstrumentKind;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

pub mod kind;

/// Jackbot representation of an `MarketDataInstrument`. Used to uniquely identify a `base_quote`
/// pair, and it's associated instrument type.
///
/// eg/ MarketDataInstrument { base: "btc", quote: "usdt", kind: Spot }
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MarketDataInstrument {
    pub base: AssetNameInternal,
    pub quote: AssetNameInternal,
    #[serde(rename = "instrument_kind")]
    pub kind: MarketDataInstrumentKind,
}

impl Display for MarketDataInstrument {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}_{}_{}", self.base, self.quote, self.kind)
    }
}

impl<S> From<(S, S, MarketDataInstrumentKind)> for MarketDataInstrument
where
    S: Into<AssetNameInternal>,
{
    fn from((base, quote, kind): (S, S, MarketDataInstrumentKind)) -> Self {
        Self {
            base: base.into(),
            quote: quote.into(),
            kind,
        }
    }
}

impl MarketDataInstrument {
    /// Constructs a new [`MarketDataInstrument`] using the provided configuration.
    pub fn new<S>(base: S, quote: S, kind: MarketDataInstrumentKind) -> Self
    where
        S: Into<AssetNameInternal>,
    {
        Self {
            base: base.into(),
            quote: quote.into(),
            kind,
        }
    }
}
