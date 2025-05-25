use jackbot_execution::{
    jackpot_orders::JackpotMonitor,
    order::{id::{OrderId, StrategyId}, request::OrderRequestOpen},
    trade::{Trade, TradeId, AssetFees},
};
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange, Side};
use chrono::{DateTime, Utc};
use rust_decimal_macros::dec;

#[test]
fn monitor_triggers_liquidation() {
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
        fees: AssetFees::quote_fees(dec!(0)),
    };
    monitor.record_trade(&trade, dec!(5));
    let order: Option<OrderRequestOpen<ExchangeId, InstrumentNameExchange>> =
        monitor.update_price(ExchangeId::BinanceSpot, &trade.instrument, dec!(94));
    assert!(order.is_some());
}

#[test]
fn monitor_no_liquidation_when_profit() {
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
        fees: AssetFees::quote_fees(dec!(0)),
    };
    monitor.record_trade(&trade, dec!(5));
    assert!(monitor.update_price(ExchangeId::BinanceSpot, &trade.instrument, dec!(105)).is_none());
}
