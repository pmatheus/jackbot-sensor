use jackbot_data::{
    exchange::binance::{spot::trade::BinanceSpotTradesTransformer, BinanceTrade},
    event::{trade::PublicTrade, MarketEvent},
    subscription::{trade::PublicTrades, Map},
    transformer::ExchangeTransformer,
};
use fnv::FnvHashMap;
use jackbot_integration::{subscription::SubscriptionId, Transformer};
use rust_decimal_macros::dec;

fn transformer() -> BinanceSpotTradesTransformer<String> {
    let sub_id = SubscriptionId::from("btcusdt@trade");
    let map = Map(FnvHashMap::from_iter([(sub_id, "BTCUSDT".to_string())]));
    BinanceSpotTradesTransformer::new(map).unwrap()
}

#[test]
fn test_transformer_success() {
    let mut transformer = transformer();
    let trade: BinanceTrade = serde_json::from_str(r#"{
        "e": "trade",
        "E": 1672515782237,
        "s": "BTCUSDT",
        "t": 12345,
        "p": "100.00",
        "q": "1.00",
        "b": 100,
        "a": 101,
        "T": 1672515782237,
        "m": true,
        "M": true
    }"#).unwrap();

    let event = transformer
        .transform(trade)
        .into_iter()
        .next()
        .unwrap()
        .unwrap();

    match event.kind {
        MarketEvent::Trade(PublicTrade {
            id,
            instrument,
            price,
            amount,
            side,
            ..
        }) => {
            assert_eq!(id, "12345".to_string());
            assert_eq!(instrument, "BTCUSDT".to_string());
            assert_eq!(price, dec!(100.00));
            assert_eq!(amount, dec!(1.00));
            assert_eq!(side, jackbot_data::event::trade::TradeSide::Buy); // "m": true indicates maker is buyer
        }
        _ => panic!("Expected MarketEvent::Trade"),
    }
}

#[test]
fn test_transformer_unidentifiable() {
    let mut transformer = transformer();
    let trade: BinanceTrade = serde_json::from_str(r#"{
        "e": "trade",
        "E": 1672515782237,
        "s": "ETHUSDT", // Different symbol from subscription
        "t": 12345,
        "p": "100.00",
        "q": "1.00",
        "b": 100,
        "a": 101,
        "T": 1672515782237,
        "m": true,
        "M": true
    }"#).unwrap();

    let event = transformer
        .transform(trade)
        .into_iter()
        .next()
        .unwrap();

    assert!(event.is_err());
    assert!(matches!(
        event.unwrap_err(),
        jackbot_data::error::DataError::Unidentifiable(_)
    ));
}
