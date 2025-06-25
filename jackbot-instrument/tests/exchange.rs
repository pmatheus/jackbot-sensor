#![allow(unused_crate_dependencies)]
use jackbot_instrument::exchange::ExchangeId;

#[test]
fn test_de_exchange_id() {
    assert_eq!(
        serde_json::from_str::<ExchangeId>(r#""htx""#).unwrap(),
        ExchangeId::Htx
    );
    assert_eq!(
        serde_json::from_str::<ExchangeId>(r#""huobi""#).unwrap(),
        ExchangeId::Htx
    );
    assert_eq!(
        serde_json::from_str::<ExchangeId>(r#""gateio""#).unwrap(),
        ExchangeId::Gateio
    );
}
