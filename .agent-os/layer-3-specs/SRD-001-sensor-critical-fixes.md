# SRD-001: Sensor Critical Fixes
**Status**: BLOCKER  
**Priority**: P0  
**Timeline**: Hour 0-1  
**Version**: 1.0.0  
**Last Updated**: 2025-07-26

## Executive Summary

The sensor component has 5 critical compilation errors blocking the entire Jackbot platform. These must be resolved within the first hour to unblock backend and integration work. All fixes maintain backward compatibility while extending functionality.

## Technical Context

### Current State
- **Component**: jackbot-data (v0.10.1)
- **Dependencies**: jackbot-instrument, jackbot-integration
- **Error Count**: 5 compilation errors
- **Blocking**: All downstream services

### Root Cause Analysis
1. **MarketDataInstrument Schema Mismatch**: Missing fields `name_exchange` and direct `kind` access
2. **Type Inference Failure**: Closure parameter types not specified
3. **Trait Mismatch**: Validator trait expecting different item type
4. **Missing Trait Bound**: DeserializeOwned not constrained on transformer input

## Detailed Fixes

### Fix 1: Extend MarketDataInstrument Schema

**File**: `/Users/user/jackbot/jackbot-sensor/jackbot-instrument/src/instrument/market_data.rs`

**Current Code** (lines 9-15):
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketDataInstrument {
    pub instrument: Instrument,
    pub symbol: String,
    pub tick_size: Option<rust_decimal::Decimal>,
    pub lot_size: Option<rust_decimal::Decimal>,
}
```

**Fixed Code**:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketDataInstrument {
    pub instrument: Instrument,
    pub symbol: String,
    pub tick_size: Option<rust_decimal::Decimal>,
    pub lot_size: Option<rust_decimal::Decimal>,
    pub name_exchange: String,  // Exchange-specific symbol name
    pub kind: kind::MarketDataInstrumentKind,  // Direct access to kind
}
```

**Constructor Update** (lines 17-30):
```rust
impl MarketDataInstrument {
    pub fn new(
        base: &str,
        quote: &str,
        kind: kind::MarketDataInstrumentKind,
        symbol: String,
    ) -> Self {
        let name_exchange = symbol.clone();  // Default to symbol
        Self {
            instrument: Instrument::new(base, quote, kind),
            symbol,
            tick_size: None,
            lot_size: None,
            name_exchange,
            kind,
        }
    }
    
    pub fn with_exchange_name(mut self, name: String) -> Self {
        self.name_exchange = name;
        self
    }
```

### Fix 2: Add Type Annotations for Closure

**File**: `/Users/user/jackbot/jackbot-sensor/jackbot-data/src/streams/builder/dynamic/indexed.rs`

**Current Code** (line 138):
```rust
let find_instrument = |exchange, kind, base, quote| {
```

**Fixed Code**:
```rust
let find_instrument = |exchange: &ExchangeId, kind: &MarketDataInstrumentKind, base: &str, quote: &str| {
```

### Fix 3: Fix Validator Trait Usage

**File**: `/Users/user/jackbot/jackbot-sensor/jackbot-data/src/subscriber/validator.rs`

**Current Code** (line 83):
```rust
Some(Ok(response)) => match response.validate(&response) {
```

**Fixed Code**:
```rust
Some(Ok(response)) => match self.validate(&response) {
```

**Note**: The validator should be called on `self`, not the response validating itself.

### Fix 4: Add DeserializeOwned Constraint

**File**: `/Users/user/jackbot/jackbot-sensor/jackbot-data/src/lib.rs`

**Current Code** (around line 330):
```rust
impl<Exchange, Subscription, StreamTransformer> Stream
    for ExchangeWsStream<Exchange, Subscription, StreamTransformer>
where
    Exchange: Connector,
    Subscription: AsRef<[Exchange::Subscription]>,
    StreamTransformer: Transformer,
```

**Fixed Code**:
```rust
impl<Exchange, Subscription, StreamTransformer> Stream
    for ExchangeWsStream<Exchange, Subscription, StreamTransformer>
where
    Exchange: Connector,
    Subscription: AsRef<[Exchange::Subscription]>,
    StreamTransformer: Transformer,
    StreamTransformer::Input: serde::de::DeserializeOwned,
```

### Fix 5: Add Exchange Field to OrderBookData

**File**: `/Users/user/jackbot/jackbot-sensor/src/kafka_producer.rs` (or relevant file using OrderBookData)

**Analysis**: The OrderBookData struct already has an `exchange` field based on the grep results. The error likely comes from a different usage context where the field is accessed incorrectly.

## Test Plan

### Unit Tests

1. **MarketDataInstrument Tests**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_market_data_instrument_creation() {
        let instrument = MarketDataInstrument::new(
            "BTC", 
            "USDT", 
            kind::MarketDataInstrumentKind::Spot,
            "BTCUSDT".to_string()
        );
        
        assert_eq!(instrument.name_exchange, "BTCUSDT");
        assert_eq!(instrument.kind, kind::MarketDataInstrumentKind::Spot);
    }
    
    #[test]
    fn test_with_exchange_name() {
        let instrument = MarketDataInstrument::new(
            "BTC", 
            "USDT", 
            kind::MarketDataInstrumentKind::Spot,
            "BTCUSDT".to_string()
        ).with_exchange_name("BTC-USDT".to_string());
        
        assert_eq!(instrument.name_exchange, "BTC-USDT");
        assert_eq!(instrument.symbol, "BTCUSDT");
    }
}
```

2. **Closure Type Inference Test**:
```rust
#[test]
fn test_find_instrument_closure() {
    let exchange_id = ExchangeId::Binance;
    let kind = MarketDataInstrumentKind::Spot;
    let base = "BTC";
    let quote = "USDT";
    
    let find_instrument = |exchange: &ExchangeId, kind: &MarketDataInstrumentKind, base: &str, quote: &str| {
        exchange == &ExchangeId::Binance && base == "BTC"
    };
    
    assert!(find_instrument(&exchange_id, &kind, base, quote));
}
```

### Integration Tests

Run after fixes are applied:
```bash
# Compile check
cargo check --package jackbot-data

# Run unit tests
cargo test --package jackbot-instrument
cargo test --package jackbot-data

# Verify no regression
cargo test --workspace
```

## Rollback Plan

If any fix causes unexpected issues:

1. **Immediate Revert**:
```bash
git revert HEAD
cargo clean
cargo build
```

2. **Partial Rollback**:
- Each fix is independent except Fix 1 (MarketDataInstrument)
- Can revert individual files if needed
- Maintain backward compatibility by keeping old fields

## Success Metrics

1. **Compilation**: `cargo check --package jackbot-data` passes with 0 errors
2. **Tests**: All existing tests continue to pass
3. **Performance**: No degradation in build time
4. **Integration**: Downstream services compile successfully

## Implementation Checklist

- [ ] Apply Fix 1: Extend MarketDataInstrument (10 min)
- [ ] Apply Fix 2: Add closure type annotations (5 min)
- [ ] Apply Fix 3: Fix validator trait usage (5 min)
- [ ] Apply Fix 4: Add DeserializeOwned constraint (10 min)
- [ ] Apply Fix 5: Verify OrderBookData usage (10 min)
- [ ] Run unit tests (10 min)
- [ ] Run integration tests (10 min)
- [ ] Commit with descriptive message (5 min)

**Total Time**: 65 minutes (within 1-hour window)

## Dependencies

- No external dependencies
- Must coordinate with backend team after sensor fixes complete
- Frontend can proceed independently

## Risk Assessment

- **Low Risk**: All changes are additive or type clarifications
- **Medium Risk**: MarketDataInstrument schema change affects serialization
- **Mitigation**: Extensive test coverage before deployment