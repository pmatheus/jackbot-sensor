use crate::{
    Identifier, exchange::bybit::Bybit, instrument::MarketInstrumentData,
    subscription::Subscription,
};
use jackbot_instrument::{
    asset::name::AssetNameInternal, instrument::market_data::MarketDataInstrument,
    index::Keyed,
};
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, StrExt, format_smolstr};

/// Type that defines how to translate a Jackbot [`Subscription`] into a [`Bybit`]
/// market that can be subscribed to.
///
/// See docs: <https://bybit-exchange.github.io/docs/v5/ws/connect>
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
pub struct BybitMarket(pub SmolStr);

impl<Server, Kind> Identifier<BybitMarket>
    for Subscription<Bybit<Server>, MarketDataInstrument, Kind>
{
    fn id(&self) -> BybitMarket {
        bybit_market(
            &AssetNameInternal(self.instrument.instrument.base.0.to_string()),
            &AssetNameInternal(self.instrument.instrument.quote.0.to_string())
        )
    }
}

impl<Server, InstrumentKey, Kind> Identifier<BybitMarket>
    for Subscription<Bybit<Server>, Keyed<InstrumentKey, MarketDataInstrument>, Kind>
{
    fn id(&self) -> BybitMarket {
        bybit_market(
            &AssetNameInternal(self.instrument.value.instrument.base.0.to_string()),
            &AssetNameInternal(self.instrument.value.instrument.quote.0.to_string())
        )
    }
}

impl<Server, InstrumentKey, Kind> Identifier<BybitMarket>
    for Subscription<Bybit<Server>, MarketInstrumentData<InstrumentKey>, Kind>
{
    fn id(&self) -> BybitMarket {
        BybitMarket(self.instrument.name_exchange.name().clone().into())
    }
}

impl AsRef<str> for BybitMarket {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn bybit_market(base: &AssetNameInternal, quote: &AssetNameInternal) -> BybitMarket {
    // Notes:
    // - Must be uppercase since Bybit sends message with uppercase MARKET (eg/ BTCUSDT).
    BybitMarket(format_smolstr!("{base}{quote}").to_uppercase_smolstr())
}
