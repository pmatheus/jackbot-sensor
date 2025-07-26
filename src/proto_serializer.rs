//! Protocol Buffer Serialization Module
//!
//! Converts sensor data types to Protocol Buffer messages for Kafka streaming

use anyhow::{Context, Result};
use prost::Message;
use prost_types::Timestamp;

use crate::api::{TickerData, OrderBookData, TradeData, KlineData};

// Include generated Protocol Buffer code
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/jackbot.market.rs"));
}

use proto::*;

/// Convert sensor data types to Protocol Buffer messages
pub struct ProtoSerializer;

impl ProtoSerializer {
    /// Convert TickerData to Protocol Buffer MarketData
    pub fn serialize_ticker(ticker: &TickerData) -> Result<Vec<u8>> {
        let tick = TickData {
            symbol: ticker.symbol.clone(),
            bid: ticker.bid,
            ask: ticker.ask,
            bid_size: 0.0, // Not available in TickerData
            ask_size: 0.0, // Not available in TickerData
            last_price: ticker.price,
            last_size: 0.0, // Not available in TickerData
            volume_24h: ticker.volume_24h,
            update_time: Some(Self::timestamp_from_millis(ticker.timestamp)),
        };

        let market_data = MarketData {
            data_id: Self::generate_data_id("tick", &ticker.exchange, &ticker.symbol),
            timestamp: Some(Self::timestamp_from_millis(ticker.timestamp)),
            exchange: ticker.exchange.clone(),
            data: Some(market_data::Data::Tick(tick)),
        };

        let mut buf = Vec::new();
        market_data.encode(&mut buf)
            .context("Failed to encode ticker data")?;
        
        Ok(buf)
    }

    /// Convert OrderBookData to Protocol Buffer MarketData
    pub fn serialize_orderbook(orderbook: &OrderBookData) -> Result<Vec<u8>> {
        let bids = orderbook.bids.iter().take(20).map(|level| PriceLevel {
            price: level.0,
            quantity: level.1,
            order_count: 0, // Not available in our data structure
        }).collect();

        let asks = orderbook.asks.iter().take(20).map(|level| PriceLevel {
            price: level.0,
            quantity: level.1,
            order_count: 0, // Not available in our data structure
        }).collect();

        let order_book = OrderBookData {
            symbol: orderbook.symbol.clone(),
            bids,
            asks,
            is_snapshot: true, // Assume snapshot for now
            sequence_number: orderbook.sequence_id.unwrap_or(0),
            update_time: Some(Self::timestamp_from_millis(orderbook.timestamp)),
        };

        let market_data = MarketData {
            data_id: Self::generate_data_id("orderbook", &orderbook.exchange, &orderbook.symbol),
            timestamp: Some(Self::timestamp_from_millis(orderbook.timestamp)),
            exchange: orderbook.exchange.clone(),
            data: Some(market_data::Data::OrderBook(order_book)),
        };

        let mut buf = Vec::new();
        market_data.encode(&mut buf)
            .context("Failed to encode orderbook data")?;
        
        Ok(buf)
    }

    /// Convert TradeData to Protocol Buffer MarketData
    pub fn serialize_trade(trade: &TradeData) -> Result<Vec<u8>> {
        let side = match trade.side.to_lowercase().as_str() {
            "buy" | "bid" => Side::Buy,
            "sell" | "ask" => Side::Sell,
            _ => Side::Buy, // Default to buy
        };

        let trade_data = TradeData {
            symbol: trade.symbol.clone(),
            trade_id: trade.id.clone(),
            price: trade.price,
            quantity: trade.quantity,
            side: side as i32,
            trade_time: Some(Self::timestamp_from_millis(trade.timestamp)),
            is_maker: trade.is_maker,
        };

        let market_data = MarketData {
            data_id: Self::generate_data_id("trade", &trade.exchange, &trade.symbol),
            timestamp: Some(Self::timestamp_from_millis(trade.timestamp)),
            exchange: trade.exchange.clone(),
            data: Some(market_data::Data::Trade(trade_data)),
        };

        let mut buf = Vec::new();
        market_data.encode(&mut buf)
            .context("Failed to encode trade data")?;
        
        Ok(buf)
    }

    /// Convert KlineData to Protocol Buffer MarketData
    pub fn serialize_kline(kline: &KlineData) -> Result<Vec<u8>> {
        let candle = CandleData {
            symbol: kline.symbol.clone(),
            open_time: Some(Self::timestamp_from_millis(kline.open_time)),
            close_time: Some(Self::timestamp_from_millis(kline.close_time)),
            open: kline.open,
            high: kline.high,
            low: kline.low,
            close: kline.close,
            volume: kline.volume,
            quote_volume: kline.quote_volume.unwrap_or(0.0),
            trade_count: kline.trade_count.unwrap_or(0) as i32,
            interval: kline.interval.clone(),
        };

        let market_data = MarketData {
            data_id: Self::generate_data_id("kline", &kline.exchange, &kline.symbol),
            timestamp: Some(Self::timestamp_from_millis(kline.close_time)),
            exchange: kline.exchange.clone(),
            data: Some(market_data::Data::Candle(candle)),
        };

        let mut buf = Vec::new();
        market_data.encode(&mut buf)
            .context("Failed to encode kline data")?;
        
        Ok(buf)
    }

    /// Generate unique data ID
    fn generate_data_id(data_type: &str, exchange: &str, symbol: &str) -> String {
        format!("{}:{}:{}:{}", 
            data_type, 
            exchange, 
            symbol, 
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default() / 1_000_000
        )
    }

    /// Convert milliseconds timestamp to protobuf Timestamp
    fn timestamp_from_millis(millis: i64) -> Timestamp {
        Timestamp {
            seconds: millis / 1000,
            nanos: ((millis % 1000) * 1_000_000) as i32,
        }
    }

    /// Deserialize Protocol Buffer MarketData for testing
    pub fn deserialize_market_data(data: &[u8]) -> Result<MarketData> {
        MarketData::decode(data)
            .context("Failed to decode market data")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_ticker() -> TickerData {
        TickerData {
            symbol: "BTC/USDT".to_string(),
            exchange: "binance".to_string(),
            price: 50000.0,
            bid: 49999.0,
            ask: 50001.0,
            volume_24h: 1000.0,
            change_24h: 2.5,
            high_24h: 51000.0,
            low_24h: 49000.0,
            timestamp: Utc::now().timestamp_millis(),
        }
    }

    fn create_test_orderbook() -> OrderBookData {
        OrderBookData {
            symbol: "BTC/USDT".to_string(),
            exchange: "binance".to_string(),
            bids: vec![(49999.0, 1.5), (49998.0, 2.0), (49997.0, 0.5)],
            asks: vec![(50001.0, 1.2), (50002.0, 1.8), (50003.0, 0.8)],
            timestamp: Utc::now().timestamp_millis(),
            sequence_id: Some(12345),
        }
    }

    fn create_test_trade() -> TradeData {
        TradeData {
            symbol: "BTC/USDT".to_string(),
            exchange: "binance".to_string(),
            id: "trade123".to_string(),
            price: 50000.0,
            quantity: 0.1,
            side: "buy".to_string(),
            timestamp: Utc::now().timestamp_millis(),
            is_maker: false,
        }
    }

    fn create_test_kline() -> KlineData {
        KlineData {
            symbol: "BTC/USDT".to_string(),
            exchange: "binance".to_string(),
            interval: "1m".to_string(),
            open_time: Utc::now().timestamp_millis() - 60000,
            close_time: Utc::now().timestamp_millis(),
            open: 49900.0,
            high: 50100.0,
            low: 49800.0,
            close: 50000.0,
            volume: 10.5,
            quote_volume: Some(525000.0),
            trade_count: Some(150),
        }
    }

    #[test]
    fn test_serialize_ticker() {
        let ticker = create_test_ticker();
        let serialized = ProtoSerializer::serialize_ticker(&ticker).unwrap();
        
        assert!(!serialized.is_empty());
        
        // Test deserialization
        let deserialized = ProtoSerializer::deserialize_market_data(&serialized).unwrap();
        assert_eq!(deserialized.exchange, "binance");
        
        if let Some(market_data::Data::Tick(tick)) = deserialized.data {
            assert_eq!(tick.symbol, "BTC/USDT");
            assert_eq!(tick.last_price, 50000.0);
        } else {
            panic!("Expected tick data");
        }
    }

    #[test]
    fn test_serialize_orderbook() {
        let orderbook = create_test_orderbook();
        let serialized = ProtoSerializer::serialize_orderbook(&orderbook).unwrap();
        
        assert!(!serialized.is_empty());
        
        let deserialized = ProtoSerializer::deserialize_market_data(&serialized).unwrap();
        if let Some(market_data::Data::OrderBook(ob)) = deserialized.data {
            assert_eq!(ob.symbol, "BTC/USDT");
            assert_eq!(ob.bids.len(), 3);
            assert_eq!(ob.asks.len(), 3);
            assert_eq!(ob.bids[0].price, 49999.0);
        } else {
            panic!("Expected orderbook data");
        }
    }

    #[test]
    fn test_serialize_trade() {
        let trade = create_test_trade();
        let serialized = ProtoSerializer::serialize_trade(&trade).unwrap();
        
        assert!(!serialized.is_empty());
        
        let deserialized = ProtoSerializer::deserialize_market_data(&serialized).unwrap();
        if let Some(market_data::Data::Trade(t)) = deserialized.data {
            assert_eq!(t.symbol, "BTC/USDT");
            assert_eq!(t.price, 50000.0);
            assert_eq!(t.side, Side::Buy as i32);
        } else {
            panic!("Expected trade data");
        }
    }

    #[test]
    fn test_serialize_kline() {
        let kline = create_test_kline();
        let serialized = ProtoSerializer::serialize_kline(&kline).unwrap();
        
        assert!(!serialized.is_empty());
        
        let deserialized = ProtoSerializer::deserialize_market_data(&serialized).unwrap();
        if let Some(market_data::Data::Candle(candle)) = deserialized.data {
            assert_eq!(candle.symbol, "BTC/USDT");
            assert_eq!(candle.interval, "1m");
            assert_eq!(candle.close, 50000.0);
        } else {
            panic!("Expected kline data");
        }
    }

    #[test]
    fn test_timestamp_conversion() {
        let now = Utc::now().timestamp_millis();
        let timestamp = ProtoSerializer::timestamp_from_millis(now);
        
        let expected_seconds = now / 1000;
        let expected_nanos = ((now % 1000) * 1_000_000) as i32;
        
        assert_eq!(timestamp.seconds, expected_seconds);
        assert_eq!(timestamp.nanos, expected_nanos);
    }
}