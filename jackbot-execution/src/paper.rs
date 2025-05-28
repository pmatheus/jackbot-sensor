//! Simulated exchange engine used for paper trading.
//!
//! [`PaperEngine`] tracks balances, open orders and trade fills using provided
//! [`PaperBook`] order books. It exposes the same behavior as a real
//! [`ExecutionClient`], allowing strategies to evaluate performance without
//! interacting with live venues.
use crate::UnindexedAccountSnapshot;
use crate::{
    balance::AssetBalance,
    error::{ApiError, UnindexedOrderError},
    order::{Order, OrderKind, TimeInForce, id::OrderId, request::OrderRequestOpen},
    trade::{AssetFees, Trade, TradeId},
};
use chrono::{DateTime, Utc};
use fnv::FnvHashMap;
use jackbot_data::books::Level;
use jackbot_instrument::{
    Side, Underlying,
    asset::{QuoteAsset, name::AssetNameExchange},
    exchange::ExchangeId,
    instrument::{Instrument, name::InstrumentNameExchange},
};
use jackbot_integration::snapshot::Snapshot;
use rust_decimal::Decimal;
use smol_str::ToSmolStr;

#[derive(Debug, Clone)]
pub struct OpenOrderNotifications {
    pub balance: Snapshot<AssetBalance<AssetNameExchange>>,
    pub trade: Trade<QuoteAsset, InstrumentNameExchange>,
}

#[derive(Debug, Clone)]
pub struct PaperBook {
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}

impl PaperBook {
    pub fn new(bids: Vec<Level>, asks: Vec<Level>) -> Self {
        let mut bids = bids;
        bids.sort_by(|a, b| b.price.cmp(&a.price));
        let mut asks = asks;
        asks.sort_by(|a, b| a.price.cmp(&b.price));
        Self { bids, asks }
    }

    pub fn fill_market(&mut self, side: Side, mut quantity: Decimal) -> (Decimal, Decimal) {
        let mut total_value = Decimal::ZERO;
        let mut filled = Decimal::ZERO;
        match side {
            Side::Buy => {
                while quantity > Decimal::ZERO && !self.asks.is_empty() {
                    let lvl = &mut self.asks[0];
                    let trade_qty = quantity.min(lvl.amount);
                    total_value += trade_qty * lvl.price;
                    filled += trade_qty;
                    lvl.amount -= trade_qty;
                    quantity -= trade_qty;
                    if lvl.amount <= Decimal::ZERO {
                        self.asks.remove(0);
                    }
                }
            }
            Side::Sell => {
                while quantity > Decimal::ZERO && !self.bids.is_empty() {
                    let lvl = &mut self.bids[0];
                    let trade_qty = quantity.min(lvl.amount);
                    total_value += trade_qty * lvl.price;
                    filled += trade_qty;
                    lvl.amount -= trade_qty;
                    quantity -= trade_qty;
                    if lvl.amount <= Decimal::ZERO {
                        self.bids.remove(0);
                    }
                }
            }
        }
        let avg_price = if filled > Decimal::ZERO {
            total_value / filled
        } else {
            Decimal::ZERO
        };
        (filled, avg_price)
    }
}

#[derive(Debug)]
pub struct PaperEngine {
    pub exchange: ExchangeId,
    pub fees_percent: Decimal,
    pub instruments: FnvHashMap<InstrumentNameExchange, Instrument<ExchangeId, AssetNameExchange>>,
    pub books: FnvHashMap<InstrumentNameExchange, PaperBook>,
    pub account: UnindexedAccountSnapshot,
    order_sequence: u64,
}

impl PaperEngine {
    pub fn new(
        exchange: ExchangeId,
        fees_percent: Decimal,
        instruments: FnvHashMap<InstrumentNameExchange, Instrument<ExchangeId, AssetNameExchange>>,
        books: FnvHashMap<InstrumentNameExchange, PaperBook>,
        snapshot: crate::UnindexedAccountSnapshot,
    ) -> Self {
        Self {
            exchange,
            fees_percent,
            instruments,
            books,
            account: snapshot,
            order_sequence: 0,
        }
    }

    // Helper to get mutable balance or insert a default one
    fn get_or_insert_balance_mut(
        &mut self,
        asset_name: &AssetNameExchange,
        time_exchange: DateTime<Utc>,
    ) -> &mut AssetBalance<AssetNameExchange> {
        if let Some(pos) = self
            .account
            .balances
            .iter()
            .position(|ab| ab.asset == *asset_name)
        {
            self.account.balances[pos].time_exchange = time_exchange; // Update time
            &mut self.account.balances[pos]
        } else {
            self.account.balances.push(AssetBalance {
                asset: asset_name.clone(),
                balance: crate::balance::Balance::default(), // Assuming Balance has a Default
                time_exchange,
            });
            self.account.balances.last_mut().unwrap()
        }
    }

    pub fn open_order(
        &mut self,
        request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> (
        Order<
            ExchangeId,
            InstrumentNameExchange,
            Result<crate::order::state::Open, UnindexedOrderError>,
        >,
        Option<OpenOrderNotifications>,
    ) {
        if request.state.kind != OrderKind::Market {
            return (
                build_open_order_err_response(
                    request,
                    UnindexedOrderError::Rejected(ApiError::OrderRejected(
                        "PaperEngine only supports Market orders".to_owned(),
                    )),
                ),
                None,
            );
        }

        let instrument_key = request.key.instrument.clone();
        let instrument = match self.instruments.get(&instrument_key) {
            Some(inst) => inst,
            None => {
                return (
                    build_open_order_err_response(
                        request,
                        UnindexedOrderError::Rejected(ApiError::InstrumentInvalid(
                            instrument_key.clone(),
                            "unknown instrument".to_string(),
                        )),
                    ),
                    None,
                );
            }
        };

        let book = match self.books.get_mut(&instrument_key) {
            Some(b) => b,
            None => {
                return (
                    build_open_order_err_response(
                        request,
                        UnindexedOrderError::Rejected(ApiError::InstrumentInvalid(
                            instrument_key.clone(),
                            "missing orderbook".to_string(),
                        )),
                    ),
                    None,
                );
            }
        };

        let (filled_qty, avg_price) =
            book.fill_market(request.state.side, request.state.quantity.abs());

        if filled_qty == Decimal::ZERO {
            return (
                build_open_order_err_response(
                    request.clone(), // Clone request as it's partially moved
                    UnindexedOrderError::Rejected(ApiError::OrderRejected(
                        "No liquidity to fill market order".to_owned(),
                    )),
                ),
                None,
            );
        }

        let time_exchange = Utc::now();
        let underlying = instrument.underlying.clone();

        let updated_balance_for_notification: AssetBalance<AssetNameExchange>;
        let fees_paid: AssetFees<QuoteAsset>;

        match request.state.side {
            Side::Buy => {
                let quote_asset_name = &underlying.quote;
                let base_asset_name = &underlying.base;

                let order_value_quote = avg_price * filled_qty;
                // Assuming fees are paid in quote asset for buys
                let order_fees_quote = order_value_quote * self.fees_percent;
                let total_quote_deduction = order_value_quote + order_fees_quote;

                // Update Quote Asset Balance
                let quote_balance_entry =
                    self.get_or_insert_balance_mut(quote_asset_name, time_exchange);

                if quote_balance_entry.balance.free < total_quote_deduction {
                    return (
                        build_open_order_err_response(
                            request,
                            UnindexedOrderError::Rejected(ApiError::BalanceInsufficient(
                                quote_asset_name.clone(),
                                format!(
                                    "Available Quote Balance: {}, Required Quote Balance (incl. fees): {}",
                                    quote_balance_entry.balance.free, total_quote_deduction
                                ),
                            )),
                        ),
                        None,
                    );
                }
                quote_balance_entry.balance.free -= total_quote_deduction;
                quote_balance_entry.balance.total -= total_quote_deduction;
                // time_exchange already updated by get_or_insert_balance_mut

                updated_balance_for_notification = quote_balance_entry.clone(); // Clone the updated AssetBalance
                fees_paid = AssetFees::quote_fees(order_fees_quote);

                // Update Base Asset Balance
                let base_balance_entry =
                    self.get_or_insert_balance_mut(base_asset_name, time_exchange);
                base_balance_entry.balance.free += filled_qty;
                base_balance_entry.balance.total += filled_qty;
            }
            Side::Sell => {
                let base_asset_name = &underlying.base;
                let quote_asset_name = &underlying.quote; // For receiving proceeds

                // Assuming fees are paid from the received quote asset for sells
                // Sell `filled_qty` of base asset
                // Proceeds in quote asset: `avg_price * filled_qty`
                // Fees in quote asset: `(avg_price * filled_qty) * self.fees_percent`

                let base_balance_entry =
                    self.get_or_insert_balance_mut(base_asset_name, time_exchange);

                if base_balance_entry.balance.free < filled_qty {
                    return (
                        build_open_order_err_response(
                            request,
                            UnindexedOrderError::Rejected(ApiError::BalanceInsufficient(
                                base_asset_name.clone(),
                                format!(
                                    "Available Base Balance: {}, Required Base Balance: {}",
                                    base_balance_entry.balance.free, filled_qty
                                ),
                            )),
                        ),
                        None,
                    );
                }
                base_balance_entry.balance.free -= filled_qty;
                base_balance_entry.balance.total -= filled_qty;

                // Calculate proceeds and fees in quote asset
                let proceeds_quote = avg_price * filled_qty;
                let order_fees_quote = proceeds_quote * self.fees_percent;
                let net_proceeds_quote = proceeds_quote - order_fees_quote;

                // Update Quote Asset Balance (add net proceeds)
                let quote_balance_entry =
                    self.get_or_insert_balance_mut(quote_asset_name, time_exchange);
                quote_balance_entry.balance.free += net_proceeds_quote;
                quote_balance_entry.balance.total += net_proceeds_quote;

                updated_balance_for_notification = quote_balance_entry.clone();
                fees_paid = AssetFees::quote_fees(order_fees_quote);
            }
        };

        let order_id = self.order_id_sequence_fetch_add();
        let trade_id = TradeId::new(order_id.0.to_string()); // Use new constructor

        let order_response = Order {
            key: request.key.clone(),
            side: request.state.side,
            price: avg_price,
            quantity: filled_qty,
            kind: request.state.kind,
            time_in_force: TimeInForce::ImmediateOrCancel, // Paper engine fills market orders immediately or not at all
            state: Ok(crate::order::state::Open {
                id: order_id.clone(),
                time_exchange,
                filled_quantity: filled_qty, // For market orders, filled_quantity is the executed quantity
            }),
        };

        let notifications = OpenOrderNotifications {
            balance: Snapshot(updated_balance_for_notification),
            trade: Trade {
                id: trade_id,
                order_id: order_id.clone(), // Use the generated order_id
                instrument: instrument_key.clone(),
                strategy: request.key.strategy,
                time_exchange,
                side: request.state.side,
                price: avg_price,
                quantity: filled_qty,
                fees: fees_paid,
            },
        };

        (order_response, Some(notifications))
    }

    pub fn account_snapshot(&self) -> UnindexedAccountSnapshot {
        self.account.clone()
    }

    fn order_id_sequence_fetch_add(&mut self) -> OrderId {
        let sequence = self.order_sequence;
        self.order_sequence += 1;
        OrderId::new(sequence.to_smolstr())
    }
}

fn build_open_order_err_response<E>(
    request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    error: E,
) -> Order<ExchangeId, InstrumentNameExchange, Result<crate::order::state::Open, UnindexedOrderError>>
where
    E: Into<UnindexedOrderError>,
{
    Order {
        key: request.key,
        side: request.state.side,
        price: request.state.price,
        quantity: request.state.quantity,
        kind: request.state.kind,
        time_in_force: request.state.time_in_force,
        state: Err(error.into()),
    }
}
