use Jackbot::statistic::summary::pnl::PnLReturns;
use Jackbot::engine::state::position::PositionExited;
use jackbot_execution::trade::{TradeId, AssetFees};
use jackbot_instrument::{asset::QuoteAsset, instrument::InstrumentIndex, Side};
use chrono::Utc;
use rust_decimal_macros::dec;

#[test]
fn test_pnl_returns_update() {
    let mut pnl = PnLReturns::default();
    let position = PositionExited {
        instrument: InstrumentIndex(0),
        side: Side::Buy,
        price_entry_average: dec!(100),
        quantity_abs_max: dec!(1),
        pnl_realised: dec!(10),
        fees_enter: AssetFees::quote_fees(dec!(0)),
        fees_exit: AssetFees::quote_fees(dec!(0)),
        time_enter: Utc::now(),
        time_exit: Utc::now(),
        trades: vec![TradeId::new("t1")],
    };

    pnl.update(&position);

    assert_eq!(pnl.pnl_raw, dec!(10));
    assert_eq!(pnl.total.count, dec!(1));
    assert_eq!(pnl.losses.count, dec!(0));
}
