//! Calculation functions for position metrics and P&L.

use jackbot_instrument::Side;
use rust_decimal::Decimal;

/// Calculates the volume-weighted average entry price when adding a [`Trade`] data to existing
/// [`Position`] data.
///
/// This function uses the formula: <br>
/// (current_value + trade_value) / (current_quantity + trade_quantity)
///
/// # Arguments
/// * `current_price_entry_average` - The current average entry price of the position
/// * `current_quantity_abs` - The current absolute quantity of the position
/// * `trade_price` - The price of the new trade
/// * `trade_quantity_abs` - The absolute quantity of the new trade
pub fn calculate_price_entry_average(
    current_price_entry_average: Decimal,
    current_quantity_abs: Decimal,
    trade_price: Decimal,
    trade_quantity_abs: Decimal,
) -> Decimal {
    if current_quantity_abs.is_zero() && trade_quantity_abs.is_zero() {
        return Decimal::ZERO;
    }

    let current_value = current_price_entry_average * current_quantity_abs;
    let trade_value = trade_price * trade_quantity_abs;

    (current_value + trade_value) / (current_quantity_abs + trade_quantity_abs)
}

/// Calculate the estimated unrealised PnL from closing a [`Position`] `quantity_abs` at the
/// provided price.
pub fn calculate_pnl_unrealised(
    position_side: Side,
    price_entry_average: Decimal,
    quantity_abs: Decimal,
    quantity_abs_max: Decimal,
    fees_enter: Decimal,
    price: Decimal,
) -> Decimal {
    let approx_exit_fees =
        approximate_remaining_exit_fees(quantity_abs, quantity_abs_max, fees_enter);

    let value_quote_current = quantity_abs * price;
    let value_quote_entry = quantity_abs * price_entry_average;

    match position_side {
        Side::Buy => value_quote_current - value_quote_entry - approx_exit_fees,
        Side::Sell => value_quote_entry - value_quote_current - approx_exit_fees,
    }
}

/// Approximate the exit fees from closing a [`Position`] with `quantity_abs`.
///
/// The `fees_enter` value was the fee cost to enter a [`Position`] of `quantity_abs_max`,
/// therefore this 'fee per quantity' ratio can be used to approximate the exit fees required to
/// close a `quantity_abs` [`Position`].
fn approximate_remaining_exit_fees(
    quantity_abs: Decimal,
    quantity_abs_max: Decimal,
    fees_enter: Decimal,
) -> Decimal {
    (quantity_abs / quantity_abs_max) * fees_enter
}

/// Calculate the realised PnL generated from closing the provided [`Position`] quantity, at the
/// specified price and closing fee.
pub fn calculate_pnl_realised(
    position_side: Side,
    price_entry_average: Decimal,
    closed_quantity: Decimal,
    closed_price: Decimal,
    closed_fee: Decimal,
) -> Decimal {
    let close_quantity = closed_quantity.abs();
    let value_quote_closed = close_quantity * closed_price;
    let value_quote_entry = close_quantity * price_entry_average;

    match position_side {
        Side::Buy => value_quote_closed - value_quote_entry - closed_fee,
        Side::Sell => value_quote_entry - value_quote_closed - closed_fee,
    }
}

/// Calculate the PnL returns.
///
/// Returns = pnl_realised / cost_of_investment
///
/// See docs: <https://www.investopedia.com/articles/basics/10/guide-to-calculating-roi.asp>
pub fn calculate_pnl_return(
    pnl_realised: Decimal,
    price_entry_average: Decimal,
    quantity_abs_max: Decimal,
) -> Decimal {
    pnl_realised / (price_entry_average * quantity_abs_max)
}