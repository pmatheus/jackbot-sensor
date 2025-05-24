//! Liquidations stream and normalization for Hyperliquid.

use crate::{
    Identifier,
    event::{MarketEvent, MarketIter},
    subscription::liquidation::Liquidation,
};
use chrono::{DateTime, Utc};
use jackbot_instrument::{Side, exchange::ExchangeId};
use jackbot_integration::subscription::SubscriptionId;
use serde::{Deserialize, Serialize};

/// Hyperliquid liquidation message as received from the WebSocket API.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HyperliquidLiquidation {
    #[serde(alias = "coin", deserialize_with = "de_liq_subscription_id")]
    pub subscription_id: SubscriptionId,
    pub side: String,
    #[serde(alias = "px", deserialize_with = "jackbot_integration::de::de_str")]
    pub price: f64,
    #[serde(alias = "sz", deserialize_with = "jackbot_integration::de::de_str")]
    pub quantity: f64,
    #[serde(
        alias = "time",
        deserialize_with = "jackbot_integration::de::de_u64_epoch_ms_as_datetime_utc",
    )]
    pub time: DateTime<Utc>,
}

impl Identifier<Option<SubscriptionId>> for HyperliquidLiquidation {
    fn id(&self) -> Option<SubscriptionId> {
        Some(self.subscription_id.clone())
    }
}

impl HyperliquidLiquidation {
    fn side(&self) -> Option<Side> {
        match self.side.to_ascii_lowercase().as_str() {
            "buy" => Some(Side::Buy),
            "sell" => Some(Side::Sell),
            _ => None,
        }
    }
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, HyperliquidLiquidation)>
    for MarketIter<InstrumentKey, Liquidation>
{
    fn from(
        (exchange_id, instrument, liq): (ExchangeId, InstrumentKey, HyperliquidLiquidation),
    ) -> Self {
        let Some(side) = liq.side() else { return Self(vec![]) };
        Self(vec![Ok(MarketEvent {
            time_exchange: liq.time,
            time_received: Utc::now(),
            exchange: exchange_id,
            instrument,
            kind: Liquidation {
                side,
                price: liq.price,
                quantity: liq.quantity,
                time: liq.time,
            },
        })])
    }
}

/// Deserialize a [`HyperliquidLiquidation`] "coin" as the associated [`SubscriptionId`].
pub fn de_liq_subscription_id<'de, D>(deserializer: D) -> Result<SubscriptionId, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    <&str as Deserialize>::deserialize(deserializer).map(|market| {
        SubscriptionId::from(format!("liquidations|{}", market))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jackbot_integration::de::datetime_utc_from_epoch_duration;
    use std::time::Duration;

    #[test]
    fn test_hyperliquid_liquidation() {
        let input = r#"{\"coin\":\"BTC\",\"side\":\"buy\",\"px\":\"30000.0\",\"sz\":\"1.0\",\"time\":1717000000000}"#;
        let liq: HyperliquidLiquidation = serde_json::from_str(input).unwrap();
        assert_eq!(liq.subscription_id, SubscriptionId::from("liquidations|BTC"));
        assert_eq!(liq.price, 30000.0);
        assert_eq!(liq.quantity, 1.0);
        assert_eq!(liq.side(), Some(Side::Buy));
        assert_eq!(
            liq.time,
            datetime_utc_from_epoch_duration(Duration::from_millis(1717000000000))
        );
    }
}
