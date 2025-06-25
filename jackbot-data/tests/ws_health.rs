use jackbot_data::exchange::{
    Connector, DEFAULT_HEARTBEAT_INTERVAL, bybit::spot::BybitSpot, coinbase::Coinbase,
    hyperliquid::Hyperliquid, kraken::Kraken, kucoin::Kucoin, mexc::Mexc, okx::Okx,
};

#[test]
fn test_exchange_heartbeat_intervals() {
    assert_eq!(
        BybitSpot::heartbeat_interval(),
        Some(DEFAULT_HEARTBEAT_INTERVAL)
    );
    assert_eq!(
        Coinbase::heartbeat_interval(),
        Some(DEFAULT_HEARTBEAT_INTERVAL)
    );
    assert_eq!(
        Kraken::heartbeat_interval(),
        Some(DEFAULT_HEARTBEAT_INTERVAL)
    );
    assert_eq!(
        Kucoin::heartbeat_interval(),
        Some(DEFAULT_HEARTBEAT_INTERVAL)
    );
    assert_eq!(Mexc::heartbeat_interval(), Some(DEFAULT_HEARTBEAT_INTERVAL));
    assert_eq!(Okx::heartbeat_interval(), Some(DEFAULT_HEARTBEAT_INTERVAL));
    assert_eq!(
        Hyperliquid::heartbeat_interval(),
        Some(DEFAULT_HEARTBEAT_INTERVAL)
    );
}
