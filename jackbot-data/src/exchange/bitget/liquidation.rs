//! Liquidation event types and parsing for Bitget exchange.

use crate::{
    event::{MarketEvent, MarketIter},
    Identifier,
    subscription::liquidation::Liquidation,
};
use chrono::{DateTime, Utc};
use jackbot_instrument::{exchange::ExchangeId, Side};
use jackbot_integration::subscription::SubscriptionId;
use serde::{Deserialize, Serialize};

/// Bitget liquidation order message.
#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct BitgetLiquidation {
    #[serde(alias = "instId", deserialize_with = "de_liquidation_subscription_id")]
    pub subscription_id: SubscriptionId,
    #[serde(alias = "px", alias = "price", deserialize_with = "jackbot_integration::de::de_str")]
    pub price: f64,
    #[serde(alias = "sz", alias = "size", deserialize_with = "jackbot_integration::de::de_str")]
    pub quantity: f64,
    #[serde(deserialize_with = "de_side")]
    pub side: Side,
    #[serde(
        alias = "ts",
        alias = "timestamp",
        deserialize_with = "jackbot_integration::de::de_str_u64_epoch_ms_as_datetime_utc",
    )]
    pub time: DateTime<Utc>,
}

impl Identifier<Option<SubscriptionId>> for BitgetLiquidation {
    fn id(&self) -> Option<SubscriptionId> {
        Some(self.subscription_id.clone())
    }
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, BitgetLiquidation)>
    for MarketIter<InstrumentKey, Liquidation>
{
    fn from(
        (exchange_id, instrument, liquidation): (ExchangeId, InstrumentKey, BitgetLiquidation),
    ) -> Self {
        Self(vec![Ok(MarketEvent {
            time_exchange: liquidation.time,
            time_received: Utc::now(),
            exchange: exchange_id,
            instrument,
            kind: Liquidation {
                side: liquidation.side,
                price: liquidation.price,
                quantity: liquidation.quantity,
                time: liquidation.time,
            },
        })])
    }
}

impl BitgetLiquidation {
    /// Convert the Bitget message into the standard [`Liquidation`] type.
    pub fn normalize(&self) -> Liquidation {
        Liquidation {
            side: self.side,
            price: self.price,
            quantity: self.quantity,
            time: self.time,
        }
    }
}

/// Deserialize a [`BitgetLiquidation`] market field into a [`SubscriptionId`].
pub fn de_liquidation_subscription_id<'de, D>(deserializer: D) -> Result<SubscriptionId, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    <&str as Deserialize>::deserialize(deserializer)
        .map(|market| SubscriptionId::from(format!("liquidation|{}", market)))
}

/// Deserialize a liquidation side as a [`Side`].
fn de_side<'de, D>(deserializer: D) -> Result<Side, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let side = <&str as Deserialize>::deserialize(deserializer)?.to_ascii_lowercase();
    match side.as_str() {
        "buy" | "long" => Ok(Side::Buy),
        "sell" | "short" => Ok(Side::Sell),
        _ => Err(serde::de::Error::custom(format!(
            "Failed to deserialize Bitget liquidation side: {}",
            side
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jackbot_integration::de::datetime_utc_from_epoch_duration;
    use std::time::Duration;

    #[test]
    fn test_bitget_liquidation_deserialize_and_normalize() {
        let json = r#"{
            \"instId\": \"BTCUSDT_UMCBL\",
            \"px\": \"42000\",
            \"sz\": \"0.01\",
            \"side\": \"sell\",
            \"ts\": \"1717000000000\"
        }"#;

        let liq: BitgetLiquidation = serde_json::from_str(json).unwrap();

        assert_eq!(liq.subscription_id.as_str(), "liquidation|BTCUSDT_UMCBL");
        assert_eq!(liq.price, 42000.0);
        assert_eq!(liq.quantity, 0.01);
        assert_eq!(liq.side, Side::Sell);
        assert_eq!(
            liq.time,
            datetime_utc_from_epoch_duration(Duration::from_millis(1717000000000))
        );

        let normal = liq.normalize();
        assert_eq!(normal.price, 42000.0);
        assert_eq!(normal.quantity, 0.01);
        assert_eq!(normal.side, Side::Sell);
    }
}
