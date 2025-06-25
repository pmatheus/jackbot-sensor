use jackbot_data::exchange::binance::futures::liquidation::*;
use jackbot_data::Identifier;
use jackbot_instrument::Side;
use jackbot_integration::subscription::SubscriptionId;
use jackbot_integration::de::datetime_utc_from_epoch_duration;
use std::time::Duration;
use chrono::{DateTime, Utc}; // Added based on usage in original file

#[cfg(test)]
mod tests {
    use super::*;

    mod de {
        use super::*;

        #[test]
        fn test_binance_liquidation() {
            let input = r#"
            {
                "e": "forceOrder",
                "E": 1665523974222,
                "o": {
                    "s": "BTCUSDT",
                    "S": "SELL",
                    "o": "LIMIT",
                    "f": "IOC",
                    "q": "0.009",
                    "p": "18917.15",
                    "ap": "18990.00",
                    "X": "FILLED",
                    "l": "0.009",
                    "z": "0.009",
                    "T": 1665523974217
                }
            }
            "#;

            assert_eq!(
                serde_json::from_str::<BinanceLiquidation>(input).unwrap(),
                BinanceLiquidation {
                    order: BinanceLiquidationOrder {
                        subscription_id: SubscriptionId::from("@forceOrder|BTCUSDT"),
                        side: Side::Sell,
                        price: 18917.15,
                        quantity: 0.009,
                        time: datetime_utc_from_epoch_duration(Duration::from_millis(
                            1665523974217,
                        )),
                    },
                }
            );
        }
    }
}
