//! Exchange connector implementations for Jackbot Sensor
//!
//! This module provides implementations of the Exchange trait for all supported exchanges,
//! bridging the gap between the jackbot-execution clients and the sensor's unified interface.

use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

pub mod binance;
pub mod bitget;
pub mod bingx;
pub mod bybit;
pub mod coinbase;
pub mod coinbase_production;
pub mod gateio;
pub mod hyperliquid;
pub mod kraken;
pub mod kucoin;
pub mod mexc;
pub mod okx;

// Re-export the Exchange trait and related types
pub use crate::connector::{Exchange, Connection, OrderResult, Order, OrderId, Balance};

/// Supported exchanges enum matching the specification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportedExchange {
    Binance,      // Spot & Futures
    Coinbase,     // Institutional grade
    Bybit,        // Derivatives focus
    Bitget,       // Copy trading
    Hyperliquid,  // On-chain perps
    KuCoin,       // Wide selection
    Kraken,       // Regulated
    OKX,          // Comprehensive
    Gateio,       // Top 10 global exchange
    MEXC,         // Fast-growing exchange
    BingX,        // Rising exchange with competitive fees
}

impl SupportedExchange {
    /// Get the exchange name as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            SupportedExchange::Binance => "binance",
            SupportedExchange::Coinbase => "coinbase",
            SupportedExchange::Bybit => "bybit",
            SupportedExchange::Bitget => "bitget",
            SupportedExchange::Hyperliquid => "hyperliquid",
            SupportedExchange::KuCoin => "kucoin",
            SupportedExchange::Kraken => "kraken",
            SupportedExchange::OKX => "okx",
            SupportedExchange::Gateio => "gateio",
            SupportedExchange::MEXC => "mexc",
            SupportedExchange::BingX => "bingx",
        }
    }

    /// Convert to jackbot_instrument ExchangeId
    pub fn to_exchange_id(&self) -> jackbot_instrument::exchange::ExchangeId {
        match self {
            SupportedExchange::Binance => jackbot_instrument::exchange::ExchangeId::BinanceSpot,
            SupportedExchange::Coinbase => jackbot_instrument::exchange::ExchangeId::Coinbase,
            SupportedExchange::Bybit => jackbot_instrument::exchange::ExchangeId::BybitPerpetualsUsd,
            SupportedExchange::Bitget => jackbot_instrument::exchange::ExchangeId::Bitget,
            SupportedExchange::Hyperliquid => jackbot_instrument::exchange::ExchangeId::Hyperliquid,
            SupportedExchange::KuCoin => jackbot_instrument::exchange::ExchangeId::Kucoin,
            SupportedExchange::Kraken => jackbot_instrument::exchange::ExchangeId::Kraken,
            SupportedExchange::OKX => jackbot_instrument::exchange::ExchangeId::Okx,
            // TODO: Add proper ExchangeId mappings when available in jackbot_instrument
            SupportedExchange::Gateio => jackbot_instrument::exchange::ExchangeId::Binance, // Temporary mapping
            SupportedExchange::MEXC => jackbot_instrument::exchange::ExchangeId::Binance, // Temporary mapping
            SupportedExchange::BingX => jackbot_instrument::exchange::ExchangeId::Binance, // Temporary mapping
        }
    }
}

/// Factory function to create exchange connectors
pub fn create_connector(
    exchange: SupportedExchange,
    api_key: Option<String>,
    api_secret: Option<String>,
    sandbox: bool,
) -> Result<Box<dyn Exchange>> {
    match exchange {
        SupportedExchange::Binance => {
            Ok(Box::new(binance::BinanceConnector::new(api_key, api_secret, sandbox)?))
        }
        SupportedExchange::Coinbase => {
            if sandbox {
                Ok(Box::new(coinbase::CoinbaseConnector::new(api_key.clone(), api_secret.clone(), sandbox)?))
            } else {
                // Use production-optimized connector for real trading
                Ok(Box::new(coinbase_production::CoinbaseProductionConnector::new(
                    api_key, 
                    api_secret, 
                    None // API passphrase will be handled internally
                )?))
            }
        }
        SupportedExchange::Bybit => {
            Ok(Box::new(bybit::BybitConnector::new(api_key, api_secret, sandbox)?))
        }
        SupportedExchange::Bitget => {
            Ok(Box::new(bitget::BitgetConnector::new(api_key, api_secret, sandbox)?))
        }
        SupportedExchange::Hyperliquid => {
            Ok(Box::new(hyperliquid::HyperliquidConnector::new(api_key, api_secret, sandbox)?))
        }
        SupportedExchange::KuCoin => {
            Ok(Box::new(kucoin::KuCoinConnector::new(api_key, api_secret, sandbox)?))
        }
        SupportedExchange::Kraken => {
            Ok(Box::new(kraken::KrakenConnector::new(api_key, api_secret, sandbox)?))
        }
        SupportedExchange::OKX => {
            Ok(Box::new(okx::OKXConnector::new(api_key, api_secret, sandbox)?))
        }
        SupportedExchange::Gateio => {
            Ok(Box::new(gateio::GateioConnector::new(api_key, api_secret, sandbox)?))
        }
        SupportedExchange::MEXC => {
            Ok(Box::new(mexc::MexcConnector::new(api_key, api_secret, sandbox)?))
        }
        SupportedExchange::BingX => {
            Ok(Box::new(bingx::BingXConnector::new(api_key, api_secret, sandbox)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_exchange_conversion() {
        assert_eq!(SupportedExchange::Binance.as_str(), "binance");
        assert_eq!(SupportedExchange::Coinbase.as_str(), "coinbase");
        
        // Test ExchangeId conversion
        assert_eq!(
            SupportedExchange::Binance.to_exchange_id(),
            jackbot_instrument::exchange::ExchangeId::BinanceSpot
        );
    }

    #[tokio::test]
    async fn test_create_connector() {
        // Test creating connectors without credentials (sandbox mode)
        for exchange in [
            SupportedExchange::Binance,
            SupportedExchange::Coinbase,
            SupportedExchange::Bybit,
            SupportedExchange::Bitget,
            SupportedExchange::Hyperliquid,
            SupportedExchange::KuCoin,
            SupportedExchange::Kraken,
            SupportedExchange::OKX,
            SupportedExchange::Gateio,
            SupportedExchange::MEXC,
            SupportedExchange::BingX,
        ] {
            let result = create_connector(exchange, None, None, true);
            assert!(result.is_ok(), "Failed to create {} connector", exchange.as_str());
        }
    }
}