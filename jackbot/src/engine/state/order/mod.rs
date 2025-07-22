/// Order management module for tracking order lifecycle.
///
/// This module provides a comprehensive order management system that tracks orders
/// through their various states from placement to completion.

pub mod in_flight_recorder;
pub mod jackpot;
pub mod manager;
pub mod orders;
pub mod prophetic;

#[cfg(test)]
mod tests;

// Re-export the main Orders struct and traits
pub use orders::Orders;
pub use in_flight_recorder::InFlightRequestRecorder;
pub use manager::OrderManager;