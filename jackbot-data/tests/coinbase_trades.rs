use jackbot_data::exchange::{
    coinbase::{channel::CoinbaseChannel, market::CoinbaseMarket, Coinbase},
    subscription::ExchangeSub,
};
use tokio_tungstenite::tungstenite::Message;

#[test]
fn test_coinbase_spot_trade_requests() {
    let subs = vec![
        ExchangeSub::from((CoinbaseChannel::TRADES, CoinbaseMarket("BTC-USD".into()))),
        ExchangeSub::from((CoinbaseChannel::TRADES, CoinbaseMarket("ETH-USD".into()))),
    ];

    let msgs = Coinbase::requests(subs);
    assert_eq!(msgs.len(), 2);
    for msg in msgs {
        match msg {
            Message::Text(text) => {
                let v: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(v["type"], "subscribe");
                assert_eq!(v["channels"], serde_json::json!(["matches"]));
            }
            _ => panic!("expected text message"),
        }
    }
}

#[test]
fn test_coinbase_spot_trade_requests_empty() {
    let msgs = Coinbase::requests(Vec::<ExchangeSub<_, _>>::new());
    assert!(msgs.is_empty());
}
