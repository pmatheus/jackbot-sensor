use std::collections::HashMap;
use std::sync::LazyLock;

/// Comprehensive symbol mapping for Kraken's unique asset naming system
/// 
/// Kraken uses a complex naming convention:
/// - Major cryptocurrencies prefixed with 'X' (XXBT for BTC, XETH for ETH)
/// - Major fiat currencies prefixed with 'Z' (ZUSD for USD, ZEUR for EUR)
/// - Some assets have different names entirely
/// - WebSocket and REST APIs may use different formats

pub struct KrakenSymbolMapper;

impl KrakenSymbolMapper {
    /// Convert standard asset name to Kraken's REST API format
    pub fn standard_to_kraken_asset(asset: &str) -> String {
        STANDARD_TO_KRAKEN_ASSETS.get(asset)
            .map(|s| s.to_string())
            .unwrap_or_else(|| asset.to_string())
    }

    /// Convert Kraken's asset name to standard format
    pub fn kraken_to_standard_asset(kraken_asset: &str) -> String {
        KRAKEN_TO_STANDARD_ASSETS.get(kraken_asset)
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                // Handle automatic conversion for X/Z prefixed assets
                if kraken_asset.len() == 4 {
                    match kraken_asset.chars().next() {
                        Some('X') | Some('Z') => kraken_asset[1..].to_string(),
                        _ => kraken_asset.to_string(),
                    }
                } else {
                    kraken_asset.to_string()
                }
            })
    }

    /// Convert standard trading pair to Kraken's REST API format
    pub fn standard_to_kraken_pair(symbol: &str) -> String {
        STANDARD_TO_KRAKEN_PAIRS.get(symbol)
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                // Fallback: try to construct from known assets
                if let Some((base, quote)) = parse_symbol(symbol) {
                    let kraken_base = Self::standard_to_kraken_asset(&base);
                    let kraken_quote = Self::standard_to_kraken_asset(&quote);
                    format!("{}{}", kraken_base, kraken_quote)
                } else {
                    symbol.to_string()
                }
            })
    }

    /// Convert Kraken's trading pair to standard format
    pub fn kraken_to_standard_pair(kraken_symbol: &str) -> String {
        KRAKEN_TO_STANDARD_PAIRS.get(kraken_symbol)
            .map(|s| s.to_string())
            .unwrap_or_else(|| kraken_symbol.to_string())
    }

    /// Convert standard trading pair to Kraken's WebSocket format (with slash)
    pub fn standard_to_kraken_ws_pair(symbol: &str) -> String {
        if let Some(kraken_pair) = STANDARD_TO_KRAKEN_WS_PAIRS.get(symbol) {
            kraken_pair.to_string()
        } else if let Some((base, quote)) = parse_symbol(symbol) {
            let kraken_base = Self::standard_to_kraken_asset(&base);
            let kraken_quote = Self::standard_to_kraken_asset(&quote);
            format!("{}/{}", kraken_base, kraken_quote)
        } else {
            symbol.to_string()
        }
    }

    /// Convert Kraken's WebSocket pair (with slash) to standard format
    pub fn kraken_ws_to_standard_pair(kraken_ws_symbol: &str) -> String {
        KRAKEN_WS_TO_STANDARD_PAIRS.get(kraken_ws_symbol)
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                // Remove slash and convert
                let no_slash = kraken_ws_symbol.replace('/', "");
                Self::kraken_to_standard_pair(&no_slash)
            })
    }

    /// Get decimal precision for a Kraken asset
    pub fn get_asset_precision(asset: &str) -> u32 {
        ASSET_PRECISION.get(asset).copied().unwrap_or(8)
    }

    /// Get lot size (minimum order size) for a Kraken pair
    pub fn get_lot_size(pair: &str) -> &'static str {
        LOT_SIZES.get(pair).unwrap_or(&"0.00000001")
    }

    /// Get tick size (price increment) for a Kraken pair
    pub fn get_tick_size(pair: &str) -> &'static str {
        TICK_SIZES.get(pair).unwrap_or(&"0.1")
    }

    /// Check if an asset is a major cryptocurrency in Kraken's system
    pub fn is_major_crypto(asset: &str) -> bool {
        matches!(asset, "BTC" | "ETH" | "LTC" | "XRP" | "ADA" | "DOT" | "LINK" | "XLM" | "XMR" | "ZEC")
    }

    /// Check if an asset is a fiat currency in Kraken's system
    pub fn is_fiat_currency(asset: &str) -> bool {
        matches!(asset, "USD" | "EUR" | "GBP" | "JPY" | "CAD" | "CHF" | "AUD")
    }
}

// Parse a symbol like "BTCUSD" into ("BTC", "USD")
fn parse_symbol(symbol: &str) -> Option<(String, String)> {
    // Common quote currencies
    const QUOTE_CURRENCIES: &[&str] = &["USD", "EUR", "GBP", "JPY", "CAD", "CHF", "AUD", "USDT", "USDC"];
    
    for &quote in QUOTE_CURRENCIES {
        if symbol.ends_with(quote) && symbol.len() > quote.len() {
            let base = &symbol[..symbol.len() - quote.len()];
            return Some((base.to_string(), quote.to_string()));
        }
    }
    
    None
}

// Static mappings for efficient lookups
static STANDARD_TO_KRAKEN_ASSETS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("BTC", "XXBT"),
        ("ETH", "XETH"),
        ("LTC", "XLTC"),
        ("XRP", "XXRP"),
        ("XLM", "XXLM"),
        ("XMR", "XXMR"),
        ("ZEC", "XZEC"),
        ("USD", "ZUSD"),
        ("EUR", "ZEUR"),
        ("GBP", "ZGBP"),
        ("JPY", "ZJPY"),
        ("CAD", "ZCAD"),
        ("CHF", "ZCHF"),
        ("AUD", "ZAUD"),
        // Newer assets typically don't have prefixes
        ("ADA", "ADA"),
        ("DOT", "DOT"),
        ("LINK", "LINK"),
        ("USDT", "USDT"),
        ("USDC", "USDC"),
    ])
});

static KRAKEN_TO_STANDARD_ASSETS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    STANDARD_TO_KRAKEN_ASSETS.iter()
        .map(|(&standard, &kraken)| (kraken, standard))
        .collect()
});

static STANDARD_TO_KRAKEN_PAIRS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("BTCUSD", "XBTUSD"),
        ("BTCEUR", "XBTEUR"),
        ("BTCGBP", "XBTGBP"),
        ("BTCJPY", "XBTJPY"),
        ("BTCCAD", "XBTCAD"),
        ("BTCCHF", "XBTCHF"),
        ("BTCAUD", "XBTAUD"),
        ("ETHUSD", "ETHUSD"),
        ("ETHEUR", "ETHEUR"),
        ("ETHGBP", "ETHGBP"),
        ("ETHJPY", "ETHJPY"),
        ("ETHCAD", "ETHCAD"),
        ("ETHCHF", "ETHCHF"),
        ("ETHAUD", "ETHAUD"),
        ("ETHBTC", "ETHXBT"),
        ("LTCUSD", "LTCUSD"),
        ("LTCEUR", "LTCEUR"),
        ("LTCBTC", "LTCXBT"),
        ("XRPUSD", "XRPUSD"),
        ("XRPEUR", "XRPEUR"),
        ("XRPBTC", "XRPXBT"),
        ("ADAUSD", "ADAUSD"),
        ("ADAEUR", "ADAEUR"),
        ("ADABTC", "ADAXBT"),
        ("DOTUSD", "DOTUSD"),
        ("DOTEUR", "DOTEUR"),
        ("DOTBTC", "DOTXBT"),
    ])
});

static KRAKEN_TO_STANDARD_PAIRS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    STANDARD_TO_KRAKEN_PAIRS.iter()
        .map(|(&standard, &kraken)| (kraken, standard))
        .collect()
});

static STANDARD_TO_KRAKEN_WS_PAIRS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("BTCUSD", "XBT/USD"),
        ("BTCEUR", "XBT/EUR"),
        ("BTCGBP", "XBT/GBP"),
        ("BTCJPY", "XBT/JPY"),
        ("BTCCAD", "XBT/CAD"),
        ("BTCCHF", "XBT/CHF"),
        ("BTCAUD", "XBT/AUD"),
        ("ETHUSD", "ETH/USD"),
        ("ETHEUR", "ETH/EUR"),
        ("ETHGBP", "ETH/GBP"),
        ("ETHJPY", "ETH/JPY"),
        ("ETHCAD", "ETH/CAD"),
        ("ETHCHF", "ETH/CHF"),
        ("ETHAUD", "ETH/AUD"),
        ("ETHBTC", "ETH/XBT"),
        ("LTCUSD", "LTC/USD"),
        ("LTCEUR", "LTC/EUR"),
        ("LTCBTC", "LTC/XBT"),
        ("XRPUSD", "XRP/USD"),
        ("XRPEUR", "XRP/EUR"),
        ("XRPBTC", "XRP/XBT"),
        ("ADAUSD", "ADA/USD"),
        ("ADAEUR", "ADA/EUR"),
        ("ADABTC", "ADA/XBT"),
        ("DOTUSD", "DOT/USD"),
        ("DOTEUR", "DOT/EUR"),
        ("DOTBTC", "DOT/XBT"),
    ])
});

static KRAKEN_WS_TO_STANDARD_PAIRS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    STANDARD_TO_KRAKEN_WS_PAIRS.iter()
        .map(|(&standard, &kraken_ws)| (kraken_ws, standard))
        .collect()
});

static ASSET_PRECISION: LazyLock<HashMap<&'static str, u32>> = LazyLock::new(|| {
    HashMap::from([
        ("BTC", 8),
        ("ETH", 10),
        ("LTC", 8),
        ("XRP", 8),
        ("ADA", 8),
        ("DOT", 10),
        ("LINK", 8),
        ("XLM", 8),
        ("XMR", 8),
        ("ZEC", 8),
        ("USD", 2),
        ("EUR", 2),
        ("GBP", 2),
        ("JPY", 0),
        ("CAD", 2),
        ("CHF", 2),
        ("AUD", 2),
        ("USDT", 8),
        ("USDC", 8),
    ])
});

static LOT_SIZES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("BTCUSD", "0.0001"),
        ("BTCEUR", "0.0001"),
        ("ETHUSD", "0.001"),
        ("ETHEUR", "0.001"),
        ("ETHBTC", "0.001"),
        ("LTCUSD", "0.01"),
        ("LTCEUR", "0.01"),
        ("LTCBTC", "0.01"),
        ("XRPUSD", "1"),
        ("XRPEUR", "1"),
        ("XRPBTC", "1"),
        ("ADAUSD", "1"),
        ("ADAEUR", "1"),
        ("ADABTC", "1"),
        ("DOTUSD", "0.1"),
        ("DOTEUR", "0.1"),
        ("DOTBTC", "0.1"),
    ])
});

static TICK_SIZES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("BTCUSD", "0.1"),
        ("BTCEUR", "0.1"),
        ("ETHUSD", "0.01"),
        ("ETHEUR", "0.01"),
        ("ETHBTC", "0.000001"),
        ("LTCUSD", "0.01"),
        ("LTCEUR", "0.01"),
        ("LTCBTC", "0.000001"),
        ("XRPUSD", "0.0001"),
        ("XRPEUR", "0.0001"),
        ("XRPBTC", "0.00000001"),
        ("ADAUSD", "0.0001"),
        ("ADAEUR", "0.0001"),
        ("ADABTC", "0.00000001"),
        ("DOTUSD", "0.001"),
        ("DOTEUR", "0.001"),
        ("DOTBTC", "0.000001"),
    ])
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_conversion() {
        assert_eq!(KrakenSymbolMapper::standard_to_kraken_asset("BTC"), "XXBT");
        assert_eq!(KrakenSymbolMapper::standard_to_kraken_asset("USD"), "ZUSD");
        assert_eq!(KrakenSymbolMapper::standard_to_kraken_asset("ADA"), "ADA");
        
        assert_eq!(KrakenSymbolMapper::kraken_to_standard_asset("XXBT"), "BTC");
        assert_eq!(KrakenSymbolMapper::kraken_to_standard_asset("ZUSD"), "USD");
        assert_eq!(KrakenSymbolMapper::kraken_to_standard_asset("ADA"), "ADA");
    }

    #[test]
    fn test_pair_conversion() {
        assert_eq!(KrakenSymbolMapper::standard_to_kraken_pair("BTCUSD"), "XBTUSD");
        assert_eq!(KrakenSymbolMapper::standard_to_kraken_ws_pair("BTCUSD"), "XBT/USD");
        
        assert_eq!(KrakenSymbolMapper::kraken_to_standard_pair("XBTUSD"), "BTCUSD");
        assert_eq!(KrakenSymbolMapper::kraken_ws_to_standard_pair("XBT/USD"), "BTCUSD");
    }

    #[test]
    fn test_trading_info() {
        assert_eq!(KrakenSymbolMapper::get_lot_size("BTCUSD"), "0.0001");
        assert_eq!(KrakenSymbolMapper::get_tick_size("BTCUSD"), "0.1");
        assert_eq!(KrakenSymbolMapper::get_asset_precision("BTC"), 8);
        assert_eq!(KrakenSymbolMapper::get_asset_precision("JPY"), 0);
    }

    #[test]
    fn test_asset_classification() {
        assert!(KrakenSymbolMapper::is_major_crypto("BTC"));
        assert!(KrakenSymbolMapper::is_major_crypto("ETH"));
        assert!(!KrakenSymbolMapper::is_major_crypto("USD"));
        
        assert!(KrakenSymbolMapper::is_fiat_currency("USD"));
        assert!(KrakenSymbolMapper::is_fiat_currency("EUR"));
        assert!(!KrakenSymbolMapper::is_fiat_currency("BTC"));
    }

    #[test]
    fn test_parse_symbol() {
        assert_eq!(parse_symbol("BTCUSD"), Some(("BTC".to_string(), "USD".to_string())));
        assert_eq!(parse_symbol("ETHEUR"), Some(("ETH".to_string(), "EUR".to_string())));
        assert_eq!(parse_symbol("ADAUSDT"), Some(("ADA".to_string(), "USDT".to_string())));
        assert_eq!(parse_symbol("INVALID"), None);
    }
}