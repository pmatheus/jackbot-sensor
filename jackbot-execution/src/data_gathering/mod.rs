pub mod exchange_connector;
/// Real-time market data collection and exchange integration
pub mod market_data_collector;
pub mod order_book_processor;
pub mod websocket_manager;

pub use market_data_collector::{InstrumentKey, MarketDataCollector, MarketDataUpdate, PriceData};
pub use order_book_processor::{OrderBookProcessor, ProcessorError};
pub use websocket_manager::{ConnectionStatus, WebSocketConnection, WebSocketManager};

// Deliberately not re-exporting exchange_connector::* to avoid conflicts
pub use exchange_connector::{
    ConnectorError as ExchConnectorError, ExchangeConnector as ExchConnector,
};
