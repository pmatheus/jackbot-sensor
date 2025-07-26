use super::Kraken;
use crate::{Identifier, instrument::MarketInstrumentData, subscription::Subscription};
use jackbot_instrument::{
    asset::name::AssetNameInternal, instrument::market_data::MarketDataInstrument,
    index::Keyed,
};
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, StrExt, format_smolstr};

/// Type that defines how to translate a Jackbot [`Subscription`] into a
/// [`Kraken`] market that can be subscribed to.
///
/// See docs: <https://docs.kraken.com/websockets/#message-subscribe>
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
pub struct KrakenMarket(pub SmolStr);

impl<Kind> Identifier<KrakenMarket> for Subscription<Kraken, MarketDataInstrument, Kind> {
    fn id(&self) -> KrakenMarket {
        kraken_market(
            &AssetNameInternal(self.instrument.instrument.base.0.to_string()),
            &AssetNameInternal(self.instrument.instrument.quote.0.to_string())
        )
    }
}

impl<InstrumentKey, Kind> Identifier<KrakenMarket>
    for Subscription<Kraken, Keyed<InstrumentKey, MarketDataInstrument>, Kind>
{
    fn id(&self) -> KrakenMarket {
        kraken_market(
            &AssetNameInternal(self.instrument.value.instrument.base.0.to_string()),
            &AssetNameInternal(self.instrument.value.instrument.quote.0.to_string())
        )
    }
}

impl<InstrumentKey, Kind> Identifier<KrakenMarket>
    for Subscription<Kraken, MarketInstrumentData<InstrumentKey>, Kind>
{
    fn id(&self) -> KrakenMarket {
        KrakenMarket(self.instrument.name_exchange.name().clone().into())
    }
}

impl AsRef<str> for KrakenMarket {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn kraken_market(base: &AssetNameInternal, quote: &AssetNameInternal) -> KrakenMarket {
    KrakenMarket(format_smolstr!("{base}/{quote}").to_lowercase_smolstr())
}
