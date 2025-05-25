//! Futures market modules for Kraken.

pub mod l2;
pub mod trade;
/// User WebSocket utilities.
pub mod user_ws;

pub use l2::{
    KrakenFuturesOrderBookL2, KrakenFuturesOrderBooksL2SnapshotFetcher,
    KrakenFuturesOrderBooksL2Transformer,
};
