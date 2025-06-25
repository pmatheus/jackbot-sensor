use jackbot_data::exchange::kraken::{message::KrakenError, subscription::KrakenSubResponse};
use jackbot_integration::{error::SocketError, Validator};

// Paste the test module content here, without the #[cfg(test)] and mod tests {}
// Add necessary imports if not already present at the top level of this new file

mod de {
    use super::*;

    #[test]
    fn test_kraken_sub_response() {
        struct TestCase {
            input: &'static str,
            expected: Result<KrakenSubResponse, SocketError>,
        }

        let cases = vec![
            TestCase {
                // TC0: input response is Subscribed
                input: r#"
                {
                    "channelID": 10001,
                    "channelName": "ticker",
                    "event": "subscriptionStatus",
                    "pair": "XBT/EUR",
                    "status": "subscribed",
                    "subscription": {
                        "name": "ticker"
                    }
                }
                "#,
                expected: Ok(KrakenSubResponse::Subscribed {
                    channel_id: 10001,
                    channel_name: "ticker".to_string(),
                    pair: "XBT/EUR".to_string(),
                }),
            },
            TestCase {
                // TC1: input response is failed subscription
                input: r#"
                {
                    "errorMessage": "Subscription name invalid",
                    "event": "subscriptionStatus",
                    "pair": "XBT/USD",
                    "status": "error",
                    "subscription": {
                        "name": "trades"
                    }
                }
                "#,
                expected: Ok(KrakenSubResponse::Error(KrakenError {
                    message: "Subscription name invalid".to_string(),
                })),
            },
        ];

        for (index, test) in cases.into_iter().enumerate() {
            let actual = serde_json::from_str::<KrakenSubResponse>(test.input);
            match (actual, test.expected) {
                (Ok(actual), Ok(expected)) => {
                    assert_eq!(actual, expected, "TC{} failed", index)
                }
                (Err(_), Err(_)) => {
                    // Test passed
                }
                (actual, expected) => {
                    // Test failed
                    panic!(
                        "TC{index} failed because actual != expected. \nActual: {actual:?}\nExpected: {expected:?}\n"
                    );
                }
            }
        }
    }
}

#[test]
fn test_kraken_sub_response_validate() {
    struct TestCase {
        input_response: KrakenSubResponse,
        is_valid: bool,
    }

    let cases = vec![
        TestCase {
            // TC0: input response is successful subscription
            input_response: KrakenSubResponse::Subscribed {
                channel_id: 10001,
                channel_name: "ticker".to_string(),
                pair: "XBT/EUR".to_string(),
            },
            is_valid: true,
        },
        TestCase {
            // TC1: input response is failed subscription
            input_response: KrakenSubResponse::Error(KrakenError {
                message: "Subscription name invalid".to_string(),
            }),
            is_valid: false,
        },
    ];

    for (index, test) in cases.into_iter().enumerate() {
        let actual = test.input_response.validate().is_ok();
        assert_eq!(actual, test.is_valid, "TestCase {} failed", index);
    }
}
