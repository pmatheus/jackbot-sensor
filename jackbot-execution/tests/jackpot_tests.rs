use chrono::{DateTime, Utc};
use jackbot_execution::jackpot::JackpotMonitor;
use jackbot_execution::order::id::{OrderId, StrategyId};
use jackbot_execution::trade::{AssetFees, Trade, TradeId};
use jackbot_instrument::asset::QuoteAsset;
use jackbot_instrument::exchange::ExchangeId;
use jackbot_instrument::instrument::name::InstrumentNameExchange;
use jackbot_instrument::Side;
use rust_decimal_macros::dec;

#[test]
fn test_liquidation_triggered() {
    let mut monitor = JackpotMonitor::default();
    let trade = Trade {
        id: TradeId::new("t"),
        order_id: OrderId::new("o"), // This is OrderId<SmolStr>
        instrument: InstrumentNameExchange::from("BTC-USDT"),
        strategy: StrategyId::new("j"),
        time_exchange: DateTime::<Utc>::MIN_UTC,
        side: Side::Buy,
        price: dec!(100),
        quantity: dec!(1),
        fees: AssetFees::<QuoteAsset>::quote_fees(dec!(0)), // Adjusted AssetFees for QuoteAsset
    };
    monitor.record_trade(&trade, dec!(10));
    let order = monitor.update_price(ExchangeId::BinanceSpot, &trade.instrument, dec!(89));
    assert!(order.is_some());
    assert!(monitor.is_empty());
}

#[test]
fn test_no_liquidation_when_safe() {
    let mut monitor = JackpotMonitor::default();
    let trade = Trade {
        id: TradeId::new("t"),
        order_id: OrderId::new("o"),
        instrument: InstrumentNameExchange::from("BTC-USDT"),
        strategy: StrategyId::new("j"),
        time_exchange: DateTime::<Utc>::MIN_UTC,
        side: Side::Buy,
        price: dec!(100),
        quantity: dec!(1),
        fees: AssetFees::<QuoteAsset>::quote_fees(dec!(0)), // Adjusted AssetFees for QuoteAsset
    };
    monitor.record_trade(&trade, dec!(10));
    assert!(monitor
        .update_price(ExchangeId::BinanceSpot, &trade.instrument, dec!(95))
        .is_none());
    assert!(!monitor.is_empty());
}
