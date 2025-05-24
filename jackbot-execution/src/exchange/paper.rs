use crate::{
    exchange::mock::account::AccountState,
    exchange::mock::OpenOrderNotifications,
    order::{id::OrderId, request::OrderRequestOpen, Order, OrderKind, TimeInForce},
    trade::{AssetFees, Trade, TradeId},
    error::{ApiError, UnindexedOrderError},
};
use jackbot_data::books::Level;
use jackbot_instrument::{
    asset::{QuoteAsset, name::AssetNameExchange},
    exchange::ExchangeId,
    instrument::{Instrument, name::InstrumentNameExchange},
    Side,
};
use jackbot_integration::snapshot::Snapshot;
use chrono::Utc;
use fnv::FnvHashMap;
use rust_decimal::Decimal;
use smol_str::ToSmolStr;
use crate::UnindexedAccountSnapshot;

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
        let avg_price = if filled > Decimal::ZERO { total_value / filled } else { Decimal::ZERO };
        (filled, avg_price)
    }
}

#[derive(Debug)]
pub struct PaperEngine {
    pub exchange: ExchangeId,
    pub fees_percent: Decimal,
    pub instruments: FnvHashMap<InstrumentNameExchange, Instrument<ExchangeId, AssetNameExchange>>,
    pub books: FnvHashMap<InstrumentNameExchange, PaperBook>,
    pub account: AccountState,
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
            account: AccountState::from(snapshot),
            order_sequence: 0,
        }
    }

    pub fn open_order(
        &mut self,
        request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> (
        Order<ExchangeId, InstrumentNameExchange, Result<crate::order::state::Open, UnindexedOrderError>>,
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

        let instrument = match self.instruments.get(&request.key.instrument) {
            Some(inst) => inst,
            None => {
                return (
                    build_open_order_err_response(
                        request,
                        UnindexedOrderError::Rejected(ApiError::InstrumentInvalid(
                            request.key.instrument,
                            "unknown instrument".to_string(),
                        )),
                    ),
                    None,
                )
            }
        };

        let book = match self.books.get_mut(&request.key.instrument) {
            Some(b) => b,
            None => {
                return (
                    build_open_order_err_response(
                        request,
                        UnindexedOrderError::Rejected(ApiError::InstrumentInvalid(
                            request.key.instrument,
                            "missing orderbook".to_string(),
                        )),
                    ),
                    None,
                )
            }
        };

        let (filled_qty, avg_price) = book.fill_market(request.state.side, request.state.quantity.abs());
        let time_exchange = Utc::now();

        let underlying = instrument.underlying.clone();
        let balance_change_result = match request.state.side {
            Side::Buy => {
                let current = self
                    .account
                    .balance_mut(&underlying.quote)
                    .expect("balance for quote asset");
                assert_eq!(current.balance.total, current.balance.free);
                let order_value_quote = avg_price * filled_qty;
                let order_fees_quote = order_value_quote * self.fees_percent;
                let quote_required = order_value_quote + order_fees_quote;
                let maybe_new_balance = current.balance.free - quote_required;
                if maybe_new_balance >= Decimal::ZERO {
                    current.balance.free = maybe_new_balance;
                    current.balance.total = maybe_new_balance;
                    current.time_exchange = time_exchange;
                    Ok((current.clone(), AssetFees::quote_fees(order_fees_quote)))
                } else {
                    Err(ApiError::BalanceInsufficient(
                        underlying.quote,
                        format!(
                            "Available Balance: {}, Required Balance inc. fees: {}",
                            current.balance.free, quote_required
                        ),
                    ))
                }
            }
            Side::Sell => {
                let current = self
                    .account
                    .balance_mut(&underlying.quote)
                    .expect("balance for quote asset");
                assert_eq!(current.balance.total, current.balance.free);
                let order_value_base = filled_qty;
                let order_fees_base = order_value_base * self.fees_percent;
                let base_required = order_value_base + order_fees_base;
                let maybe_new_balance = current.balance.free - base_required;
                if maybe_new_balance >= Decimal::ZERO {
                    current.balance.free = maybe_new_balance;
                    current.balance.total = maybe_new_balance;
                    current.time_exchange = time_exchange;
                    let fees_quote = order_fees_base * avg_price;
                    Ok((current.clone(), AssetFees::quote_fees(fees_quote)))
                } else {
                    Err(ApiError::BalanceInsufficient(
                        underlying.quote,
                        format!(
                            "Available Balance: {}, Required Balance inc. fees: {}",
                            current.balance.free, base_required
                        ),
                    ))
                }
            }
        };

        let (balance_snapshot, fees) = match balance_change_result {
            Ok((balance_snapshot, fees)) => (Snapshot(balance_snapshot), fees),
            Err(error) => return (build_open_order_err_response(request, error), None),
        };

        let order_id = self.order_id_sequence_fetch_add();
        let trade_id = TradeId(order_id.0.clone());

        let order_response = Order {
            key: request.key.clone(),
            side: request.state.side,
            price: avg_price,
            quantity: filled_qty,
            kind: request.state.kind,
            time_in_force: TimeInForce::ImmediateOrCancel,
            state: Ok(crate::order::state::Open {
                id: order_id.clone(),
                time_exchange,
                filled_quantity: filled_qty,
            }),
        };

        let notifications = OpenOrderNotifications {
            balance: balance_snapshot,
            trade: Trade {
                id: trade_id,
                order_id: order_id.clone(),
                instrument: request.key.instrument,
                strategy: request.key.strategy,
                time_exchange,
                side: request.state.side,
                price: avg_price,
                quantity: filled_qty,
                fees,
            },
        };

        (order_response, Some(notifications))
    }

    pub fn account_snapshot(&self) -> UnindexedAccountSnapshot {
        let balances = self.account.balances().cloned().collect();
        UnindexedAccountSnapshot {
            exchange: self.exchange,
            balances,
            instruments: Vec::new(),
        }
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
