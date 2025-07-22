//! Bybit exchange client implementation supporting Unified Trading API v5.
//! Supports both spot and futures trading with advanced order types.

pub mod rest;

// Re-export the main REST client
pub use rest::{BybitRestClient, BybitRestConfig};