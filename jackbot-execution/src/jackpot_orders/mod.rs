//! Jackpot order monitoring and automatic liquidation.
//!
//! This module tracks open jackpot positions and issues closing
//! market orders when the configured ticket loss is exceeded.

use crate::order::{
    OrderKey, OrderKind, TimeInForce,
    request::{OrderRequestOpen, RequestOpen},
};
use crate::trade::Trade;
use jackbot_instrument::{
    Side,
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Internal representation of an open jackpot position.
#[derive(Debug, Clone)]
struct MonitoredPosition {
    side: Side,
    entry_price: Decimal,
    quantity: Decimal,
    ticket_loss: Decimal,
    strategy: crate::order::id::StrategyId,
    cid: crate::order::id::ClientOrderId,
}

impl MonitoredPosition {
    /// Returns true if unrealised loss exceeds the ticket loss.
    fn loss_exceeded(&self, price: Decimal) -> bool {
        let value_current = price * self.quantity;
        let value_entry = self.entry_price * self.quantity;
        let pnl = match self.side {
            Side::Buy => value_current - value_entry,
            Side::Sell => value_entry - value_current,
        };
        pnl <= -self.ticket_loss
    }
}

/// Tracks jackpot positions and generates liquidation orders when necessary.
#[derive(Debug, Default)]
pub struct JackpotMonitor {
    positions: HashMap<InstrumentNameExchange, MonitoredPosition>,
}

impl JackpotMonitor {
    /// Start monitoring a new jackpot position based on the executed trade and configured ticket loss.
    pub fn record_trade(&mut self, trade: &Trade<crate::trade::QuoteAsset, InstrumentNameExchange>, ticket_loss: Decimal) {
        self.positions.insert(
            trade.instrument.clone(),
            MonitoredPosition {
                side: trade.side,
                entry_price: trade.price,
                quantity: trade.quantity.abs(),
                ticket_loss,
                strategy: trade.strategy.clone(),
                cid: trade.order_id.into(),
            },
        );
    }

    /// Update the latest price for an instrument. If the loss threshold is
    /// violated a market order request to close the position is returned.
    pub fn update_price(
        &mut self,
        exchange: ExchangeId,
        instrument: &InstrumentNameExchange,
        price: Decimal,
    ) -> Option<OrderRequestOpen<ExchangeId, InstrumentNameExchange>> {
        let pos = self.positions.get(instrument)?;
        if !pos.loss_exceeded(price) {
            return None;
        }

        let order = OrderRequestOpen {
            key: OrderKey {
                exchange,
                instrument: instrument.clone(),
                strategy: pos.strategy.clone(),
                cid: pos.cid.clone(),
            },
            state: RequestOpen {
                side: match pos.side {
                    Side::Buy => Side::Sell,
                    Side::Sell => Side::Buy,
                },
                price,
                quantity: pos.quantity,
                kind: OrderKind::Market,
                time_in_force: TimeInForce::ImmediateOrCancel,
            },
        };

        self.positions.remove(instrument);
        Some(order)
    }

    /// Returns true if no positions are being monitored.
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade::{AssetFees, TradeId};
    use crate::order::id::{ClientOrderId, OrderId};
    use chrono::{DateTime, Utc};
    use rust_decimal_macros::dec;

    #[test]
    fn test_liquidation_triggered() {
        let mut monitor = JackpotMonitor::default();
        let trade = Trade {
            id: TradeId::new("t"),
            order_id: OrderId::new("o"),
            instrument: InstrumentNameExchange::from("BTC-USDT"),
            strategy: crate::order::id::StrategyId::new("j"),
            time_exchange: DateTime::<Utc>::MIN_UTC,
            side: Side::Buy,
            price: dec!(100),
            quantity: dec!(1),
            fees: AssetFees::quote_fees(dec!(0)),
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
            strategy: crate::order::id::StrategyId::new("j"),
            time_exchange: DateTime::<Utc>::MIN_UTC,
            side: Side::Buy,
            price: dec!(100),
            quantity: dec!(1),
            fees: AssetFees::quote_fees(dec!(0)),
        };
        monitor.record_trade(&trade, dec!(10));
        assert!(monitor.update_price(ExchangeId::BinanceSpot, &trade.instrument, dec!(95)).is_none());
        assert!(!monitor.is_empty());
    }
}
