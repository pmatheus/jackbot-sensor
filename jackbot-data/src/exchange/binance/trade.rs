//! Trade event parsing and types for Binance exchange.
use super::BinanceChannel;
use crate::{
    Identifier,
    event::{MarketEvent, MarketIter},
    exchange::ExchangeSub,
    subscription::trade::PublicTrade,
};
use chrono::{DateTime, Utc};
use jackbot_instrument::{Side, exchange::ExchangeId};
use jackbot_integration::subscription::SubscriptionId;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct BinanceTrade {
    #[serde(alias = "s", deserialize_with = "de_trade_subscription_id")]
    pub subscription_id: SubscriptionId,
    #[serde(
        alias = "T",
        deserialize_with = "jackbot_integration::de::de_u64_epoch_ms_as_datetime_utc"
    )]
    pub time: DateTime<Utc>,
    #[serde(alias = "t")]
    pub id: u64,
    #[serde(alias = "p", deserialize_with = "jackbot_integration::de::de_str")]
    pub price: f64,
    #[serde(alias = "q", deserialize_with = "jackbot_integration::de::de_str")]
    pub amount: f64,
    #[serde(alias = "m", deserialize_with = "de_side_from_buyer_is_maker")]
    pub side: Side,
}

impl Identifier<Option<SubscriptionId>> for BinanceTrade {
    fn id(&self) -> Option<SubscriptionId> {
        Some(self.subscription_id.clone())
    }
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, BinanceTrade)>
    for MarketIter<InstrumentKey, PublicTrade>
{
    fn from((exchange_id, instrument, trade): (ExchangeId, InstrumentKey, BinanceTrade)) -> Self {
        Self(vec![Ok(MarketEvent {
            time_exchange: trade.time,
            time_received: Utc::now(),
            exchange: exchange_id,
            instrument,
            kind: PublicTrade {
                id: trade.id.to_string(),
                price: trade.price,
                amount: trade.amount,
                side: trade.side,
            },
        })])
    }
}

/// Deserialize a [`BinanceTrade`] "s" (eg/ "BTCUSDT") as the associated [`SubscriptionId`]
/// (eg/ "@trade|BTCUSDT").
pub fn de_trade_subscription_id<'de, D>(deserializer: D) -> Result<SubscriptionId, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    <&str as Deserialize>::deserialize(deserializer)
        .map(|market| ExchangeSub::from((BinanceChannel::TRADES, market)).id())
}

/// Deserialize a [`BinanceTrade`] "buyer_is_maker" boolean field to a Jackbot [`Side`].
///
/// Variants:
/// buyer_is_maker => Side::Sell
/// !buyer_is_maker => Side::Buy
pub fn de_side_from_buyer_is_maker<'de, D>(deserializer: D) -> Result<Side, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(|buyer_is_maker| {
        if buyer_is_maker {
            Side::Sell
        } else {
            Side::Buy
        }
    })
}
