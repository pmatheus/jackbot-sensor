//! Core position data structures for the trading engine.

use chrono::{DateTime, Utc};
use derive_more::Constructor;
use jackbot_execution::trade::{AssetFees, Trade, TradeId};
use jackbot_instrument::{
    Side,
    asset::{AssetIndex, QuoteAsset},
    instrument::InstrumentIndex,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use tracing::error;

use crate::engine::state::position::calculations::{
    calculate_pnl_realised, calculate_pnl_unrealised, calculate_price_entry_average,
};

/// Represents an open trading position for a specific instrument.
///
/// # Type Parameters
/// - `AssetKey`: The type representing the asset used for fees (e.g. AssetIndex, QuoteAsset, etc.)
/// - `InstrumentKey`: The type identifying the traded instrument (e.g. InstrumentIndex, etc.)
///
/// # Examples
/// ## Partially Reduce LONG Position
/// ```rust
/// use Jackbot::engine::state::position::Position;
/// use jackbot_execution::order::id::{OrderId, StrategyId};
/// use jackbot_execution::trade::{AssetFees, Trade, TradeId};
/// use jackbot_instrument::asset::QuoteAsset;
/// use jackbot_instrument::instrument::name::InstrumentNameInternal;
/// use jackbot_instrument::Side;
/// use chrono::{DateTime, Utc};
/// use std::str::FromStr;
/// use rust_decimal_macros::dec;
///
/// // Create a new LONG Position from an initial Buy trade
/// let position = Position::from(&Trade {
///     id: TradeId::new("trade_1"),
///     order_id: OrderId::new("order_1"),
///     instrument: InstrumentNameInternal::new("BTC-USD"),
///     strategy: StrategyId::new("strategy_1"),
///     time_exchange: DateTime::from_str("2024-01-01T00:00:00Z").unwrap(),
///     side: Side::Buy,
///     price: dec!(50_000.0),
///     quantity: dec!(0.1),
///     fees: AssetFees::quote_fees(dec!(5.0))
/// });
/// assert_eq!(position.side, Side::Buy);
/// assert_eq!(position.quantity_abs, dec!(0.1));
///
/// // Partially reduce LONG Position from a new Sell Trade
/// let (updated_position, closed_position) = position.update_from_trade(&Trade {
///     id: TradeId::new("trade_2"),
///     order_id: OrderId::new("order_2"),
///     instrument: InstrumentNameInternal::new("BTC-USD"),
///     strategy: StrategyId::new("strategy_1"),
///     time_exchange: DateTime::from_str("2024-01-01T01:00:00Z").unwrap(),
///     side: Side::Sell,
///     price: dec!(60_000.0),
///     quantity: dec!(0.05),
///     fees: AssetFees::quote_fees(dec!(3.0))
/// });
///
/// // Position has been reduced by 0.05
/// let position = updated_position.unwrap();
/// assert_eq!(position.quantity_abs, dec!(0.05));
/// assert_eq!(position.quantity_abs_max, dec!(0.1));
/// assert_eq!(position.pnl_realised, dec!(497.0)); // 500 profit - 3 exit fee - 5 entry fee
/// ```
#[derive(
    Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize, Constructor,
)]
pub struct Position<AssetKey = AssetIndex, InstrumentKey = InstrumentIndex> {
    /// [`Position`] Instrument identifier (eg/ InstrumentIndex, InstrumentNameInternal, etc.).
    pub instrument: InstrumentKey,

    /// [`Position`] direction (Side::Buy => LONG, Side::Sell => SHORT).
    pub side: Side,

    /// Volume-weighted average entry price across all [`Position`] increasing [`Trade`]s.
    pub price_entry_average: Decimal,

    /// Current absolute [`Position`] quantity.
    pub quantity_abs: Decimal,

    /// Maximum absolute [`Position`] quantity reached by all entry/increase [`Trade`]s.
    pub quantity_abs_max: Decimal,

    /// Estimated unrealised PnL generated from closing the remaining [`Position`] `quantity_abs`.
    ///
    /// Note this includes estimated exit fees.
    pub pnl_unrealised: Decimal,

    /// Cumulative realised PnL from any partially closed [`Position`] `quantity_abs_max`.
    ///
    /// Note this includes fees.
    pub pnl_realised: Decimal,

    /// Cumulative fees paid when entering/increasing [`Position`] quantity.
    pub fees_enter: AssetFees<AssetKey>,

    /// Cumulative fees paid when exiting/reducing [`Position`] quantity.
    pub fees_exit: AssetFees<AssetKey>,

    /// Timestamp of [`Trade`] that triggered the initial [`Position`] entry.
    pub time_enter: DateTime<Utc>,

    /// Timestamp of most recent [`Position`] update.
    ///
    /// Note this could be an update triggered by a [`Trade`], or a `pnl_unrealised` update by a
    /// new market price.
    pub time_exchange_update: DateTime<Utc>,

    /// [`TradeId`]s of all the [`Trade`]s associated with this [`Position`].
    pub trades: Vec<TradeId>,
}

impl<InstrumentKey> Position<QuoteAsset, InstrumentKey> {
    /// Updates the [`Position`] state based on a new [`Trade`].
    ///
    /// This method handles various scenarios:
    /// - Increasing an existing [`Position`] (same [`Side`] [`Trade`]).
    /// - Reducing an existing [`Position`] (opposite [`Side`], partially closing some quantity).
    /// - Closing a [`Position`] exactly (opposite [`Side`], fully closing quantity).
    /// - Flipping a [`Position`] - closing and opening a new [`Position`] on the opposite [`Side`].
    ///
    /// # Arguments
    /// * `trade` - The new trade to process
    ///
    /// # Returns
    /// A tuple containing:
    /// - `Option<Position>`: The updated [`Position`], unless it was exactly closed.
    /// - `Option<PositionExited>`: The closed [`PositionExited`], if the [`Position`] was closed.
    pub fn update_from_trade(
        mut self,
        trade: &Trade<QuoteAsset, InstrumentKey>,
    ) -> (
        Option<Self>,
        Option<PositionExited<QuoteAsset, InstrumentKey>>,
    )
    where
        InstrumentKey: Debug + Clone + PartialEq,
    {
        // Sanity check
        if self.instrument != trade.instrument {
            error!(
                position = ?self,
                trade = ?trade,
                "Position tried to be updated from a Trade for a different Instrument - ignoring"
            );
            return (Some(self), None);
        }

        // Add TradeId to current Position
        self.trades.push(trade.id.clone());

        use Side::*;
        match (self.side, trade.side) {
            // Increase LONG/SHORT Position
            (Buy, Buy) | (Sell, Sell) => {
                self.update_price_entry_average(trade);
                self.quantity_abs += trade.quantity.abs();
                if self.quantity_abs > self.quantity_abs_max {
                    self.quantity_abs_max = self.quantity_abs;
                }
                self.pnl_realised -= trade.fees.fees;
                self.fees_enter.fees += trade.fees.fees;
                self.time_exchange_update = trade.time_exchange;
                self.update_pnl_unrealised(trade.price);

                (Some(self), None)
            }
            // Reduce LONG/SHORT Position
            (Buy, Sell) | (Sell, Buy) if self.quantity_abs > trade.quantity.abs() => {
                // Update pnl_realised
                self.update_pnl_realised(trade.quantity, trade.price, trade.fees.fees);

                // Update remaining Position state
                self.quantity_abs -= trade.quantity.abs();
                self.fees_exit.fees += trade.fees.fees;
                self.time_exchange_update = trade.time_exchange;

                // Update pnl_unrealised for remaining Position
                self.update_pnl_unrealised(trade.price);

                (Some(self), None)
            }
            // Close LONG/SHORT Position (exactly)
            (Buy, Sell) | (Sell, Buy) if self.quantity_abs == trade.quantity.abs() => {
                self.quantity_abs -= trade.quantity.abs();
                self.fees_exit.fees += trade.fees.fees;
                self.time_exchange_update = trade.time_exchange;
                self.update_pnl_realised(trade.quantity, trade.price, trade.fees.fees);
                self.update_pnl_unrealised(trade.price);

                (None, Some(PositionExited::from(self)))
            }

            // Close LONG/SHORT Position & open SHORT/LONG with remaining trade.quantity
            (Buy, Sell) | (Sell, Buy) if self.quantity_abs < trade.quantity.abs() => {
                // Trade flips Position, so generate theoretical initial Trade for next Position
                let next_position_quantity = trade.quantity.abs() - self.quantity_abs;
                let next_position_fee_enter =
                    trade.fees.fees * (next_position_quantity / trade.quantity.abs());
                let next_position_trade = Trade {
                    id: trade.id.clone(),
                    order_id: trade.order_id.clone(),
                    instrument: trade.instrument.clone(),
                    strategy: trade.strategy.clone(),
                    time_exchange: trade.time_exchange,
                    side: trade.side,
                    price: trade.price,
                    quantity: next_position_quantity,
                    fees: AssetFees {
                        asset: trade.fees.asset.clone(),
                        fees: next_position_fee_enter,
                    },
                };

                // Update closing Position with appropriate ratio of fees for theoretical quantity
                let fee_exit = trade.fees.fees * (self.quantity_abs / trade.quantity.abs());
                self.fees_exit.fees += fee_exit;
                self.time_exchange_update = trade.time_exchange;
                self.update_pnl_realised(self.quantity_abs, trade.price, fee_exit);
                self.quantity_abs = Decimal::ZERO;
                self.update_pnl_unrealised(trade.price);

                (
                    Some(Self::from(&next_position_trade)),
                    Some(PositionExited::from(self)),
                )
            }
            _ => unreachable!("match expression guard statements cover all cases"),
        }
    }

    /// Updates the volume-weighted average entry price of the [`Position`].
    ///
    /// Internally uses the logic defined in [`calculate_price_entry_average`].
    fn update_price_entry_average(&mut self, trade: &Trade<QuoteAsset, InstrumentKey>) {
        self.price_entry_average = calculate_price_entry_average(
            self.price_entry_average,
            self.quantity_abs,
            trade.price,
            trade.quantity.abs(),
        );
    }

    /// Update [`Position::pnl_unrealised`](Position) with the estimated PnL from closing
    /// the [`Position`] at the provided price.
    ///
    /// Note that this could be called with a recent [`Trade`] price, or a price generated from
    /// a model based on public market data.
    pub fn update_pnl_unrealised(&mut self, price: Decimal) {
        self.pnl_unrealised = calculate_pnl_unrealised(
            self.side,
            self.price_entry_average,
            self.quantity_abs,
            self.quantity_abs_max,
            self.fees_enter.fees,
            price,
        );
    }

    /// Updates the [`Position`] `pnl_realised` from a closed portion of the [`Position`] quantity.
    pub fn update_pnl_realised(
        &mut self,
        closed_quantity: Decimal,
        closed_price: Decimal,
        closed_fee: Decimal,
    ) {
        // Update total Position pnl_realised with closed quantity PnL
        self.pnl_realised += calculate_pnl_realised(
            self.side,
            self.price_entry_average,
            closed_quantity,
            closed_price,
            closed_fee,
        );
    }

    /// Returns true if the cumulative realised and unrealised PnL is below the negative loss limit.
    pub fn is_loss_breached(&self, loss_limit: Decimal) -> bool {
        (self.pnl_realised + self.pnl_unrealised) < loss_limit
    }
}

impl<InstrumentKey> From<&Trade<QuoteAsset, InstrumentKey>> for Position<QuoteAsset, InstrumentKey>
where
    InstrumentKey: Clone,
{
    fn from(value: &Trade<QuoteAsset, InstrumentKey>) -> Self {
        Self {
            instrument: value.instrument.clone(),
            side: value.side,
            price_entry_average: value.price,
            quantity_abs: value.quantity.abs(),
            quantity_abs_max: value.quantity.abs(),
            pnl_unrealised: Decimal::ZERO,
            pnl_realised: -value.fees.fees,
            fees_enter: value.fees.clone(),
            fees_exit: AssetFees::quote_fees(Decimal::ZERO),
            time_enter: value.time_exchange,
            time_exchange_update: value.time_exchange,
            trades: vec![value.id.clone()],
        }
    }
}

/// Represents a fully closed trading [`Position`] for a specific instrument.
///
/// Contains the final state and history of a [`Position`] that has been completely closed.
///
/// # Type Parameters
/// - `AssetKey`: The type representing the asset used for fees (e.g. AssetIndex, QuoteAsset, etc.)
/// - `InstrumentKey`: The type identifying the traded instrument (e.g. InstrumentIndex, etc.)
#[derive(
    Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize, Constructor,
)]
pub struct PositionExited<AssetKey, InstrumentKey = InstrumentIndex> {
    /// Closed [`Position`] Instrument identifier (eg/ InstrumentIndex, InstrumentNameInternal, etc.).
    pub instrument: InstrumentKey,

    /// Closed [`Position`] direction (Side::Buy => LONG, Side::Sell => SHORT).
    pub side: Side,

    /// Volume-weighted average entry price across all [`Position`] increasing [`Trade`]s.
    pub price_entry_average: Decimal,

    /// Maximum absolute [`Position`] quantity reached by all entry/increase [`Trade`]s.
    pub quantity_abs_max: Decimal,

    /// Cumulative realised PnL from closing the full [`Position`] `quantity_abs_max`.
    ///
    /// Note this includes fees.
    pub pnl_realised: Decimal,

    /// Cumulative fees paid when entering the [`Position`].
    pub fees_enter: AssetFees<AssetKey>,

    /// Cumulative fees paid when exiting the [`Position`].
    pub fees_exit: AssetFees<AssetKey>,

    /// Timestamp of [`Trade`] that triggered the initial [`Position`] entry.
    pub time_enter: DateTime<Utc>,

    /// Timestamp of [`Trade`] that triggered the closing of the [`Position`].
    pub time_exit: DateTime<Utc>,

    /// [`TradeId`]s of all the [`Trade`]s associated with the closed [`Position`].
    pub trades: Vec<TradeId>,
}

impl<AssetKey, InstrumentKey> From<Position<AssetKey, InstrumentKey>>
    for PositionExited<AssetKey, InstrumentKey>
{
    fn from(value: Position<AssetKey, InstrumentKey>) -> Self {
        Self {
            instrument: value.instrument,
            side: value.side,
            price_entry_average: value.price_entry_average,
            quantity_abs_max: value.quantity_abs_max,
            pnl_realised: value.pnl_realised,
            fees_enter: value.fees_enter,
            fees_exit: value.fees_exit,
            time_enter: value.time_enter,
            time_exit: value.time_exchange_update,
            trades: value.trades,
        }
    }
}