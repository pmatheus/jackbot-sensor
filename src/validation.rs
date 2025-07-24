use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;
use chrono::{DateTime, NaiveDateTime};

use crate::api::{
    ErrorCode, PlaceOrderRequest, OrderType,
    TickerData, OrderBookData, TradeData, KlineData
};

/// Data validation and sanitization utilities according to API contract
pub struct DataValidator {
    symbol_regex: Regex,
    max_string_length: usize,
    min_notional_value: f64,
    symbol_info_cache: Arc<std::sync::RwLock<HashMap<String, SymbolInfo>>>,
    normalizer: DataNormalizer,
}

/// Comprehensive data normalization system for standardizing data across exchanges
pub struct DataNormalizer {
    exchange_symbol_patterns: HashMap<String, ExchangeSymbolConfig>,
    timestamp_patterns: Vec<TimestampPattern>,
    precision_configs: HashMap<String, PrecisionConfig>,
}

#[derive(Debug, Clone)]
pub struct ExchangeSymbolConfig {
    pub format: SymbolFormat,
    pub separator: String,
    pub case_style: CaseStyle,
    pub common_quote_assets: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum SymbolFormat {
    Concatenated,  // BTCUSDT
    Separated,     // BTC-USD, BTC/USDT
    Underscore,    // BTC_USDT
}

#[derive(Debug, Clone)]
pub enum CaseStyle {
    Upper,
    Lower,
    Mixed,
}

#[derive(Debug, Clone)]
pub struct TimestampPattern {
    pub name: String,
    pub regex: Regex,
    pub parser: TimestampParser,
}

#[derive(Debug, Clone)]
pub enum TimestampParser {
    UnixSeconds,
    UnixMilliseconds,
    UnixMicroseconds,
    UnixNanoseconds,
    Iso8601,
    Rfc3339,
    Custom(String), // Format string
}

#[derive(Debug, Clone)]
pub struct PrecisionConfig {
    pub price_precision: u32,
    pub quantity_precision: u32,
    pub min_price_increment: f64,
    pub min_quantity_increment: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub status: String,
    pub min_quantity: f64,
    pub max_quantity: f64,
    pub quantity_precision: u32,
    pub min_price: f64,
    pub max_price: f64,
    pub price_precision: u32,
    pub min_notional: f64,
    pub tradable: bool,
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub code: ErrorCode,
    pub message: String,
    pub field: Option<String>,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", 
            serde_json::to_string(&self.code).unwrap_or_else(|_| "VALIDATION_ERROR".to_string()).trim_matches('"'),
            self.message
        )
    }
}

impl std::error::Error for ValidationError {}

impl DataNormalizer {
    pub fn new() -> Self {
        let mut exchange_configs = HashMap::new();
        
        // Binance: BTCUSDT
        exchange_configs.insert("binance".to_string(), ExchangeSymbolConfig {
            format: SymbolFormat::Concatenated,
            separator: "".to_string(),
            case_style: CaseStyle::Upper,
            common_quote_assets: vec![
                "USDT".to_string(), "USDC".to_string(), "USD".to_string(),
                "BTC".to_string(), "ETH".to_string(), "BNB".to_string(),
                "BUSD".to_string(), "EUR".to_string(), "GBP".to_string(),
                "AUD".to_string(), "TRY".to_string(), "TUSD".to_string(),
                "DAI".to_string()
            ],
        });
        
        // Coinbase: BTC-USD
        exchange_configs.insert("coinbase".to_string(), ExchangeSymbolConfig {
            format: SymbolFormat::Separated,
            separator: "-".to_string(),
            case_style: CaseStyle::Upper,
            common_quote_assets: vec![
                "USD".to_string(), "EUR".to_string(), "GBP".to_string(),
                "USDC".to_string(), "DAI".to_string(), "BTC".to_string(),
                "ETH".to_string()
            ],
        });
        
        // Bybit: BTCUSDT
        exchange_configs.insert("bybit".to_string(), ExchangeSymbolConfig {
            format: SymbolFormat::Concatenated,
            separator: "".to_string(),
            case_style: CaseStyle::Upper,
            common_quote_assets: vec![
                "USDT".to_string(), "USDC".to_string(), "USD".to_string(),
                "BTC".to_string(), "ETH".to_string()
            ],
        });
        
        // OKX: BTC-USDT
        exchange_configs.insert("okx".to_string(), ExchangeSymbolConfig {
            format: SymbolFormat::Separated,
            separator: "-".to_string(),
            case_style: CaseStyle::Upper,
            common_quote_assets: vec![
                "USDT".to_string(), "USDC".to_string(), "USD".to_string(),
                "BTC".to_string(), "ETH".to_string()
            ],
        });
        
        // Kraken: XBTUSD
        exchange_configs.insert("kraken".to_string(), ExchangeSymbolConfig {
            format: SymbolFormat::Concatenated,
            separator: "".to_string(),
            case_style: CaseStyle::Upper,
            common_quote_assets: vec![
                "USD".to_string(), "EUR".to_string(), "GBP".to_string(),
                "CAD".to_string(), "JPY".to_string(), "CHF".to_string(),
                "AUD".to_string(), "XBT".to_string(), "ETH".to_string(),
                "USDT".to_string(), "USDC".to_string()
            ],
        });
        
        // KuCoin: BTC-USDT
        exchange_configs.insert("kucoin".to_string(), ExchangeSymbolConfig {
            format: SymbolFormat::Separated,
            separator: "-".to_string(),
            case_style: CaseStyle::Upper,
            common_quote_assets: vec![
                "USDT".to_string(), "USDC".to_string(), "USD".to_string(),
                "BTC".to_string(), "ETH".to_string(), "KCS".to_string()
            ],
        });
        
        let timestamp_patterns = vec![
            TimestampPattern {
                name: "unix_milliseconds".to_string(),
                regex: Regex::new(r"^\d{13}$").unwrap(),
                parser: TimestampParser::UnixMilliseconds,
            },
            TimestampPattern {
                name: "unix_seconds".to_string(),
                regex: Regex::new(r"^\d{10}$").unwrap(),
                parser: TimestampParser::UnixSeconds,
            },
            TimestampPattern {
                name: "unix_microseconds".to_string(),
                regex: Regex::new(r"^\d{16}$").unwrap(),
                parser: TimestampParser::UnixMicroseconds,
            },
            TimestampPattern {
                name: "iso8601".to_string(),
                regex: Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z?$").unwrap(),
                parser: TimestampParser::Iso8601,
            },
            TimestampPattern {
                name: "rfc3339".to_string(),
                regex: Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?([+-]\d{2}:\d{2}|Z)$").unwrap(),
                parser: TimestampParser::Rfc3339,
            },
        ];
        
        let mut precision_configs = HashMap::new();
        
        // Default precision configs for major trading pairs
        precision_configs.insert("BTC/USDT".to_string(), PrecisionConfig {
            price_precision: 2,
            quantity_precision: 6,
            min_price_increment: 0.01,
            min_quantity_increment: 0.000001,
        });
        
        precision_configs.insert("ETH/USDT".to_string(), PrecisionConfig {
            price_precision: 2,
            quantity_precision: 5,
            min_price_increment: 0.01,
            min_quantity_increment: 0.00001,
        });
        
        // Default config for unknown pairs
        precision_configs.insert("DEFAULT".to_string(), PrecisionConfig {
            price_precision: 8,
            quantity_precision: 8,
            min_price_increment: 0.00000001,
            min_quantity_increment: 0.00000001,
        });
        
        Self {
            exchange_symbol_patterns: exchange_configs,
            timestamp_patterns,
            precision_configs,
        }
    }
    
    /// Normalize symbol from exchange format to standard format (BASE/QUOTE)
    pub fn normalize_symbol(&self, symbol: &str, exchange: Option<&str>) -> String {
        let symbol = symbol.trim();
        
        // Already in standard format
        if symbol.contains('/') {
            return symbol.to_uppercase();
        }
        
        // Handle Kraken special cases
        if let Some("kraken") = exchange {
            let normalized = self.normalize_kraken_symbol(symbol);
            if normalized.contains('/') {
                return normalized;
            }
        }
        
        // Get exchange config
        let config = if let Some(ex) = exchange {
            self.exchange_symbol_patterns.get(ex)
        } else {
            None
        };
        
        // Try format-specific normalization
        if let Some(config) = config {
            match config.format {
                SymbolFormat::Separated => {
                    if symbol.contains(&config.separator) {
                        return symbol.replace(&config.separator, "/").to_uppercase();
                    }
                },
                SymbolFormat::Concatenated => {
                    if let Some(result) = self.split_concatenated_symbol(symbol, &config.common_quote_assets) {
                        return result;
                    }
                },
                SymbolFormat::Underscore => {
                    if symbol.contains('_') {
                        return symbol.replace('_', "/").to_uppercase();
                    }
                }
            }
        }
        
        // Fallback: try common separators
        if symbol.contains('-') {
            return symbol.replace('-', "/").to_uppercase();
        }
        
        if symbol.contains('_') {
            return symbol.replace('_', "/").to_uppercase();
        }
        
        // Fallback: try to split concatenated format with common quote assets
        let common_quotes: Vec<String> = vec!["USDT", "USDC", "USD", "EUR", "BTC", "ETH", "BNB", "BUSD", "DAI"]
            .into_iter().map(|s| s.to_string()).collect();
        if let Some(result) = self.split_concatenated_symbol(symbol, &common_quotes) {
            return result;
        }
        
        // If all else fails, return uppercase
        symbol.to_uppercase()
    }
    
    /// Handle Kraken-specific symbol normalization
    fn normalize_kraken_symbol(&self, symbol: &str) -> String {
        let symbol = symbol.to_uppercase();
        
        // Kraken special mappings
        let kraken_mappings = HashMap::from([
            ("XBT", "BTC"),
            ("XDG", "DOGE"),
            ("XXBT", "BTC"),
            ("XXRP", "XRP"),
            ("XXLM", "XLM"),
            ("XETH", "ETH"),
            ("XETC", "ETC"),
            ("XLTC", "LTC"),
            ("XREP", "REP"),
            ("XXMR", "XMR"),
            ("XZEC", "ZEC"),
            ("ZUSD", "USD"),
            ("ZEUR", "EUR"),
            ("ZGBP", "GBP"),
            ("ZCAD", "CAD"),
            ("ZJPY", "JPY"),
            ("ZCHF", "CHF"),
            ("ZAUD", "AUD"),
        ]);
        
        // Try to split Kraken format
        let quotes = vec!["USD", "EUR", "GBP", "CAD", "JPY", "CHF", "AUD", "XBT", "ETH", "USDT", "USDC"];
        for quote in &quotes {
            let kraken_quote = format!("Z{}", quote);
            if symbol.ends_with(&kraken_quote) {
                let base_part = &symbol[..symbol.len() - kraken_quote.len()];
                let normalized_base = kraken_mappings.get(base_part).unwrap_or(&base_part);
                let normalized_quote = kraken_mappings.get(quote).unwrap_or(quote);
                return format!("{}/{}", normalized_base, normalized_quote);
            }
            
            if symbol.ends_with(quote) {
                let base_part = &symbol[..symbol.len() - quote.len()];
                let normalized_base = kraken_mappings.get(base_part).unwrap_or(&base_part);
                let normalized_quote = kraken_mappings.get(quote).unwrap_or(quote);
                return format!("{}/{}", normalized_base, normalized_quote);
            }
        }
        
        symbol
    }
    
    /// Split concatenated symbol format (e.g., BTCUSDT -> BTC/USDT)
    fn split_concatenated_symbol(&self, symbol: &str, quote_assets: &[String]) -> Option<String> {
        let symbol = symbol.to_uppercase();
        
        // Sort quote assets by length (longest first) to avoid partial matches
        let mut sorted_quotes = quote_assets.to_vec();
        sorted_quotes.sort_by(|a, b| b.len().cmp(&a.len()));
        
        for quote in sorted_quotes.iter() {
            if symbol.ends_with(quote) && symbol.len() > quote.len() {
                let base = &symbol[..symbol.len() - quote.len()];
                if !base.is_empty() && base.len() >= 2 { // Minimum 2 chars for base
                    return Some(format!("{}/{}", base, quote));
                }
            }
        }
        
        None
    }
    
    /// Normalize timestamp to Unix milliseconds
    pub fn normalize_timestamp(&self, timestamp_str: &str) -> Result<i64, ValidationError> {
        let timestamp_str = timestamp_str.trim();
        
        // Try to parse as number first
        if let Ok(num) = timestamp_str.parse::<i64>() {
            return self.normalize_timestamp_i64(num);
        }
        
        // Try to parse as float (some exchanges use decimal seconds)
        if let Ok(num) = timestamp_str.parse::<f64>() {
            let as_int = (num * 1000.0) as i64; // Convert to milliseconds
            return self.normalize_timestamp_i64(as_int);
        }
        
        // Try timestamp patterns
        for pattern in &self.timestamp_patterns {
            if pattern.regex.is_match(timestamp_str) {
                match &pattern.parser {
                    TimestampParser::Iso8601 | TimestampParser::Rfc3339 => {
                        if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp_str) {
                            return Ok(dt.timestamp_millis());
                        }
                        
                        // Try without timezone
                        if let Ok(ndt) = NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%dT%H:%M:%S%.fZ") {
                            return Ok(ndt.and_utc().timestamp_millis());
                        }
                        
                        if let Ok(ndt) = NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%dT%H:%M:%S") {
                            return Ok(ndt.and_utc().timestamp_millis());
                        }
                    },
                    TimestampParser::Custom(format) => {
                        if let Ok(ndt) = NaiveDateTime::parse_from_str(timestamp_str, format) {
                            return Ok(ndt.and_utc().timestamp_millis());
                        }
                    },
                    _ => {} // Numeric patterns handled above
                }
            }
        }
        
        Err(ValidationError {
            code: ErrorCode::ValidationError,
            message: format!("Unable to parse timestamp: {}", timestamp_str),
            field: Some("timestamp".to_string()),
        })
    }
    
    /// Normalize timestamp from i64 (auto-detect format)
    pub fn normalize_timestamp_i64(&self, timestamp: i64) -> Result<i64, ValidationError> {
        let timestamp_str = timestamp.to_string();
        
        match timestamp_str.len() {
            10 => Ok(timestamp * 1000), // Unix seconds to milliseconds
            13 => Ok(timestamp),        // Already milliseconds
            16 => Ok(timestamp / 1000), // Microseconds to milliseconds
            19 => Ok(timestamp / 1_000_000), // Nanoseconds to milliseconds
            _ => {
                // Check if it's a reasonable timestamp
                let now = chrono::Utc::now().timestamp_millis();
                let one_year_ms = 365 * 24 * 60 * 60 * 1000;
                
                if timestamp > now - one_year_ms && timestamp < now + one_year_ms {
                    Ok(timestamp) // Assume it's correct
                } else {
                    Err(ValidationError {
                        code: ErrorCode::ValidationError,
                        message: format!("Invalid timestamp format: {}", timestamp),
                        field: Some("timestamp".to_string()),
                    })
                }
            }
        }
    }
    
    /// Normalize price with proper precision
    pub fn normalize_price(&self, price: f64, symbol: &str, exchange: Option<&str>) -> Result<f64, ValidationError> {
        if price <= 0.0 || !price.is_finite() {
            return Err(ValidationError {
                code: ErrorCode::InvalidPrice,
                message: "Price must be a positive finite number".to_string(),
                field: Some("price".to_string()),
            });
        }
        
        let normalized_symbol = self.normalize_symbol(symbol, exchange);
        let config = self.precision_configs.get(&normalized_symbol)
            .or_else(|| self.precision_configs.get("DEFAULT"))
            .unwrap();
            
        let factor = 10_f64.powi(config.price_precision as i32);
        let normalized = (price * factor).round() / factor;
        
        // Ensure it meets minimum increment
        let increments = (normalized / config.min_price_increment).round();
        Ok(increments * config.min_price_increment)
    }
    
    /// Normalize quantity with proper precision
    pub fn normalize_quantity(&self, quantity: f64, symbol: &str, exchange: Option<&str>) -> Result<f64, ValidationError> {
        if quantity <= 0.0 || !quantity.is_finite() {
            return Err(ValidationError {
                code: ErrorCode::InvalidQuantity,
                message: "Quantity must be a positive finite number".to_string(),
                field: Some("quantity".to_string()),
            });
        }
        
        let normalized_symbol = self.normalize_symbol(symbol, exchange);
        let config = self.precision_configs.get(&normalized_symbol)
            .or_else(|| self.precision_configs.get("DEFAULT"))
            .unwrap();
            
        let factor = 10_f64.powi(config.quantity_precision as i32);
        let normalized = (quantity * factor).round() / factor;
        
        // Ensure it meets minimum increment
        let increments = (normalized / config.min_quantity_increment).round();
        Ok(increments * config.min_quantity_increment)
    }
}

impl DataNormalizer {
    /// Normalize ticker data to API contract format
    pub fn normalize_ticker_data(&self, ticker: &mut TickerData, source_exchange: &str) -> Result<(), ValidationError> {
        // Normalize symbol
        ticker.symbol = self.normalize_symbol(&ticker.symbol, Some(source_exchange));
        
        // Normalize timestamp
        ticker.timestamp = self.normalize_timestamp_i64(ticker.timestamp)?;
        
        // Normalize prices
        ticker.price = self.normalize_price(ticker.price, &ticker.symbol, Some(source_exchange))?;
        ticker.bid = self.normalize_price(ticker.bid, &ticker.symbol, Some(source_exchange))?;
        ticker.ask = self.normalize_price(ticker.ask, &ticker.symbol, Some(source_exchange))?;
        ticker.high_24h = self.normalize_price(ticker.high_24h, &ticker.symbol, Some(source_exchange))?;
        ticker.low_24h = self.normalize_price(ticker.low_24h, &ticker.symbol, Some(source_exchange))?;
        
        // Normalize volume
        ticker.volume_24h = self.normalize_quantity(ticker.volume_24h, &ticker.symbol, Some(source_exchange))?;
        
        // Normalize percentage (4 decimal places)
        ticker.change_24h = (ticker.change_24h * 10000.0).round() / 10000.0;
        
        Ok(())
    }
    
    /// Normalize order book data to API contract format
    pub fn normalize_orderbook_data(&self, orderbook: &mut OrderBookData, source_exchange: &str) -> Result<(), ValidationError> {
        // Normalize symbol
        orderbook.symbol = self.normalize_symbol(&orderbook.symbol, Some(source_exchange));
        
        // Normalize timestamp
        orderbook.timestamp = self.normalize_timestamp_i64(orderbook.timestamp)?;
        
        // Normalize bid prices and quantities
        for bid in &mut orderbook.bids {
            bid[0] = self.normalize_price(bid[0], &orderbook.symbol, Some(source_exchange))?;
            bid[1] = self.normalize_quantity(bid[1], &orderbook.symbol, Some(source_exchange))?;
        }
        
        // Normalize ask prices and quantities
        for ask in &mut orderbook.asks {
            ask[0] = self.normalize_price(ask[0], &orderbook.symbol, Some(source_exchange))?;
            ask[1] = self.normalize_quantity(ask[1], &orderbook.symbol, Some(source_exchange))?;
        }
        
        // Sort bids (highest price first) and asks (lowest price first)
        orderbook.bids.sort_by(|a, b| b[0].partial_cmp(&a[0]).unwrap());
        orderbook.asks.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
        
        Ok(())
    }
    
    /// Normalize trade data to API contract format
    pub fn normalize_trade_data(&self, trade: &mut TradeData, source_exchange: &str) -> Result<(), ValidationError> {
        // Normalize symbol
        trade.symbol = self.normalize_symbol(&trade.symbol, Some(source_exchange));
        
        // Normalize timestamp
        trade.timestamp = self.normalize_timestamp_i64(trade.timestamp)?;
        
        // Normalize price and quantity
        trade.price = self.normalize_price(trade.price, &trade.symbol, Some(source_exchange))?;
        trade.quantity = self.normalize_quantity(trade.quantity, &trade.symbol, Some(source_exchange))?;
        
        // Normalize side to lowercase
        trade.side = trade.side.to_lowercase();
        if trade.side != "buy" && trade.side != "sell" {
            return Err(ValidationError {
                code: ErrorCode::ValidationError,
                message: "Trade side must be 'buy' or 'sell'".to_string(),
                field: Some("side".to_string()),
            });
        }
        
        Ok(())
    }
    
    /// Normalize kline data to API contract format
    pub fn normalize_kline_data(&self, kline: &mut KlineData, source_exchange: &str) -> Result<(), ValidationError> {
        // Normalize symbol
        kline.symbol = self.normalize_symbol(&kline.symbol, Some(source_exchange));
        
        // Normalize timestamps
        kline.open_time = self.normalize_timestamp_i64(kline.open_time)?;
        kline.close_time = self.normalize_timestamp_i64(kline.close_time)?;
        
        // Normalize OHLC prices
        kline.open = self.normalize_price(kline.open, &kline.symbol, Some(source_exchange))?;
        kline.high = self.normalize_price(kline.high, &kline.symbol, Some(source_exchange))?;
        kline.low = self.normalize_price(kline.low, &kline.symbol, Some(source_exchange))?;
        kline.close = self.normalize_price(kline.close, &kline.symbol, Some(source_exchange))?;
        
        // Normalize volume
        kline.volume = self.normalize_quantity(kline.volume, &kline.symbol, Some(source_exchange))?;
        
        // Validate OHLC relationships
        if kline.high < kline.low {
            return Err(ValidationError {
                code: ErrorCode::ValidationError,
                message: "High price cannot be less than low price".to_string(),
                field: Some("ohlc".to_string()),
            });
        }
        
        if kline.open > kline.high || kline.open < kline.low {
            return Err(ValidationError {
                code: ErrorCode::ValidationError,
                message: "Open price must be between high and low".to_string(),
                field: Some("ohlc".to_string()),
            });
        }
        
        if kline.close > kline.high || kline.close < kline.low {
            return Err(ValidationError {
                code: ErrorCode::ValidationError,
                message: "Close price must be between high and low".to_string(),
                field: Some("ohlc".to_string()),
            });
        }
        
        Ok(())
    }
}

impl DataValidator {
    pub fn new() -> Self {
        Self {
            symbol_regex: Regex::new(r"^[A-Z0-9]+/[A-Z0-9]+$").unwrap(),
            max_string_length: 1000,
            min_notional_value: 10.0,
            symbol_info_cache: Arc::new(std::sync::RwLock::new(HashMap::new())),
            normalizer: DataNormalizer::new(),
        }
    }
    
    /// Validate and normalize symbol format with security checks
    pub fn validate_symbol(&self, symbol: &str) -> Result<String, ValidationError> {
        use crate::security::SecurityValidator;
        
        if symbol.is_empty() {
            return Err(ValidationError {
                code: ErrorCode::InvalidSymbol,
                message: "Symbol cannot be empty".to_string(),
                field: Some("symbol".to_string()),
            });
        }
        
        // Security validation first
        if let Err(e) = SecurityValidator::validate_database_input(symbol) {
            return Err(ValidationError {
                code: ErrorCode::InvalidSymbol,
                message: format!("Invalid symbol: {}", e),
                field: Some("symbol".to_string()),
            });
        }
        
        // Normalize symbol to standard format
        let normalized = self.normalize_symbol(symbol);
        
        if !self.symbol_regex.is_match(&normalized) {
            return Err(ValidationError {
                code: ErrorCode::InvalidSymbol,
                message: "Symbol must be in format BASE/QUOTE (e.g., BTC/USDT)".to_string(),
                field: Some("symbol".to_string()),
            });
        }
        
        Ok(normalized)
    }
    
    /// Normalize symbol from various exchange formats to standard format
    pub fn normalize_symbol(&self, symbol: &str) -> String {
        self.normalizer.normalize_symbol(symbol, None)
    }
    
    /// Normalize symbol with exchange context for better accuracy
    pub fn normalize_symbol_for_exchange(&self, symbol: &str, exchange: &str) -> String {
        self.normalizer.normalize_symbol(symbol, Some(exchange))
    }
    
    /// Normalize timestamp to Unix milliseconds
    pub fn normalize_timestamp(&self, timestamp: &str) -> Result<i64, ValidationError> {
        self.normalizer.normalize_timestamp(timestamp)
    }
    
    /// Normalize timestamp from i64 (auto-detect format)
    pub fn normalize_timestamp_i64(&self, timestamp: i64) -> Result<i64, ValidationError> {
        self.normalizer.normalize_timestamp_i64(timestamp)
    }
    
    /// Normalize price with proper precision
    pub fn normalize_price(&self, price: f64, symbol: &str, exchange: Option<&str>) -> Result<f64, ValidationError> {
        self.normalizer.normalize_price(price, symbol, exchange)
    }
    
    /// Normalize quantity with proper precision
    pub fn normalize_quantity(&self, quantity: f64, symbol: &str, exchange: Option<&str>) -> Result<f64, ValidationError> {
        self.normalizer.normalize_quantity(quantity, symbol, exchange)
    }
    
    /// Normalize ticker data to API contract format
    pub fn normalize_ticker_data(&self, ticker: &mut TickerData, source_exchange: &str) -> Result<(), ValidationError> {
        self.normalizer.normalize_ticker_data(ticker, source_exchange)
    }
    
    /// Normalize order book data to API contract format
    pub fn normalize_orderbook_data(&self, orderbook: &mut OrderBookData, source_exchange: &str) -> Result<(), ValidationError> {
        self.normalizer.normalize_orderbook_data(orderbook, source_exchange)
    }
    
    /// Normalize trade data to API contract format
    pub fn normalize_trade_data(&self, trade: &mut TradeData, source_exchange: &str) -> Result<(), ValidationError> {
        self.normalizer.normalize_trade_data(trade, source_exchange)
    }
    
    /// Normalize kline data to API contract format
    pub fn normalize_kline_data(&self, kline: &mut KlineData, source_exchange: &str) -> Result<(), ValidationError> {
        self.normalizer.normalize_kline_data(kline, source_exchange)
    }
    
    /// Validate timestamp format (must be Unix milliseconds)
    pub fn validate_timestamp(&self, timestamp: i64) -> Result<i64, ValidationError> {
        // First try to normalize the timestamp
        let normalized = self.normalize_timestamp_i64(timestamp)?;
        
        // Should be within reasonable range (not too far in past/future)
        let now = chrono::Utc::now().timestamp_millis();
        let one_year_ms = 365 * 24 * 60 * 60 * 1000;
        
        if normalized < now - one_year_ms || normalized > now + one_year_ms {
            return Err(ValidationError {
                code: ErrorCode::ValidationError,
                message: "Timestamp is outside reasonable range".to_string(),
                field: Some("timestamp".to_string()),
            });
        }
        
        Ok(normalized)
    }
    
    /// Validate numeric precision according to API contract
    pub fn validate_price_precision(&self, price: f64, symbol: &str) -> Result<f64, ValidationError> {
        if price <= 0.0 {
            return Err(ValidationError {
                code: ErrorCode::InvalidPrice,
                message: "Price must be greater than 0".to_string(),
                field: Some("price".to_string()),
            });
        }
        
        if !price.is_finite() {
            return Err(ValidationError {
                code: ErrorCode::InvalidPrice,
                message: "Price must be a finite number".to_string(),
                field: Some("price".to_string()),
            });
        }
        
        // Check maximum 8 decimal places as per API contract
        let price_str = format!("{:.8}", price);
        let decimal_places = price_str.split('.').nth(1).unwrap_or("").len();
        
        if decimal_places > 8 {
            return Err(ValidationError {
                code: ErrorCode::InvalidPrice,
                message: "Price precision cannot exceed 8 decimal places".to_string(),
                field: Some("price".to_string()),
            });
        }
        
        // Get symbol-specific precision if available
        if let Some(symbol_info) = self.get_symbol_info(symbol) {
            if price < symbol_info.min_price || price > symbol_info.max_price {
                return Err(ValidationError {
                    code: ErrorCode::InvalidPrice,
                    message: format!("Price must be between {} and {}", 
                        symbol_info.min_price, symbol_info.max_price),
                    field: Some("price".to_string()),
                });
            }
            
            // Round to symbol's price precision
            let precision = symbol_info.price_precision;
            let factor = 10_f64.powi(precision as i32);
            return Ok((price * factor).round() / factor);
        }
        
        Ok(price)
    }
    
    /// Validate quantity precision
    pub fn validate_quantity_precision(&self, quantity: f64, symbol: &str) -> Result<f64, ValidationError> {
        if quantity <= 0.0 {
            return Err(ValidationError {
                code: ErrorCode::InvalidQuantity,
                message: "Quantity must be greater than 0".to_string(),
                field: Some("quantity".to_string()),
            });
        }
        
        if !quantity.is_finite() {
            return Err(ValidationError {
                code: ErrorCode::InvalidQuantity,
                message: "Quantity must be a finite number".to_string(),
                field: Some("quantity".to_string()),
            });
        }
        
        // Get symbol-specific precision if available
        if let Some(symbol_info) = self.get_symbol_info(symbol) {
            if quantity < symbol_info.min_quantity || quantity > symbol_info.max_quantity {
                return Err(ValidationError {
                    code: ErrorCode::InvalidQuantity,
                    message: format!("Quantity must be between {} and {}", 
                        symbol_info.min_quantity, symbol_info.max_quantity),
                    field: Some("quantity".to_string()),
                });
            }
            
            // Round to symbol's quantity precision
            let precision = symbol_info.quantity_precision;
            let factor = 10_f64.powi(precision as i32);
            return Ok((quantity * factor).round() / factor);
        }
        
        Ok(quantity)
    }
    
    /// Validate percentage values (should be 4 decimal places max)
    pub fn validate_percentage(&self, percentage: f64) -> Result<f64, ValidationError> {
        if !percentage.is_finite() {
            return Err(ValidationError {
                code: ErrorCode::ValidationError,
                message: "Percentage must be a finite number".to_string(),
                field: Some("percentage".to_string()),
            });
        }
        
        // Round to 4 decimal places as per API contract
        Ok((percentage * 10000.0).round() / 10000.0)
    }
    
    /// Validate fiat values (should be 2 decimal places)
    pub fn validate_fiat_value(&self, value: f64) -> Result<f64, ValidationError> {
        if value < 0.0 {
            return Err(ValidationError {
                code: ErrorCode::ValidationError,
                message: "Fiat value cannot be negative".to_string(),
                field: Some("fiatValue".to_string()),
            });
        }
        
        if !value.is_finite() {
            return Err(ValidationError {
                code: ErrorCode::ValidationError,
                message: "Fiat value must be a finite number".to_string(),
                field: Some("fiatValue".to_string()),
            });
        }
        
        // Round to 2 decimal places as per API contract
        Ok((value * 100.0).round() / 100.0)
    }
    
    /// Comprehensive order validation
    pub fn validate_order(&self, order: &PlaceOrderRequest) -> Result<PlaceOrderRequest, ValidationError> {
        let mut validated_order = (*order).clone();
        
        // Validate and normalize symbol
        validated_order.symbol = self.validate_symbol(&order.symbol)?;
        
        // Validate quantity
        validated_order.quantity = self.validate_quantity_precision(order.quantity, &validated_order.symbol)?;
        
        // Validate price for limit orders
        if matches!(order.order_type, OrderType::Limit | OrderType::StopLimit) {
            match order.price {
                Some(price) => {
                    validated_order.price = Some(self.validate_price_precision(price, &validated_order.symbol)?);
                },
                None => {
                    return Err(ValidationError {
                        code: ErrorCode::InvalidPrice,
                        message: "Price is required for limit orders".to_string(),
                        field: Some("price".to_string()),
                    });
                }
            }
        }
        
        // Validate stop price for stop orders
        if matches!(order.order_type, OrderType::Stop | OrderType::StopLimit) {
            match order.stop_price {
                Some(stop_price) => {
                    validated_order.stop_price = Some(self.validate_price_precision(stop_price, &validated_order.symbol)?);
                },
                None => {
                    return Err(ValidationError {
                        code: ErrorCode::InvalidPrice,
                        message: "Stop price is required for stop orders".to_string(),
                        field: Some("stopPrice".to_string()),
                    });
                }
            }
        }
        
        // Validate minimum notional value
        let notional = match validated_order.price {
            Some(price) => price * validated_order.quantity,
            None => 50000.0 * validated_order.quantity, // Assume market price for market orders
        };
        
        let min_notional = self.get_symbol_info(&validated_order.symbol)
            .map(|info| info.min_notional)
            .unwrap_or(self.min_notional_value);
        
        if notional < min_notional {
            return Err(ValidationError {
                code: ErrorCode::ValidationError,
                message: format!("Order value must be at least ${:.2}", min_notional),
                field: Some("notional".to_string()),
            });
        }
        
        // Validate exchange
        validated_order.exchange = self.sanitize_string(&order.exchange);
        if validated_order.exchange.is_empty() {
            return Err(ValidationError {
                code: ErrorCode::ValidationError,
                message: "Exchange is required".to_string(),
                field: Some("exchange".to_string()),
            });
        }
        
        // Sanitize client order ID if provided
        if let Some(client_order_id) = &order.client_order_id {
            validated_order.client_order_id = Some(self.sanitize_string(client_order_id));
        }
        
        Ok(validated_order)
    }
    
    /// Sanitize string input to prevent XSS and injection attacks
    pub fn sanitize_string(&self, input: &str) -> String {
        input
            .chars()
            .filter(|c| !c.is_control() && *c != '<' && *c != '>' && *c != '"' && *c != '\'')
            .take(self.max_string_length)
            .collect::<String>()
            .trim()
            .to_string()
    }
    
    /// Validate channel name for WebSocket subscriptions
    pub fn validate_channel(&self, channel: &str, user_id: Option<&str>) -> Result<String, ValidationError> {
        let parts: Vec<&str> = channel.split(':').collect();
        
        if parts.is_empty() {
            return Err(ValidationError {
                code: ErrorCode::ValidationError,
                message: "Channel format is invalid".to_string(),
                field: Some("channel".to_string()),
            });
        }
        
        match parts[0] {
            "ticker" | "orderbook" | "trades" | "klines" => {
                if parts.len() < 3 {
                    return Err(ValidationError {
                        code: ErrorCode::ValidationError,
                        message: "Market data channels require format: type:symbol:exchange".to_string(),
                        field: Some("channel".to_string()),
                    });
                }
                
                // Validate symbol
                let _normalized_symbol = self.validate_symbol(parts[1])?;
                
                // Validate exchange
                let exchange = self.sanitize_string(parts[2]);
                if exchange.is_empty() {
                    return Err(ValidationError {
                        code: ErrorCode::ValidationError,
                        message: "Exchange cannot be empty".to_string(),
                        field: Some("channel".to_string()),
                    });
                }
            },
            "orders" | "positions" | "balances" | "alerts" => {
                if parts.len() < 2 {
                    return Err(ValidationError {
                        code: ErrorCode::ValidationError,
                        message: "Account channels require format: type:user_id".to_string(),
                        field: Some("channel".to_string()),
                    });
                }
                
                // Validate user authorization
                if user_id.is_none() || user_id.unwrap() != parts[1] {
                    return Err(ValidationError {
                        code: ErrorCode::Forbidden,
                        message: "Not authorized to access this channel".to_string(),
                        field: Some("channel".to_string()),
                    });
                }
            },
            _ => {
                return Err(ValidationError {
                    code: ErrorCode::ValidationError,
                    message: "Unknown channel type".to_string(),
                    field: Some("channel".to_string()),
                });
            }
        }
        
        Ok(channel.to_string())
    }
    
    /// Get symbol information from cache or default
    fn get_symbol_info(&self, symbol: &str) -> Option<SymbolInfo> {
        // Try to get info for normalized symbol first
        let normalized = self.normalize_symbol(symbol);
        if let Some(info) = self.symbol_info_cache.read().ok()?.get(&normalized).cloned() {
            return Some(info);
        }
        
        // Fallback to original symbol
        self.symbol_info_cache.read().ok()?.get(symbol).cloned()
    }
    
    /// Get the underlying normalizer for advanced operations
    pub fn get_normalizer(&self) -> &DataNormalizer {
        &self.normalizer
    }
    
    /// Update symbol information cache
    pub fn update_symbol_info(&self, symbol_info: SymbolInfo) {
        if let Ok(mut cache) = self.symbol_info_cache.write() {
            cache.insert(symbol_info.symbol.clone(), symbol_info);
        }
    }
    
    /// Load default symbol information for common pairs
    pub fn load_default_symbols(&self) {
        let default_symbols = vec![
            SymbolInfo {
                symbol: "BTC/USDT".to_string(),
                base_asset: "BTC".to_string(),
                quote_asset: "USDT".to_string(),
                status: "TRADING".to_string(),
                min_quantity: 0.00001,
                max_quantity: 9000.0,
                quantity_precision: 5,
                min_price: 0.01,
                max_price: 1000000.0,
                price_precision: 2,
                min_notional: 10.0,
                tradable: true,
            },
            SymbolInfo {
                symbol: "ETH/USDT".to_string(),
                base_asset: "ETH".to_string(),
                quote_asset: "USDT".to_string(),
                status: "TRADING".to_string(),
                min_quantity: 0.0001,
                max_quantity: 10000.0,
                quantity_precision: 4,
                min_price: 0.01,
                max_price: 100000.0,
                price_precision: 2,
                min_notional: 10.0,
                tradable: true,
            },
        ];
        
        for symbol_info in default_symbols {
            self.update_symbol_info(symbol_info);
        }
        
        debug!("Loaded default symbol information");
    }
}

impl Default for DataValidator {
    fn default() -> Self {
        let validator = Self::new();
        validator.load_default_symbols();
        validator
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{OrderType, OrderSide, TimeInForce};
    
    #[test]
    fn test_symbol_validation() {
        let validator = DataValidator::new();
        
        // Valid symbols
        assert!(validator.validate_symbol("BTC/USDT").is_ok());
        assert!(validator.validate_symbol("ETH/BTC").is_ok());
        
        // Invalid symbols
        assert!(validator.validate_symbol("").is_err());
        assert!(validator.validate_symbol("BTC").is_err());
        assert!(validator.validate_symbol("BTC/").is_err());
        assert!(validator.validate_symbol("/USDT").is_err());
    }
    
    #[test]
    fn test_symbol_normalization() {
        let normalizer = DataNormalizer::new();
        
        // Standard format (should remain unchanged)
        assert_eq!(normalizer.normalize_symbol("BTC/USDT", None), "BTC/USDT");
        assert_eq!(normalizer.normalize_symbol("ETH/BTC", None), "ETH/BTC");
        
        // Dash separator
        assert_eq!(normalizer.normalize_symbol("BTC-USD", Some("coinbase")), "BTC/USD");
        assert_eq!(normalizer.normalize_symbol("ETH-EUR", Some("coinbase")), "ETH/EUR");
        
        // Concatenated format
        assert_eq!(normalizer.normalize_symbol("BTCUSDT", Some("binance")), "BTC/USDT");
        assert_eq!(normalizer.normalize_symbol("ETHBTC", Some("binance")), "ETH/BTC");
        assert_eq!(normalizer.normalize_symbol("ADAUSDT", Some("binance")), "ADA/USDT");
        assert_eq!(normalizer.normalize_symbol("BNBBUSD", Some("binance")), "BNB/BUSD");
        
        // Underscore separator
        assert_eq!(normalizer.normalize_symbol("BTC_USDT", None), "BTC/USDT");
        assert_eq!(normalizer.normalize_symbol("ETH_BTC", None), "ETH/BTC");
        
        // Kraken special cases
        assert_eq!(normalizer.normalize_symbol("XBTUSD", Some("kraken")), "BTC/USD");
        assert_eq!(normalizer.normalize_symbol("XXBTZUSD", Some("kraken")), "BTC/USD");
        assert_eq!(normalizer.normalize_symbol("XETHZEUR", Some("kraken")), "ETH/EUR");
        
        // Edge cases
        assert_eq!(normalizer.normalize_symbol("btcusdt", Some("binance")), "BTC/USDT");
        assert_eq!(normalizer.normalize_symbol(" BTC-USD ", Some("coinbase")), "BTC/USD");
    }
    
    #[test]
    fn test_symbol_normalization_fallback() {
        let normalizer = DataNormalizer::new();
        
        // Without exchange context, should still work for common formats
        assert_eq!(normalizer.normalize_symbol("BTC-USD", None), "BTC/USD");
        assert_eq!(normalizer.normalize_symbol("BTCUSDT", None), "BTC/USDT");
        assert_eq!(normalizer.normalize_symbol("ETH_USDC", None), "ETH/USDC");
    }
    
    #[test]
    fn test_timestamp_normalization() {
        let normalizer = DataNormalizer::new();
        
        // Unix seconds to milliseconds
        assert_eq!(normalizer.normalize_timestamp_i64(1640995200).unwrap(), 1640995200000);
        
        // Already in milliseconds
        assert_eq!(normalizer.normalize_timestamp_i64(1640995200000).unwrap(), 1640995200000);
        
        // Microseconds to milliseconds
        assert_eq!(normalizer.normalize_timestamp_i64(1640995200000000).unwrap(), 1640995200000);
        
        // Nanoseconds to milliseconds
        assert_eq!(normalizer.normalize_timestamp_i64(1640995200000000000).unwrap(), 1640995200000);
        
        // String timestamps
        assert!(normalizer.normalize_timestamp("1640995200").is_ok());
        assert!(normalizer.normalize_timestamp("1640995200000").is_ok());
        assert!(normalizer.normalize_timestamp("1640995200.123").is_ok());
        
        // ISO 8601 / RFC 3339
        assert!(normalizer.normalize_timestamp("2022-01-01T00:00:00Z").is_ok());
        assert!(normalizer.normalize_timestamp("2022-01-01T00:00:00.123Z").is_ok());
        assert!(normalizer.normalize_timestamp("2022-01-01T00:00:00+00:00").is_ok());
        
        // Invalid timestamps
        assert!(normalizer.normalize_timestamp("invalid").is_err());
        assert!(normalizer.normalize_timestamp("").is_err());
    }
    
    #[test]
    fn test_price_normalization() {
        let normalizer = DataNormalizer::new();
        
        // BTC/USDT precision (2 decimal places)
        assert_eq!(normalizer.normalize_price(50000.123456, "BTC/USDT", Some("binance")).unwrap(), 50000.12);
        assert_eq!(normalizer.normalize_price(50000.126, "BTC/USDT", Some("binance")).unwrap(), 50000.13);
        
        // ETH/USDT precision (2 decimal places)
        assert_eq!(normalizer.normalize_price(3000.567, "ETH/USDT", Some("binance")).unwrap(), 3000.57);
        
        // Invalid prices
        assert!(normalizer.normalize_price(0.0, "BTC/USDT", Some("binance")).is_err());
        assert!(normalizer.normalize_price(-100.0, "BTC/USDT", Some("binance")).is_err());
        assert!(normalizer.normalize_price(f64::NAN, "BTC/USDT", Some("binance")).is_err());
        assert!(normalizer.normalize_price(f64::INFINITY, "BTC/USDT", Some("binance")).is_err());
    }
    
    #[test]
    fn test_quantity_normalization() {
        let normalizer = DataNormalizer::new();
        
        // BTC/USDT quantity precision (6 decimal places)
        assert_eq!(normalizer.normalize_quantity(1.1234567, "BTC/USDT", Some("binance")).unwrap(), 1.123457);
        assert_eq!(normalizer.normalize_quantity(0.0000012345, "BTC/USDT", Some("binance")).unwrap(), 0.000001);
        
        // ETH/USDT quantity precision (5 decimal places)
        assert_eq!(normalizer.normalize_quantity(10.123456, "ETH/USDT", Some("binance")).unwrap(), 10.12346);
        
        // Invalid quantities
        assert!(normalizer.normalize_quantity(0.0, "BTC/USDT", Some("binance")).is_err());
        assert!(normalizer.normalize_quantity(-1.0, "BTC/USDT", Some("binance")).is_err());
        assert!(normalizer.normalize_quantity(f64::NAN, "BTC/USDT", Some("binance")).is_err());
    }
    
    #[test]
    fn test_ticker_data_normalization() {
        let normalizer = DataNormalizer::new();
        
        let mut ticker = TickerData {
            symbol: "BTCUSDT".to_string(),
            exchange: "binance".to_string(),
            price: 50000.123456,
            bid: 49999.876543,
            ask: 50000.234567,
            volume_24h: 1234.5678901234,
            change_24h: 5.123456789,
            high_24h: 51000.987654,
            low_24h: 49000.123456,
            timestamp: 1640995200,
        };
        
        assert!(normalizer.normalize_ticker_data(&mut ticker, "binance").is_ok());
        
        assert_eq!(ticker.symbol, "BTC/USDT");
        assert_eq!(ticker.timestamp, 1640995200000);
        assert_eq!(ticker.price, 50000.12);
        assert_eq!(ticker.bid, 49999.88);
        assert_eq!(ticker.ask, 50000.23);
        assert_eq!(ticker.volume_24h, 1234.567890);
        assert_eq!(ticker.change_24h, 5.1235); // 4 decimal places
        assert_eq!(ticker.high_24h, 51000.99);
        assert_eq!(ticker.low_24h, 49000.12);
    }
    
    #[test]
    fn test_orderbook_data_normalization() {
        let normalizer = DataNormalizer::new();
        
        let mut orderbook = OrderBookData {
            symbol: "BTCUSDT".to_string(),
            exchange: "binance".to_string(),
            bids: vec![
                [49999.123, 1.234567],
                [49998.456, 2.345678],
            ],
            asks: vec![
                [50001.789, 0.987654],
                [50000.654, 1.876543],
            ],
            timestamp: 1640995200,
            sequence_id: Some(12345),
        };
        
        assert!(normalizer.normalize_orderbook_data(&mut orderbook, "binance").is_ok());
        
        assert_eq!(orderbook.symbol, "BTC/USDT");
        assert_eq!(orderbook.timestamp, 1640995200000);
        
        // Check bid normalization and sorting (highest first)
        assert_eq!(orderbook.bids[0][0], 49999.12);
        assert_eq!(orderbook.bids[0][1], 1.234567);
        assert_eq!(orderbook.bids[1][0], 49998.46);
        
        // Check ask normalization and sorting (lowest first)
        assert_eq!(orderbook.asks[0][0], 50000.65);
        assert_eq!(orderbook.asks[1][0], 50001.79);
    }
    
    #[test]
    fn test_trade_data_normalization() {
        let normalizer = DataNormalizer::new();
        
        let mut trade = TradeData {
            symbol: "BTCUSDT".to_string(),
            exchange: "binance".to_string(),
            id: "12345".to_string(),
            price: 50000.123456,
            quantity: 1.234567890,
            side: "BUY".to_string(),
            timestamp: 1640995200,
            is_maker: false,
        };
        
        assert!(normalizer.normalize_trade_data(&mut trade, "binance").is_ok());
        
        assert_eq!(trade.symbol, "BTC/USDT");
        assert_eq!(trade.timestamp, 1640995200000);
        assert_eq!(trade.price, 50000.12);
        assert_eq!(trade.quantity, 1.234568);
        assert_eq!(trade.side, "buy");
        
        // Test invalid side
        trade.side = "invalid".to_string();
        assert!(normalizer.normalize_trade_data(&mut trade, "binance").is_err());
    }
    
    #[test]
    fn test_kline_data_normalization() {
        let normalizer = DataNormalizer::new();
        
        let mut kline = KlineData {
            symbol: "BTCUSDT".to_string(),
            exchange: "binance".to_string(),
            interval: "1h".to_string(),
            open_time: 1640995200,
            close_time: 1640998800,
            open: 50000.123,
            high: 50500.456,
            low: 49500.789,
            close: 50200.012,
            volume: 1234.567890,
            trades: 1000,
            is_final: true,
        };
        
        assert!(normalizer.normalize_kline_data(&mut kline, "binance").is_ok());
        
        assert_eq!(kline.symbol, "BTC/USDT");
        assert_eq!(kline.open_time, 1640995200000);
        assert_eq!(kline.close_time, 1640998800000);
        assert_eq!(kline.open, 50000.12);
        assert_eq!(kline.high, 50500.46);
        assert_eq!(kline.low, 49500.79);
        assert_eq!(kline.close, 50200.01);
        assert_eq!(kline.volume, 1234.567890);
        
        // Test invalid OHLC relationships
        kline.high = 49000.0; // High < Low
        assert!(normalizer.normalize_kline_data(&mut kline, "binance").is_err());
        
        kline.high = 51000.0;
        kline.open = 52000.0; // Open > High
        assert!(normalizer.normalize_kline_data(&mut kline, "binance").is_err());
    }
    
    #[test]
    fn test_timestamp_validation() {
        let validator = DataValidator::new();
        let now = chrono::Utc::now().timestamp_millis();
        
        // Valid timestamp
        assert!(validator.validate_timestamp(now).is_ok());
        
        // Invalid timestamps
        assert!(validator.validate_timestamp(1234567890).is_err()); // 10 digits
        assert!(validator.validate_timestamp(now + 500 * 24 * 60 * 60 * 1000).is_err()); // Too far in future
    }
    
    #[test]
    fn test_price_validation() {
        let validator = DataValidator::default();
        
        // Valid prices
        assert!(validator.validate_price_precision(100.12345678, "BTC/USDT").is_ok());
        assert!(validator.validate_price_precision(0.01, "BTC/USDT").is_ok());
        
        // Invalid prices
        assert!(validator.validate_price_precision(0.0, "BTC/USDT").is_err());
        assert!(validator.validate_price_precision(-100.0, "BTC/USDT").is_err());
        assert!(validator.validate_price_precision(f64::NAN, "BTC/USDT").is_err());
    }
    
    #[test]
    fn test_order_validation() {
        let validator = DataValidator::default();
        
        let valid_order = PlaceOrderRequest {
            exchange: "binance".to_string(),
            symbol: "BTC/USDT".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            quantity: 0.001,
            price: Some(50000.0),
            stop_price: None,
            time_in_force: Some(TimeInForce::GoodTillCancelled),
            client_order_id: None,
            reduce_only: None,
            post_only: None,
        };
        
        assert!(validator.validate_order(&valid_order).is_ok());
        
        // Invalid order - no price for limit order
        let invalid_order = PlaceOrderRequest {
            price: None,
            ..valid_order.clone()
        };
        
        assert!(validator.validate_order(&invalid_order).is_err());
    }
    
    #[test]
    fn test_comprehensive_symbol_normalization() {
        let normalizer = DataNormalizer::new();
        
        // Test all major exchange formats
        let test_cases = vec![
            // Binance
            ("BTCUSDT", Some("binance"), "BTC/USDT"),
            ("ETHUSDC", Some("binance"), "ETH/USDC"),
            ("ADABTC", Some("binance"), "ADA/BTC"),
            
            // Coinbase
            ("BTC-USD", Some("coinbase"), "BTC/USD"),
            ("ETH-EUR", Some("coinbase"), "ETH/EUR"),
            ("LTC-BTC", Some("coinbase"), "LTC/BTC"),
            
            // OKX
            ("BTC-USDT", Some("okx"), "BTC/USDT"),
            ("ETH-USD", Some("okx"), "ETH/USD"),
            
            // Kraken
            ("XBTUSD", Some("kraken"), "BTC/USD"),
            ("XXBTZUSD", Some("kraken"), "BTC/USD"),
            ("ETHUSD", Some("kraken"), "ETH/USD"),
            
            // KuCoin
            ("BTC-USDT", Some("kucoin"), "BTC/USDT"),
            ("ETH-KCS", Some("kucoin"), "ETH/KCS"),
            
            // Without exchange context
            ("BTC-USD", None, "BTC/USD"),
            ("BTCUSDT", None, "BTC/USDT"),
            ("BTC_USDT", None, "BTC/USDT"),
        ];
        
        for (input, exchange, expected) in test_cases {
            let result = normalizer.normalize_symbol(input, exchange);
            assert_eq!(result, expected, "Failed for input: {} with exchange: {:?}", input, exchange);
        }
    }
    
    #[test]
    fn test_edge_cases() {
        let normalizer = DataNormalizer::new();
        
        // Empty and whitespace
        assert_eq!(normalizer.normalize_symbol("", None), "");
        assert_eq!(normalizer.normalize_symbol("   ", None), "");
        
        // Already standard format
        assert_eq!(normalizer.normalize_symbol("BTC/USDT", Some("binance")), "BTC/USDT");
        
        // Mixed case
        assert_eq!(normalizer.normalize_symbol("btc-usd", Some("coinbase")), "BTC/USD");
        assert_eq!(normalizer.normalize_symbol("btcusdt", Some("binance")), "BTC/USDT");
        
        // Long quote assets (should match longest first)
        assert_eq!(normalizer.normalize_symbol("COMPUSDT", Some("binance")), "COMP/USDT");
        assert_eq!(normalizer.normalize_symbol("USDCUSDT", Some("binance")), "USDC/USDT");
    }
}