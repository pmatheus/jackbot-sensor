//! Exchange identifier and related types

use serde::{Deserialize, Serialize};

/// Exchange identifier enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum ExchangeId {
    // Spot exchanges
    #[serde(rename = "binance_spot")]
    BinanceSpot,
    #[serde(rename = "coinbase")]
    Coinbase,
    #[serde(rename = "kraken")]
    Kraken,
    #[serde(rename = "kucoin")]
    Kucoin,
    #[serde(rename = "gateio")]
    Gateio,
    #[serde(rename = "mexc")]
    Mexc,
    #[serde(rename = "cryptocom")]
    Cryptocom,
    
    // Futures/Derivatives exchanges  
    #[serde(rename = "binance_futures_usd")]
    BinanceFuturesUsd,
    #[serde(rename = "bybit_spot")]
    BybitSpot,
    #[serde(rename = "bybit_perpetuals_usd")]
    BybitPerpetualsUsd,
    #[serde(rename = "bitget")]
    Bitget,
    #[serde(rename = "bitget_spot")]
    BitgetSpot,
    #[serde(rename = "bitget_futures")]
    BitgetFutures,
    #[serde(rename = "hyperliquid")]
    Hyperliquid,
    #[serde(rename = "okx")]
    Okx,
}

impl std::fmt::Display for ExchangeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExchangeId::BinanceSpot => write!(f, "binance_spot"),
            ExchangeId::BinanceFuturesUsd => write!(f, "binance_futures_usd"),
            ExchangeId::Coinbase => write!(f, "coinbase"),
            ExchangeId::BybitSpot => write!(f, "bybit_spot"),
            ExchangeId::BybitPerpetualsUsd => write!(f, "bybit_perpetuals_usd"),
            ExchangeId::Bitget => write!(f, "bitget"),
            ExchangeId::BitgetSpot => write!(f, "bitget_spot"),
            ExchangeId::BitgetFutures => write!(f, "bitget_futures"),
            ExchangeId::Hyperliquid => write!(f, "hyperliquid"),
            ExchangeId::Kucoin => write!(f, "kucoin"),
            ExchangeId::Kraken => write!(f, "kraken"),
            ExchangeId::Okx => write!(f, "okx"),
            ExchangeId::Gateio => write!(f, "gateio"),
            ExchangeId::Mexc => write!(f, "mexc"),
            ExchangeId::Cryptocom => write!(f, "cryptocom"),
        }
    }
}

impl ExchangeId {
    /// Get all supported exchange IDs
    pub fn all() -> Vec<ExchangeId> {
        vec![
            ExchangeId::BinanceSpot,
            ExchangeId::BinanceFuturesUsd,
            ExchangeId::Coinbase,
            ExchangeId::BybitSpot,
            ExchangeId::BybitPerpetualsUsd,
            ExchangeId::Bitget,
            ExchangeId::BitgetSpot,
            ExchangeId::BitgetFutures,
            ExchangeId::Hyperliquid,
            ExchangeId::Kucoin,
            ExchangeId::Kraken,
            ExchangeId::Okx,
            ExchangeId::Gateio,
            ExchangeId::Mexc,
            ExchangeId::Cryptocom,
        ]
    }
    
    /// Get the string representation of the exchange ID
    pub fn as_str(&self) -> &'static str {
        match self {
            ExchangeId::BinanceSpot => "binance_spot",
            ExchangeId::BinanceFuturesUsd => "binance_futures_usd",
            ExchangeId::Coinbase => "coinbase",
            ExchangeId::BybitSpot => "bybit_spot",
            ExchangeId::BybitPerpetualsUsd => "bybit_perpetuals_usd",
            ExchangeId::Bitget => "bitget",
            ExchangeId::BitgetSpot => "bitget_spot",
            ExchangeId::BitgetFutures => "bitget_futures",
            ExchangeId::Hyperliquid => "hyperliquid",
            ExchangeId::Kucoin => "kucoin",
            ExchangeId::Kraken => "kraken",
            ExchangeId::Okx => "okx",
            ExchangeId::Gateio => "gateio",
            ExchangeId::Mexc => "mexc",
            ExchangeId::Cryptocom => "cryptocom",
        }
    }
}