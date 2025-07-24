-- Jackbot Sensor Database Schema

-- Exchange configuration
CREATE TABLE IF NOT EXISTS exchanges (
    id SERIAL PRIMARY KEY,
    name VARCHAR(50) UNIQUE NOT NULL,
    enabled BOOLEAN DEFAULT false,
    testnet BOOLEAN DEFAULT true,
    api_key_encrypted TEXT,
    api_secret_encrypted TEXT,
    passphrase_encrypted TEXT,
    rate_limit_requests_per_second INTEGER DEFAULT 10,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Trading pairs configuration
CREATE TABLE IF NOT EXISTS trading_pairs (
    id SERIAL PRIMARY KEY,
    exchange_id INTEGER REFERENCES exchanges(id),
    symbol VARCHAR(20) NOT NULL,
    base_asset VARCHAR(10) NOT NULL,
    quote_asset VARCHAR(10) NOT NULL,
    enabled BOOLEAN DEFAULT true,
    min_order_size DECIMAL(20, 8),
    max_order_size DECIMAL(20, 8),
    tick_size DECIMAL(20, 8),
    step_size DECIMAL(20, 8),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(exchange_id, symbol)
);

-- Order tracking
CREATE TABLE IF NOT EXISTS orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    exchange_id INTEGER REFERENCES exchanges(id),
    exchange_order_id VARCHAR(100),
    symbol VARCHAR(20) NOT NULL,
    side VARCHAR(10) NOT NULL,
    type VARCHAR(20) NOT NULL,
    price DECIMAL(20, 8),
    quantity DECIMAL(20, 8),
    filled_quantity DECIMAL(20, 8) DEFAULT 0,
    status VARCHAR(20) NOT NULL,
    client_order_id VARCHAR(100),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    filled_at TIMESTAMP,
    metadata JSONB
);

-- Trade history
CREATE TABLE IF NOT EXISTS trades (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID REFERENCES orders(id),
    exchange_id INTEGER REFERENCES exchanges(id),
    exchange_trade_id VARCHAR(100),
    symbol VARCHAR(20) NOT NULL,
    side VARCHAR(10) NOT NULL,
    price DECIMAL(20, 8) NOT NULL,
    quantity DECIMAL(20, 8) NOT NULL,
    fee DECIMAL(20, 8),
    fee_currency VARCHAR(10),
    timestamp TIMESTAMP NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Market data snapshots
CREATE TABLE IF NOT EXISTS market_snapshots (
    id BIGSERIAL PRIMARY KEY,
    exchange_id INTEGER REFERENCES exchanges(id),
    symbol VARCHAR(20) NOT NULL,
    bid_price DECIMAL(20, 8),
    bid_quantity DECIMAL(20, 8),
    ask_price DECIMAL(20, 8),
    ask_quantity DECIMAL(20, 8),
    last_price DECIMAL(20, 8),
    volume_24h DECIMAL(20, 8),
    timestamp TIMESTAMP NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Performance metrics
CREATE TABLE IF NOT EXISTS performance_metrics (
    id BIGSERIAL PRIMARY KEY,
    metric_type VARCHAR(50) NOT NULL,
    exchange_id INTEGER REFERENCES exchanges(id),
    value DECIMAL(20, 4) NOT NULL,
    unit VARCHAR(20),
    metadata JSONB,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Sensor state and configuration
CREATE TABLE IF NOT EXISTS sensor_state (
    id SERIAL PRIMARY KEY,
    instance_id VARCHAR(100) UNIQUE NOT NULL,
    status VARCHAR(20) NOT NULL,
    assigned_symbols TEXT[],
    last_heartbeat TIMESTAMP,
    metadata JSONB,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for performance
CREATE INDEX idx_orders_exchange_id ON orders(exchange_id);
CREATE INDEX idx_orders_symbol ON orders(symbol);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_created_at ON orders(created_at);

CREATE INDEX idx_trades_order_id ON trades(order_id);
CREATE INDEX idx_trades_symbol ON trades(symbol);
CREATE INDEX idx_trades_timestamp ON trades(timestamp);

CREATE INDEX idx_market_snapshots_exchange_symbol ON market_snapshots(exchange_id, symbol);
CREATE INDEX idx_market_snapshots_timestamp ON market_snapshots(timestamp);

CREATE INDEX idx_performance_metrics_type ON performance_metrics(metric_type);
CREATE INDEX idx_performance_metrics_timestamp ON performance_metrics(timestamp);

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Triggers for updated_at
CREATE TRIGGER update_exchanges_updated_at BEFORE UPDATE ON exchanges
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_trading_pairs_updated_at BEFORE UPDATE ON trading_pairs
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_orders_updated_at BEFORE UPDATE ON orders
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_sensor_state_updated_at BEFORE UPDATE ON sensor_state
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Insert default exchange configurations
INSERT INTO exchanges (name, enabled, testnet) VALUES
    ('binance', true, true),
    ('coinbase', true, true),
    ('bybit', true, true),
    ('okx', false, true),
    ('kraken', false, true),
    ('kucoin', false, true),
    ('gateio', false, true),
    ('mexc', false, true),
    ('bitget', false, true),
    ('hyperliquid', false, true),
    ('cryptocom', false, true)
ON CONFLICT (name) DO NOTHING;