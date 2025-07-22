//! Tests for calculation functions.

use crate::engine::state::position::calculations::{
    calculate_pnl_realised, calculate_pnl_return, calculate_pnl_unrealised,
    calculate_price_entry_average,
};
use jackbot_instrument::Side;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn test_calculate_price_entry_average() {
    struct TestCase {
        current_price_entry_average: Decimal,
        current_quantity_abs: Decimal,
        trade_price: Decimal,
        trade_quantity_abs: Decimal,
        expected: Decimal,
    }

    let cases = vec![
        // TC0: equal contribution
        TestCase {
            current_price_entry_average: dec!(100.0),
            current_quantity_abs: dec!(2.0),
            trade_price: dec!(200.0),
            trade_quantity_abs: dec!(2.0),
            expected: dec!(150.0),
        },
        // TC1: trade larger contribution
        TestCase {
            current_price_entry_average: dec!(100.0),
            current_quantity_abs: dec!(2.0),
            trade_price: dec!(200.0),
            trade_quantity_abs: dec!(4.0),
            expected: dec!(166.66666666666666666666666667),
        },
        // TC2: current larger contribution
        TestCase {
            current_price_entry_average: dec!(100.0),
            current_quantity_abs: dec!(20.0),
            trade_price: dec!(200.0),
            trade_quantity_abs: dec!(1.0),
            expected: dec!(104.76190476190476190476190476),
        },
        // TC3: zero current quantity, so expect trade price
        TestCase {
            current_price_entry_average: dec!(100.0),
            current_quantity_abs: dec!(0.0),
            trade_price: dec!(200.0),
            trade_quantity_abs: dec!(4.0),
            expected: dec!(200.0),
        },
        // TC4: zero trade quantity, so expect current price
        TestCase {
            current_price_entry_average: dec!(100.0),
            current_quantity_abs: dec!(10.0),
            trade_price: dec!(0.0),
            trade_quantity_abs: dec!(0.0),
            expected: dec!(100.0),
        },
        // TC5: both zero quantities
        TestCase {
            current_price_entry_average: dec!(100.0),
            current_quantity_abs: dec!(0.0),
            trade_price: dec!(200.0),
            trade_quantity_abs: dec!(0.0),
            expected: dec!(0.0),
        },
    ];

    for (index, test) in cases.into_iter().enumerate() {
        let actual = calculate_price_entry_average(
            test.current_price_entry_average,
            test.current_quantity_abs,
            test.trade_price,
            test.trade_quantity_abs,
        );

        assert_eq!(actual, test.expected, "TC{} failed", index)
    }
}

#[test]
fn test_calculate_pnl_unrealised() {
    struct TestCase {
        position_side: Side,
        price_entry_average: Decimal,
        quantity_abs: Decimal,
        quantity_abs_max: Decimal,
        fees_enter: Decimal,
        price: Decimal,
        expected: Decimal,
    }

    let cases = vec![
        // TC0: LONG position in profit
        TestCase {
            position_side: Side::Buy,
            price_entry_average: dec!(100.0),
            quantity_abs: dec!(1.0),
            quantity_abs_max: dec!(1.0),
            fees_enter: dec!(10.0),
            price: dec!(150.0),
            expected: dec!(40.0), // (150-100)*1 - 10
        },
        // TC1: LONG position at loss
        TestCase {
            position_side: Side::Buy,
            price_entry_average: dec!(100.0),
            quantity_abs: dec!(1.0),
            quantity_abs_max: dec!(1.0),
            fees_enter: dec!(10.0),
            price: dec!(80.0),
            expected: dec!(-30.0), // (80-100)*1 - 10
        },
        // TC2: SHORT position in profit
        TestCase {
            position_side: Side::Sell,
            price_entry_average: dec!(100.0),
            quantity_abs: dec!(1.0),
            quantity_abs_max: dec!(1.0),
            fees_enter: dec!(10.0),
            price: dec!(80.0),
            expected: dec!(10.0), // (100-80)*1 - 10
        },
        // TC3: SHORT position at loss
        TestCase {
            position_side: Side::Sell,
            price_entry_average: dec!(100.0),
            quantity_abs: dec!(1.0),
            quantity_abs_max: dec!(1.0),
            fees_enter: dec!(10.0),
            price: dec!(150.0),
            expected: dec!(-60.0), // (100-150)*1 - 10
        },
        // TC4: Partial position remaining (half closed)
        TestCase {
            position_side: Side::Buy,
            price_entry_average: dec!(100.0),
            quantity_abs: dec!(0.5),
            quantity_abs_max: dec!(1.0),
            fees_enter: dec!(10.0),
            price: dec!(150.0),
            expected: dec!(20.0), // (150-100)*0.5 - (0.5/1.0)*10
        },
        // TC5: Zero quantity position
        TestCase {
            position_side: Side::Buy,
            price_entry_average: dec!(100.0),
            quantity_abs: dec!(0.0),
            quantity_abs_max: dec!(1.0),
            fees_enter: dec!(10.0),
            price: dec!(150.0),
            expected: dec!(0.0),
        },
    ];

    for (index, test) in cases.into_iter().enumerate() {
        let actual = calculate_pnl_unrealised(
            test.position_side,
            test.price_entry_average,
            test.quantity_abs,
            test.quantity_abs_max,
            test.fees_enter,
            test.price,
        );

        assert_eq!(actual, test.expected, "TC{} failed", index);
    }
}

#[test]
fn test_calculate_pnl_realised() {
    struct TestCase {
        side: Side,
        price_entry_average: Decimal,
        closed_quantity: Decimal,
        closed_price: Decimal,
        closed_fee: Decimal,
        expected: Decimal,
    }

    let cases = vec![
        // TC0: LONG in profit w/ fee deduction
        TestCase {
            side: Side::Buy,
            price_entry_average: dec!(100.0),
            closed_quantity: dec!(10.0),
            closed_price: dec!(150.0),
            closed_fee: dec!(5.0),
            expected: dec!(495.0),
        },
        // TC1: LONG in profit w/o fee deduction
        TestCase {
            side: Side::Buy,
            price_entry_average: dec!(100.0),
            closed_quantity: dec!(10.0),
            closed_price: dec!(150.0),
            closed_fee: dec!(0.0),
            expected: dec!(500.0),
        },
        // TC2: LONG in profit w/ fee rebate
        TestCase {
            side: Side::Buy,
            price_entry_average: dec!(100.0),
            closed_quantity: dec!(10.0),
            closed_price: dec!(150.0),
            closed_fee: dec!(-5.0),
            expected: dec!(505.0),
        },
        // TC3: LONG in loss w/ fee deduction
        TestCase {
            side: Side::Buy,
            price_entry_average: dec!(100.0),
            closed_quantity: dec!(10.0),
            closed_price: dec!(50.0),
            closed_fee: dec!(5.0),
            expected: dec!(-505.0),
        },
        // TC4: LONG in loss w/o fee deduction
        TestCase {
            side: Side::Buy,
            price_entry_average: dec!(100.0),
            closed_quantity: dec!(10.0),
            closed_price: dec!(50.0),
            closed_fee: dec!(0.0),
            expected: dec!(-500.0),
        },
        // TC5: LONG in loss w/ fee rebate
        TestCase {
            side: Side::Buy,
            price_entry_average: dec!(100.0),
            closed_quantity: dec!(10.0),
            closed_price: dec!(50.0),
            closed_fee: dec!(-5.0),
            expected: dec!(-495.0),
        },
        // TC6: SHORT in profit w/ fee deduction
        TestCase {
            side: Side::Sell,
            price_entry_average: dec!(100.0),
            closed_quantity: dec!(10.0),
            closed_price: dec!(50.0),
            closed_fee: dec!(5.0),
            expected: dec!(495.0),
        },
        // TC7: SHORT in profit w/o fee deduction
        TestCase {
            side: Side::Sell,
            price_entry_average: dec!(100.0),
            closed_quantity: dec!(10.0),
            closed_price: dec!(50.0),
            closed_fee: dec!(0.0),
            expected: dec!(500.0),
        },
        // TC8: SHORT in profit w/ fee rebate
        TestCase {
            side: Side::Sell,
            price_entry_average: dec!(100.0),
            closed_quantity: dec!(10.0),
            closed_price: dec!(50.0),
            closed_fee: dec!(-5.0),
            expected: dec!(505.0),
        },
        // TC9: SHORT in loss w/ fee deduction
        TestCase {
            side: Side::Sell,
            price_entry_average: dec!(100.0),
            closed_quantity: dec!(10.0),
            closed_price: dec!(150.0),
            closed_fee: dec!(5.0),
            expected: dec!(-505.0),
        },
        // TC10: SHORT in loss w/o fee deduction
        TestCase {
            side: Side::Sell,
            price_entry_average: dec!(100.0),
            closed_quantity: dec!(10.0),
            closed_price: dec!(150.0),
            closed_fee: dec!(0.0),
            expected: dec!(-500.0),
        },
        // TC10: SHORT in loss w/ fee rebate
        TestCase {
            side: Side::Sell,
            price_entry_average: dec!(100.0),
            closed_quantity: dec!(10.0),
            closed_price: dec!(150.0),
            closed_fee: dec!(-5.0),
            expected: dec!(-495.0),
        },
    ];

    for (index, test) in cases.into_iter().enumerate() {
        let actual = calculate_pnl_realised(
            test.side,
            test.price_entry_average.into(),
            test.closed_quantity.into(),
            test.closed_price.into(),
            test.closed_fee.into(),
        );

        assert_eq!(actual, test.expected, "TC{} failed", index);
    }
}

#[test]
fn test_calculate_pnl_return() {
    struct TestCase {
        pnl_realised: Decimal,
        price_entry_average: Decimal,
        quantity_abs_max: Decimal,
        expected: Decimal,
    }

    let cases = vec![
        // TC0: Break even (0% return)
        TestCase {
            pnl_realised: dec!(0.0),
            price_entry_average: dec!(100.0),
            quantity_abs_max: dec!(1.0),
            expected: dec!(0.0),
        },
        // TC1: 100% return
        TestCase {
            pnl_realised: dec!(100.0),
            price_entry_average: dec!(100.0),
            quantity_abs_max: dec!(1.0),
            expected: dec!(1.0),
        },
        // TC2: -50% return
        TestCase {
            pnl_realised: dec!(-50.0),
            price_entry_average: dec!(100.0),
            quantity_abs_max: dec!(1.0),
            expected: dec!(-0.5),
        },
        // TC3: Complex case with larger position
        TestCase {
            pnl_realised: dec!(500.0),
            price_entry_average: dec!(100.0),
            quantity_abs_max: dec!(10.0),
            expected: dec!(0.5), // 500/(100*10)
        },
    ];

    for (index, test) in cases.into_iter().enumerate() {
        let actual = calculate_pnl_return(
            test.pnl_realised.into(),
            test.price_entry_average.into(),
            test.quantity_abs_max.into(),
        );

        assert_eq!(actual, test.expected, "TC{} failed", index);
    }
}