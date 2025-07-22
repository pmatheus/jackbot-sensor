# JackBot API Implementation Specification

This document tracks all API endpoints that need to be fully implemented in the JackBot sensor API.

## Overview

The JackBot sensor API (`/jackbot-sensor/src/api.rs`) currently has placeholder implementations for many endpoints. These need to be connected to the actual execution engine, data providers, and other components.

## Implementation Categories

### 1. Market Data Endpoints

#### Real-time Data
- [ ] **GET /api/ticker** - Integrate with ConnectorManager to fetch real ticker data
  - Currently returns simulated data
  - Needs: ConnectorManager.get_ticker() implementation
  
- [ ] **GET /api/tickers** - Get all tickers from exchange
  - Currently returns hardcoded sample data
  - Needs: ConnectorManager integration
  
- [ ] **GET /api/orderbook** - Get actual order book data
  - Currently returns simulated data
  - Needs: ConnectorManager L2 order book integration
  
- [ ] **GET /api/trades** - Get actual trade data
  - Currently returns simulated data
  - Needs: ConnectorManager trade stream integration
  
- [ ] **GET /api/klines** - Get actual candle/kline data
  - Currently returns simulated data
  - Needs: ConnectorManager OHLCV integration
  
- [ ] **GET /api/symbols** - Get actual symbols from exchange
  - Currently returns hardcoded symbols
  - Needs: Exchange connector symbol fetching

#### Historical Data
- [ ] **GET /api/historical/klines** - Implement historical klines from S3/Parquet
  - Currently returns empty array
  - Needs: S3 storage integration, Parquet reader
  
- [ ] **GET /api/historical/trades** - Implement historical trades from S3/Parquet
  - Currently returns empty array
  - Needs: S3 storage integration, Parquet reader

### 2. Order Management Endpoints

#### Order Placement
- [ ] **POST /api/orders** - Implement actual order placement via jackbot-execution
  - Currently returns mock order response
  - Needs: Integration with OrderExecutor
  
- [ ] **POST /api/orders/smart** - Implement smart order placement
  - Currently returns mock response
  - Needs: Smart order routing logic
  
- [ ] **POST /api/orders/prophetic** - Implement prophetic order placement
  - Currently returns mock response
  - Needs: ML prediction integration
  
- [ ] **POST /api/orders/jackpot** - Implement jackpot order placement
  - Currently returns mock response
  - Needs: Jackpot strategy implementation

#### Order Management
- [ ] **GET /api/orders/:id** - Get order by ID
  - Currently returns mock data
  - Needs: Order state management integration
  
- [ ] **DELETE /api/orders/:id** - Implement order cancellation
  - Currently returns success without action
  - Needs: OrderExecutor cancel integration
  
- [ ] **PUT /api/orders/:id** - Implement order update (price/quantity)
  - Currently returns success without action
  - Needs: Order modification logic
  
- [ ] **DELETE /api/orders** - Implement cancel all orders
  - Currently returns success without action
  - Needs: Bulk cancellation logic
  
- [ ] **GET /api/orders** - Implement get open orders
  - Currently returns empty array
  - Needs: Order state query
  
- [ ] **GET /api/orders/history** - Implement get order history
  - Currently returns empty array
  - Needs: Order history storage

### 3. Account Management Endpoints

#### Balances & Positions
- [ ] **GET /api/account/balances** - Get actual balances from jackbot-execution
  - Currently returns mock balances
  - Needs: Exchange client balance integration
  
- [ ] **GET /api/account/positions** - Get actual positions from jackbot-execution
  - Currently returns mock positions
  - Needs: Position tracking integration
  
- [ ] **GET /api/account/balance** - Get actual balance (singular)
  - Currently returns mock balance
  - Needs: Account state integration
  
- [ ] **GET /api/account/trades** - Get actual trade history
  - Currently returns mock trades
  - Needs: Trade history storage
  
- [ ] **GET /api/account/pnl** - Get actual P&L summary
  - Currently returns mock data
  - Needs: P&L calculation engine

#### Transaction History
- [ ] **GET /api/account/deposits** - Implement deposit history
  - Currently returns empty array
  - Needs: Transaction history storage
  
- [ ] **GET /api/account/withdrawals** - Implement withdrawal history
  - Currently returns empty array
  - Needs: Transaction history storage

### 4. Strategy Management Endpoints

#### Strategy CRUD
- [ ] **GET /api/strategies** - Get actual strategies
  - Currently returns mock strategies
  - Needs: Strategy storage integration
  
- [ ] **POST /api/strategies** - Implement strategy deployment
  - Currently returns mock response
  - Needs: Strategy engine integration
  
- [ ] **GET /api/strategies/:id** - Get strategy details
  - Currently returns mock data
  - Needs: Strategy state management
  
- [ ] **DELETE /api/strategies/:id** - Implement strategy deletion
  - Currently returns success without action
  - Needs: Strategy lifecycle management
  
- [ ] **PUT /api/strategies/:id** - Implement strategy update
  - Currently returns success without action
  - Needs: Strategy configuration management

#### Strategy Control
- [ ] **POST /api/strategies/:id/start** - Implement strategy start
  - Currently returns success without action
  - Needs: Strategy execution control
  
- [ ] **POST /api/strategies/:id/stop** - Implement strategy stop
  - Currently returns success without action
  - Needs: Strategy execution control
  
- [ ] **GET /api/strategies/:id/performance** - Implement strategy performance
  - Currently returns mock data
  - Needs: Performance tracking integration
  
- [ ] **GET /api/strategies/:id/status** - Implement strategy status
  - Currently returns mock data
  - Needs: Strategy state monitoring

#### Backtesting
- [ ] **POST /api/strategies/backtest** - Implement backtest via jackbot-strategy
  - Currently returns mock response
  - Needs: Backtesting engine integration

### 5. Risk Management Endpoints

- [ ] **GET /api/risk/limits** - Get actual risk limits
  - Currently returns mock limits
  - Needs: Risk manager integration
  
- [ ] **PUT /api/risk/limits** - Update risk limits in jackbot-risk
  - Currently returns success without action
  - Needs: Risk configuration management
  
- [ ] **GET /api/risk/exposure** - Get actual portfolio exposure
  - Currently returns mock data
  - Needs: Exposure calculation engine
  
- [ ] **GET /api/risk/drawdown** - Get actual drawdown metrics
  - Currently returns mock data
  - Needs: Drawdown tracking
  
- [ ] **GET /api/risk/alerts** - Get actual risk alerts
  - Currently returns mock alerts
  - Needs: Risk alert system integration

### 6. Staking Endpoints

- [ ] **GET /api/staking/products** - Get actual staking products
  - Currently returns mock products
  - Needs: Staking service integration
  
- [ ] **POST /api/staking/stake** - Implement actual staking
  - Currently returns mock response
  - Needs: Staking execution engine
  
- [ ] **POST /api/staking/unstake** - Implement actual unstaking
  - Currently returns mock response
  - Needs: Unstaking logic
  
- [ ] **GET /api/staking/positions** - Get actual staking positions
  - Currently returns mock positions
  - Needs: Staking position tracking
  
- [ ] **GET /api/staking/rewards** - Get actual staking rewards
  - Currently returns mock rewards
  - Needs: Rewards calculation engine

### 7. System Management Endpoints

- [ ] **POST /api/system/connectors/:exchange/restart** - Implement connector restart
  - Currently returns success without action
  - Needs: Connector lifecycle management
  
- [ ] **POST /api/system/symbols/update** - Implement symbol update
  - Currently returns success without action
  - Needs: Symbol cache management
  
- [ ] **GET /api/system/stats** - Get actual system statistics
  - Currently returns mock stats
  - Needs: System metrics collection
  
- [ ] **POST /api/system/emergency-stop** - Implement emergency stop
  - Currently returns success without action
  - Needs: Emergency shutdown logic
  
- [ ] **POST /api/system/logs/export** - Implement log export
  - Currently returns mock URL
  - Needs: Log aggregation and export

### 8. WebSocket Implementation

- [ ] **WebSocket subscriptions** - Add actual subscription to channels
  - Currently logs debug messages only
  - Needs: Real-time data streaming
  
- [ ] **WebSocket unsubscriptions** - Remove subscription from channels
  - Currently logs debug messages only
  - Needs: Channel management

## Implementation Priority

1. **Critical (P0)**:
   - Order placement and cancellation
   - Account balances and positions
   - Risk limits and emergency stop
   - Real-time ticker data

2. **High (P1)**:
   - Order book and trade data
   - Strategy deployment and control
   - P&L calculation
   - WebSocket streaming

3. **Medium (P2)**:
   - Historical data access
   - Staking operations
   - System statistics
   - Advanced order types

4. **Low (P3)**:
   - Log export
   - Symbol updates
   - Deposit/withdrawal history

## Technical Requirements

### Dependencies
- ConnectorManager implementation completion
- OrderExecutor integration
- Risk manager integration
- S3/Parquet storage setup
- WebSocket infrastructure
- Performance monitoring

### Data Flow
1. API receives request
2. Validates permissions and parameters
3. Routes to appropriate service (execution, data, risk, etc.)
4. Service processes request
5. Response formatted and returned
6. Metrics and logs recorded

### Error Handling
- Implement proper error types for each service
- Return appropriate HTTP status codes
- Include detailed error messages for debugging
- Log all errors with context

### Testing Requirements
- Unit tests for each endpoint
- Integration tests with mock services
- Load testing for high-volume endpoints
- End-to-end testing with real services