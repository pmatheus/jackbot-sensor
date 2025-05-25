//! Gate.io exchange module.

/// Spot market modules for Gate.io.
pub mod spot;
/// Futures market modules for Gate.io.
pub mod futures;
/// Trade event types for Gate.io.
pub mod trade;
/// Rate limiting utilities for Gate.io.
pub mod rate_limit;

use crate::exchange::DEFAULT_HEARTBEAT_INTERVAL;
use std::time::Duration;

/// Default heartbeat interval for Gate.io WebSocket streams.
pub const HEARTBEAT_INTERVAL_GATEIO: Duration = DEFAULT_HEARTBEAT_INTERVAL;
