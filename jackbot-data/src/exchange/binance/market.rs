//! Market info and types for Binance exchange.
use super::Binance;
use crate::{Identifier, instrument::MarketInstrumentData, subscription::Subscription};
use jackbot_instrument::{
    asset::name::AssetNameInternal, instrument::market_data::MarketDataInstrument,
    index::Keyed,
};
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, StrExt, format_smolstr};

/// Type that defines how to translate a Jackbot [`Subscription`] into a [`Binance`]
/// market that can be subscribed to.
///
/// See docs: <https://binance-docs.github.io/apidocs/spot/en/#websocket-market-streams>
/// See docs: <https://binance-docs.github.io/apidocs/futures/en/#websocket-market-streams>
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
pub struct BinanceMarket(pub SmolStr);

impl<Server, Kind> Identifier<BinanceMarket>
    for Subscription<Binance<Server>, MarketDataInstrument, Kind>
{
    fn id(&self) -> BinanceMarket {
        binance_market(
            &AssetNameInternal(self.instrument.instrument.base.0.to_string()),
            &AssetNameInternal(self.instrument.instrument.quote.0.to_string())
        )
    }
}

impl<Server, InstrumentKey, Kind> Identifier<BinanceMarket>
    for Subscription<Binance<Server>, Keyed<InstrumentKey, MarketDataInstrument>, Kind>
{
    fn id(&self) -> BinanceMarket {
        binance_market(
            &AssetNameInternal(self.instrument.as_ref().instrument.base.0.to_string()),
            &AssetNameInternal(self.instrument.as_ref().instrument.quote.0.to_string()),
        )
    }
}

impl<Server, InstrumentKey, Kind> Identifier<BinanceMarket>
    for Subscription<Binance<Server>, MarketInstrumentData<InstrumentKey>, Kind>
{
    fn id(&self) -> BinanceMarket {
        BinanceMarket(self.instrument.name_exchange.name().clone().into())
    }
}

impl AsRef<str> for BinanceMarket {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

pub(in crate::exchange::binance) fn binance_market(
    base: &AssetNameInternal,
    quote: &AssetNameInternal,
) -> BinanceMarket {
    // Notes:
    // - Must be lowercase when subscribing (transformed to lowercase by Binance fn requests).
    // - Must be uppercase since Binance sends message with uppercase MARKET (eg/ BTCUSDT).
    BinanceMarket(format_smolstr!("{base}{quote}").to_uppercase_smolstr())
}
