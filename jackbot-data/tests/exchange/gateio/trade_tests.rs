use jackbot_data::exchange::gateio::trade::GateioTrade;
use jackbot_instrument::Side;

#[test]
fn test_gateio_trade_to_public_trade() {
    let json = r#"{
        "currency_pair": "BTC_USDT",
        "price": "42000.5",
        "amount": "0.01",
        "side": "buy",
        "time": 1717000000000,
        "id": "abc"
    }"#;
    let trade: GateioTrade = serde_json::from_str(json).unwrap();
    let public = trade.to_public_trade().unwrap();
    assert_eq!(public.price, 42000.5);
    assert_eq!(public.amount, 0.01);
    assert_eq!(public.side, Side::Buy);
    assert_eq!(public.id, "abc");
}
