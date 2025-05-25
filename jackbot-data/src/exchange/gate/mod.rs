//! Gate.io exchange module.
//!
//! This module contains spot and futures market submodules for Gate.io.

pub mod spot;
pub mod futures;
pub mod trade;

use crate::exchange::DEFAULT_HEARTBEAT_INTERVAL;
use std::time::Duration;

/// Default heartbeat interval for Gate.io WebSocket streams.
pub const HEARTBEAT_INTERVAL_GATEIO: Duration = DEFAULT_HEARTBEAT_INTERVAL;
