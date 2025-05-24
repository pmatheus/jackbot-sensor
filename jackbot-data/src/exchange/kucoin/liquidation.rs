//! Liquidation event parsing for Kucoin futures.

use crate::{
    Identifier,
    event::{MarketEvent, MarketIter},
    subscription::liquidation::Liquidation,
};
use chrono::{DateTime, Utc};
use jackbot_instrument::{Side, exchange::ExchangeId};
use jackbot_integration::subscription::SubscriptionId;
use serde::{Deserialize, Serialize};

/// Kucoin liquidation WebSocket message.
#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct KucoinLiquidation {
    pub topic: String,
    pub data: KucoinLiquidationData,
}

/// Inner liquidation data payload.
#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct KucoinLiquidationData {
    pub symbol: String,
    #[serde(alias = "markPrice", deserialize_with = "jackbot_integration::de::de_str")]
    pub price: f64,
    #[serde(alias = "size", deserialize_with = "jackbot_integration::de::de_str")]
    pub size: f64,
    #[serde(deserialize_with = "de_side")]
    pub side: Side,
    #[serde(
        alias = "ts",
        deserialize_with = "jackbot_integration::de::de_u64_epoch_ms_as_datetime_utc",
    )]
    pub time: DateTime<Utc>,
}

impl Identifier<Option<SubscriptionId>> for KucoinLiquidation {
    fn id(&self) -> Option<SubscriptionId> {
        Some(SubscriptionId::from(self.topic.clone()))
    }
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, KucoinLiquidation)>
    for MarketIter<InstrumentKey, Liquidation>
{
    fn from((exchange_id, instrument, liq): (ExchangeId, InstrumentKey, KucoinLiquidation)) -> Self {
        Self(vec![Ok(MarketEvent {
            time_exchange: liq.data.time,
            time_received: Utc::now(),
            exchange: exchange_id,
            instrument,
            kind: Liquidation {
                side: liq.data.side,
                price: liq.data.price,
                quantity: liq.data.size,
                time: liq.data.time,
            },
        })])
    }
}

fn de_side<'de, D>(deserializer: D) -> Result<Side, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let side = String::deserialize(deserializer)?;
    match side.to_ascii_lowercase().as_str() {
        "buy" => Ok(Side::Buy),
        "sell" => Ok(Side::Sell),
        _ => Err(serde::de::Error::custom(format!(
            "Failed to deserialize Kucoin liquidation side: {}",
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
    fn test_kucoin_liquidation_de() {
        let input = r#"{
            \"topic\": \"/contractMarket/liquidation\",
            \"data\": {
                \"symbol\": \"BTC-USDT\",
                \"markPrice\": \"30000\",
                \"size\": \"1\",
                \"side\": \"sell\",
                \"ts\": 1717000000000
            }
        }"#;

        let liquidation: KucoinLiquidation = serde_json::from_str(input).unwrap();
        assert_eq!(liquidation.data.symbol, "BTC-USDT");
        assert_eq!(liquidation.data.price, 30000.0);
        assert_eq!(liquidation.data.size, 1.0);
        assert_eq!(liquidation.data.side, Side::Sell);
        assert_eq!(
            liquidation.data.time,
            datetime_utc_from_epoch_duration(Duration::from_millis(1717000000000))
        );
    }
}
