use crate::AccountSnapshot;
use jackbot_instrument::exchange::ExchangeId;
use rust_decimal::Decimal;

/// Configuration for a mock execution client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockExecutionConfig {
    /// Exchange identifier for the mock execution.
    pub mocked_exchange: ExchangeId,
    /// Initial account state snapshot for the mock exchange.
    pub initial_state: AccountSnapshot,
    /// Simulated network latency in milliseconds.
    pub latency_ms: u64,
    /// Fees percentage applied to mock orders.
    pub fees_percent: Decimal,
}
