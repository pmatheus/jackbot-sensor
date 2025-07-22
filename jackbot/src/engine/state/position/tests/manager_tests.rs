//! Tests for PositionManager.

use crate::engine::state::position::PositionManager;
use crate::test_utils::trade;
use chrono::{DateTime, Utc};
use jackbot_instrument::instrument::name::InstrumentNameInternal;
use jackbot_instrument::Side;
use rust_decimal_macros::dec;

#[test]
fn test_position_manager_default() {
    let manager: PositionManager<InstrumentNameInternal> = PositionManager::default();
    assert!(manager.current.is_none());
}

#[test]
fn test_position_manager_update_from_trade() {
    let mut manager: PositionManager<InstrumentNameInternal> = PositionManager::default();
    let base_time = DateTime::<Utc>::MIN_UTC;
    
    // First trade should create a new position
    let trade1 = trade(base_time, Side::Buy, 100.0, 1.0, 10.0);
    let closed = manager.update_from_trade(&trade1);
    assert!(closed.is_none());
    assert!(manager.current.is_some());
    
    let position = manager.current.as_ref().unwrap();
    assert_eq!(position.side, Side::Buy);
    assert_eq!(position.quantity_abs, dec!(1.0));
    assert_eq!(position.price_entry_average, dec!(100.0));
    
    // Second trade (opposite side, partial close) should reduce position
    let trade2 = trade(base_time, Side::Sell, 120.0, 0.5, 5.0);
    let closed = manager.update_from_trade(&trade2);
    assert!(closed.is_none());
    assert!(manager.current.is_some());
    
    let position = manager.current.as_ref().unwrap();
    assert_eq!(position.quantity_abs, dec!(0.5));
    
    // Third trade (opposite side, exact close) should close position
    let trade3 = trade(base_time, Side::Sell, 130.0, 0.5, 5.0);
    let closed = manager.update_from_trade(&trade3);
    assert!(closed.is_some());
    assert!(manager.current.is_none());
    
    let exited = closed.unwrap();
    assert_eq!(exited.side, Side::Buy);
    assert_eq!(exited.quantity_abs_max, dec!(1.0));
}

#[test]
fn test_position_manager_flip() {
    let mut manager: PositionManager<InstrumentNameInternal> = PositionManager::default();
    let base_time = DateTime::<Utc>::MIN_UTC;
    
    // Create a long position
    let trade1 = trade(base_time, Side::Buy, 100.0, 1.0, 10.0);
    manager.update_from_trade(&trade1);
    
    // Flip with a larger opposite trade
    let trade2 = trade(base_time, Side::Sell, 110.0, 2.0, 20.0);
    let closed = manager.update_from_trade(&trade2);
    
    assert!(closed.is_some());
    assert!(manager.current.is_some());
    
    // Check closed position
    let exited = closed.unwrap();
    assert_eq!(exited.side, Side::Buy);
    assert_eq!(exited.quantity_abs_max, dec!(1.0));
    
    // Check new position
    let position = manager.current.as_ref().unwrap();
    assert_eq!(position.side, Side::Sell);
    assert_eq!(position.quantity_abs, dec!(1.0));
    assert_eq!(position.price_entry_average, dec!(110.0));
}