//! Position management module for the trading engine.
//!
//! This module provides functionality for tracking and managing trading positions,
//! including opening, updating, and closing positions, as well as calculating
//! various metrics like P&L, returns, and fees.

mod calculations;
mod position_manager;
mod position_types;

#[cfg(test)]
mod tests;

// Re-export the main types and functions
pub use calculations::{
    calculate_pnl_realised, calculate_pnl_return, calculate_pnl_unrealised,
};
pub use position_manager::PositionManager;
pub use position_types::{Position, PositionExited};