//! # Jackbot Instrument
//! 
//! Instrument and exchange definitions for the Jackbot trading system.
//! Provides normalized types for trading pairs, exchanges, and market data.

pub mod exchange;
pub mod instrument;
pub mod index;
pub mod asset;

use serde::{Deserialize, Serialize};

/// Trait for types that can be keyed/indexed
pub trait Keyed {
    type Key;
    fn key(&self) -> &Self::Key;
}

/// Trading side enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Side {
    #[serde(rename = "buy")]
    Buy,
    #[serde(rename = "sell")]
    Sell,
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Side::Buy => write!(f, "buy"),
            Side::Sell => write!(f, "sell"),
        }
    }
}