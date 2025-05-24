//! Market info and types for Kucoin exchange.

use super::Kucoin;
use crate::{Identifier, instrument::MarketInstrumentData, subscription::Subscription};
use jackbot_instrument::{
    Keyed,
    asset::name::AssetNameInternal,
    instrument::market_data::MarketDataInstrument,
};
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, StrExt, format_smolstr};

/// Type that defines how to translate a Jackbot [`Subscription`] into a
/// [`Kucoin`] market that can be subscribed to.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
pub struct KucoinMarket(pub SmolStr);

impl From<&str> for KucoinMarket {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl From<String> for KucoinMarket {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}

impl<Kind> Identifier<KucoinMarket> for Subscription<Kucoin, MarketDataInstrument, Kind> {
    fn id(&self) -> KucoinMarket {
        kucoin_market(&self.instrument.base, &self.instrument.quote)
    }
}

impl<InstrumentKey, Kind> Identifier<KucoinMarket>
    for Subscription<Kucoin, Keyed<InstrumentKey, MarketDataInstrument>, Kind>
{
    fn id(&self) -> KucoinMarket {
        kucoin_market(&self.instrument.value.base, &self.instrument.value.quote)
    }
}

impl<InstrumentKey, Kind> Identifier<KucoinMarket>
    for Subscription<Kucoin, MarketInstrumentData<InstrumentKey>, Kind>
{
    fn id(&self) -> KucoinMarket {
        KucoinMarket(self.instrument.name_exchange.name().clone())
    }
}

impl AsRef<str> for KucoinMarket {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl KucoinMarket {
    /// Normalise the market identifier for use in subscription requests.
    ///
    /// Kucoin expects uppercase markets separated by a dash, e.g. "BTC-USDT".
    pub fn normalize(&self) -> SmolStr {
        self.0.to_uppercase_smolstr()
    }
}

fn kucoin_market(base: &AssetNameInternal, quote: &AssetNameInternal) -> KucoinMarket {
    KucoinMarket(format_smolstr!("{base}-{quote}").to_uppercase_smolstr())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_market() {
        let market = KucoinMarket::from("btc-usdt");
        assert_eq!(market.normalize().as_str(), "BTC-USDT");
    }
}

