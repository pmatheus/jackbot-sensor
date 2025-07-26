# SRD-002: Backend Service Fixes
**Status**: HIGH PRIORITY  
**Priority**: P1  
**Timeline**: Hours 1-3  
**Version**: 1.0.0  
**Last Updated**: 2025-07-26

## Executive Summary

Backend services have 5 categories of errors totaling approximately 30+ compilation errors. These must be resolved after sensor fixes to enable integration testing. Focus on Arrow crate version alignment and trait implementations.

## Technical Context

### Affected Components
1. **test-utils**: 12 errors - Debug trait bounds
2. **data-lake-query**: Unused variables, missing trait implementations  
3. **market-data-service**: 5 errors - ArrowWriter conflicts
4. **sensor-management**: Missing error conversions
5. **Root Issue**: Arrow crate version mismatch (53.4.0 vs 51.0.0)

## Detailed Fixes

### Fix 1: Align Arrow Crate Versions

**Strategy**: Standardize on Arrow 51.0.0 across all services

**Files to Update**:
1. `/backend/test-utils/Cargo.toml`
2. `/backend/data-lake-query/Cargo.toml`
3. `/backend/market-data-service/Cargo.toml`
4. `/backend/sensor-management/Cargo.toml`

**Update Pattern**:
```toml
[dependencies]
arrow = "51.0.0"
arrow-array = "51.0.0"
arrow-schema = "51.0.0"
arrow-flight = "51.0.0"
parquet = "51.0.0"  # Must match arrow version
```

**Verification Script**:
```bash
#!/bin/bash
# verify-arrow-versions.sh
find . -name "Cargo.toml" -exec grep -H "arrow" {} \; | grep -v "51.0.0"
```

### Fix 2: Debug Trait Implementations for test-utils

**File**: `/backend/test-utils/src/lib.rs`

**Common Pattern for Missing Debug**:
```rust
// Before
pub struct TestData<T> {
    data: T,
}

// After
#[derive(Debug)]
pub struct TestData<T> 
where 
    T: Debug,
{
    data: T,
}
```

**Specific Fixes**:

1. **MockExchange struct**:
```rust
#[derive(Debug, Clone)]
pub struct MockExchange {
    pub name: String,
    pub orders: Arc<Mutex<Vec<Order>>>,
    pub trades: Arc<Mutex<Vec<Trade>>>,
}
```

2. **TestContext struct**:
```rust
#[derive(Debug)]
pub struct TestContext<T: Debug> {
    pub exchange: MockExchange,
    pub data: T,
    pub timestamp: i64,
}
```

3. **Generic Bounds**:
```rust
impl<T> TestRunner<T> 
where 
    T: Debug + Send + Sync + 'static,
{
    // implementation
}
```

### Fix 3: data-lake-query Fixes

**File**: `/backend/data-lake-query/src/query_engine.rs`

**Unused Variable Fixes**:
```rust
// Before
pub async fn execute_query(query: &str, params: QueryParams) -> Result<RecordBatch> {
    let context = SessionContext::new();
    let df = context.sql(query).await?;
    // params unused
    df.collect().await
}

// After
pub async fn execute_query(query: &str, params: QueryParams) -> Result<RecordBatch> {
    let context = SessionContext::new();
    
    // Apply params to context
    if let Some(limit) = params.limit {
        context.set_config_option("datafusion.execution.batch_size", &limit.to_string())?;
    }
    
    let df = context.sql(query).await?;
    df.collect().await
}
```

**Missing Trait Implementations**:
```rust
// Add From implementations for error types
impl From<ArrowError> for QueryError {
    fn from(err: ArrowError) -> Self {
        QueryError::ArrowError(err.to_string())
    }
}

impl From<DataFusionError> for QueryError {
    fn from(err: DataFusionError) -> Self {
        QueryError::DataFusionError(err.to_string())
    }
}
```

### Fix 4: market-data-service ArrowWriter Fixes

**File**: `/backend/market-data-service/src/writer.rs`

**ArrowWriter Version Compatibility**:
```rust
use arrow::array::{ArrayRef, RecordBatch};
use arrow::datatypes::Schema;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;

pub struct MarketDataWriter {
    writer: ArrowWriter<File>,
}

impl MarketDataWriter {
    pub fn new(path: &Path, schema: Schema) -> Result<Self> {
        let file = File::create(path)?;
        let props = WriterProperties::builder()
            .set_compression(parquet::basic::Compression::SNAPPY)
            .build();
            
        let writer = ArrowWriter::try_new(file, Arc::new(schema), Some(props))?;
        
        Ok(Self { writer })
    }
    
    pub fn write_batch(&mut self, batch: &RecordBatch) -> Result<()> {
        self.writer.write(batch)?;
        Ok(())
    }
    
    pub fn close(mut self) -> Result<()> {
        self.writer.close()?;
        Ok(())
    }
}
```

### Fix 5: sensor-management Error Conversions

**File**: `/backend/sensor-management/src/errors.rs`

**Add Missing Conversions**:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SensorError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Kafka error: {0}")]
    Kafka(#[from] rdkafka::error::KafkaError),
    
    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
    
    #[error("Custom error: {0}")]
    Custom(String),
}

// Implement conversions for external types
impl From<tokio::task::JoinError> for SensorError {
    fn from(err: tokio::task::JoinError) -> Self {
        SensorError::Custom(format!("Task join error: {}", err))
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for SensorError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        SensorError::Custom(err.to_string())
    }
}
```

## Test Plan

### Unit Tests per Service

1. **test-utils**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mock_exchange_debug() {
        let exchange = MockExchange::new("test");
        println!("{:?}", exchange); // Should compile
    }
    
    #[test]
    fn test_generic_bounds() {
        #[derive(Debug)]
        struct TestData { value: i32 }
        
        let context = TestContext {
            exchange: MockExchange::new("test"),
            data: TestData { value: 42 },
            timestamp: 0,
        };
        
        assert_eq!(format!("{:?}", context).is_empty(), false);
    }
}
```

2. **data-lake-query**:
```rust
#[tokio::test]
async fn test_query_with_params() {
    let params = QueryParams {
        limit: Some(100),
        filters: vec![],
    };
    
    let result = execute_query("SELECT * FROM trades", params).await;
    assert!(result.is_ok());
}
```

3. **market-data-service**:
```rust
#[test]
fn test_arrow_writer_creation() {
    let schema = Schema::new(vec![
        Field::new("timestamp", DataType::Int64, false),
        Field::new("price", DataType::Float64, false),
    ]);
    
    let temp_dir = tempdir().unwrap();
    let path = temp_dir.path().join("test.parquet");
    
    let writer = MarketDataWriter::new(&path, schema);
    assert!(writer.is_ok());
}
```

### Integration Tests

```bash
#!/bin/bash
# run-backend-tests.sh

# Check each service compiles
for service in test-utils data-lake-query market-data-service sensor-management; do
    echo "Checking $service..."
    cargo check --package $service || exit 1
done

# Run unit tests
cargo test --package test-utils
cargo test --package data-lake-query  
cargo test --package market-data-service
cargo test --package sensor-management

# Integration test
cargo test --test backend_integration
```

## Rollback Plan

1. **Version Rollback**:
```bash
git checkout HEAD -- Cargo.lock
cargo update -p arrow --precise 53.4.0
```

2. **Service-by-Service Rollback**:
- Each service can be rolled back independently
- Keep version alignment documentation

## Success Metrics

1. **Compilation**: All backend services compile with 0 errors
2. **Tests**: 100% of existing tests pass
3. **Performance**: No regression in query performance
4. **Memory**: No increase in memory usage

## Implementation Order

1. **Hour 1**: Fix Arrow version conflicts (30 min)
2. **Hour 1.5**: Fix test-utils Debug traits (30 min)
3. **Hour 2**: Fix data-lake-query (30 min)
4. **Hour 2.5**: Fix market-data-service (30 min)
5. **Hour 3**: Fix sensor-management + integration tests (30 min)

## Dependencies

- Requires SENSOR fixes to be complete
- Blocks integration testing
- Frontend can proceed in parallel

## Risk Assessment

- **High Risk**: Arrow version changes affect data serialization
- **Medium Risk**: Error conversion changes affect error handling
- **Low Risk**: Debug trait additions are non-breaking
- **Mitigation**: Extensive testing before deployment, feature flags for gradual rollout