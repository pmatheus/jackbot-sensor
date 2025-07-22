//! Tests for Position structure and update logic.

use crate::engine::state::position::{Position, PositionExited};
use crate::test_utils::{time_plus_days, trade};
use chrono::{DateTime, Utc};
use jackbot_execution::trade::{AssetFees, Trade, TradeId};
use jackbot_instrument::asset::QuoteAsset;
use jackbot_instrument::instrument::name::InstrumentNameInternal;
use jackbot_instrument::Side;
use rust_decimal_macros::dec;

#[test]
fn test_position_update_from_trade() {
    struct TestCase {
        initial_trade: Trade<QuoteAsset, InstrumentNameInternal>,
        update_trade: Trade<QuoteAsset, InstrumentNameInternal>,
        expected_position: Option<Position<QuoteAsset, InstrumentNameInternal>>,
        expected_position_exited: Option<PositionExited<QuoteAsset, InstrumentNameInternal>>,
    }

    let base_time = DateTime::<Utc>::MIN_UTC;

    let cases = vec![
        // TC0: Increase long position
        TestCase {
            initial_trade: trade(base_time, Side::Buy, 100.0, 1.0, 10.0),
            update_trade: trade(time_plus_days(base_time, 1), Side::Buy, 120.0, 1.0, 10.0),
            expected_position: Some(Position {
                instrument: InstrumentNameInternal::new("instrument"),
                side: Side::Buy,
                price_entry_average: dec!(110.0),
                quantity_abs: dec!(2.0),
                quantity_abs_max: dec!(2.0),
                pnl_unrealised: dec!(0.0),
                pnl_realised: dec!(-20.0), // Sum of fees
                fees_enter: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(20.0),
                },
                fees_exit: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(0.0),
                },
                time_enter: base_time,
                time_exchange_update: time_plus_days(base_time, 1),
                trades: vec![TradeId::new("trade_id"), TradeId::new("trade_id")],
            }),
            expected_position_exited: None,
        },
        // TC1: Partial reduce long position
        TestCase {
            initial_trade: trade(base_time, Side::Buy, 100.0, 2.0, 10.0),
            update_trade: trade(time_plus_days(base_time, 1), Side::Sell, 150.0, 0.5, 5.0),
            expected_position: Some(Position {
                instrument: InstrumentNameInternal::new("instrument"),
                side: Side::Buy,
                price_entry_average: dec!(100.0), // update_trade is Sell, so unchanged
                quantity_abs: dec!(1.5),
                quantity_abs_max: dec!(2.0),
                pnl_unrealised: dec!(67.5), // (150-100)*(2.0-0.5) - approx_exit_fees (1.5/2 * 10)
                pnl_realised: dec!(10.0),   // (150-100)*0.5 - 15_fees
                fees_enter: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(10.0),
                },
                fees_exit: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(5.0),
                },
                time_enter: base_time,
                time_exchange_update: time_plus_days(base_time, 1),
                trades: vec![TradeId::new("trade_id"), TradeId::new("trade_id")],
            }),
            expected_position_exited: None,
        },
        // TC2: Exact position close, in profit
        TestCase {
            initial_trade: trade(base_time, Side::Buy, 100.0, 1.0, 10.0),
            update_trade: trade(time_plus_days(base_time, 1), Side::Sell, 150.0, 1.0, 10.0),
            expected_position: None,
            expected_position_exited: Some(PositionExited {
                instrument: InstrumentNameInternal::new("instrument"),
                side: Side::Buy,
                price_entry_average: dec!(100.0),
                quantity_abs_max: dec!(1.0),
                pnl_realised: dec!(30.0), // (150-100)*1 - 20 (total fees)
                fees_enter: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(10.0),
                },
                fees_exit: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(10.0),
                },
                time_enter: base_time,
                time_exit: time_plus_days(base_time, 1),
                trades: vec![TradeId::new("trade_id"), TradeId::new("trade_id")],
            }),
        },
        // TC3: Position flip (close and open new)
        TestCase {
            initial_trade: trade(base_time, Side::Buy, 100.0, 1.0, 10.0),
            update_trade: trade(time_plus_days(base_time, 1), Side::Sell, 150.0, 2.0, 20.0),
            expected_position: Some(Position {
                instrument: InstrumentNameInternal::new("instrument"),
                side: Side::Sell,
                price_entry_average: dec!(150.0),
                quantity_abs: dec!(1.0),
                quantity_abs_max: dec!(1.0),
                pnl_unrealised: dec!(0.0),
                pnl_realised: dec!(-10.0), // Entry fees for new position (2-1)*(1/2)*20
                fees_enter: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(10.0),
                },
                fees_exit: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(0.0),
                },
                time_enter: time_plus_days(base_time, 1),
                time_exchange_update: time_plus_days(base_time, 1),
                trades: vec![TradeId::new("trade_id")],
            }),
            expected_position_exited: Some(PositionExited {
                instrument: InstrumentNameInternal::new("instrument"),
                side: Side::Buy,
                price_entry_average: dec!(100.0),
                quantity_abs_max: dec!(1.0),
                pnl_realised: dec!(30.0), // (150-100)*1 - 20 (total fees)
                fees_enter: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(10.0),
                },
                fees_exit: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(10.0),
                },
                time_enter: base_time,
                time_exit: time_plus_days(base_time, 1),
                trades: vec![TradeId::new("trade_id"), TradeId::new("trade_id")],
            }),
        },
        // TC4: Increase short position
        TestCase {
            initial_trade: trade(base_time, Side::Sell, 100.0, 1.0, 10.0),
            update_trade: trade(base_time, Side::Sell, 80.0, 1.0, 10.0),
            expected_position: Some(Position {
                instrument: InstrumentNameInternal::new("instrument"),
                side: Side::Sell,
                price_entry_average: dec!(90.0), // (100*1 + 80*1)/(1 + 1)
                quantity_abs: dec!(2.0),
                quantity_abs_max: dec!(2.0),
                pnl_unrealised: dec!(0.0), // (90-80)*2 - approx_exit_fees(2/2 * 20)
                pnl_realised: dec!(-20.0), // Sum of entry fees
                fees_enter: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(20.0),
                },
                fees_exit: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(0.0),
                },
                time_enter: base_time,
                time_exchange_update: base_time,
                trades: vec![TradeId::new("trade_id"), TradeId::new("trade_id")],
            }),
            expected_position_exited: None,
        },
        // TC5: Partial reduce short position
        TestCase {
            initial_trade: trade(base_time, Side::Sell, 100.0, 2.0, 10.0),
            update_trade: trade(base_time, Side::Buy, 80.0, 0.5, 5.0),
            expected_position: Some(Position {
                instrument: InstrumentNameInternal::new("instrument"),
                side: Side::Sell,
                price_entry_average: dec!(100.0), // update_trade is Buy, so unchanged
                quantity_abs: dec!(1.5),
                quantity_abs_max: dec!(2.0),
                pnl_unrealised: dec!(22.5), // (100-80)*1.5 - approx_exit_fees(1.5/2 * 10)
                pnl_realised: dec!(-5.0),   // 10_fee_entry - (100-80)*0.5 - 5_fee_exit
                fees_enter: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(10.0),
                },
                fees_exit: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(5.0),
                },
                time_enter: base_time,
                time_exchange_update: base_time,
                trades: vec![TradeId::new("trade_id"), TradeId::new("trade_id")],
            }),
            expected_position_exited: None,
        },
        // TC6: Exact short position close
        TestCase {
            initial_trade: trade(base_time, Side::Sell, 100.0, 1.0, 10.0),
            update_trade: trade(base_time, Side::Buy, 80.0, 1.0, 10.0),
            expected_position: None,
            expected_position_exited: Some(PositionExited {
                instrument: InstrumentNameInternal::new("instrument"),
                side: Side::Sell,
                price_entry_average: dec!(100.0),
                quantity_abs_max: dec!(1.0),
                pnl_realised: dec!(0.0), // (100-80)*1 - 20 (total fees)
                fees_enter: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(10.0),
                },
                fees_exit: AssetFees {
                    asset: QuoteAsset,
                    fees: dec!(10.0),
                },
                time_enter: base_time,
                time_exit: base_time,
                trades: vec![TradeId::new("trade_id"), TradeId::new("trade_id")],
            }),
        },
    ];

    for (index, test) in cases.into_iter().enumerate() {
        let position = Position::from(&test.initial_trade);
        let (updated_position, exited_position) =
            position.update_from_trade(&test.update_trade);

        assert_eq!(updated_position, test.expected_position, "TC{index} failed");
        assert_eq!(
            exited_position, test.expected_position_exited,
            "TC{index} failed"
        );
    }
}