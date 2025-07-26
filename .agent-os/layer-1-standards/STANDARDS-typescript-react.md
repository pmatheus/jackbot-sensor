# TypeScript & React Development Standards
**Layer 1 - Global Standards**  
**Version**: 1.0.0  
**Last Updated**: 2025-07-26

## Overview

These standards ensure consistent, type-safe, and performant frontend development across all Jackbot web applications.

## TypeScript Configuration

### tsconfig.json
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "jsx": "react-jsx",
    "strict": true,
    "noImplicitAny": true,
    "strictNullChecks": true,
    "strictFunctionTypes": true,
    "noImplicitThis": true,
    "alwaysStrict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noImplicitReturns": true,
    "noFallthroughCasesInSwitch": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "allowSyntheticDefaultImports": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "moduleResolution": "bundler"
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist", "build"]
}
```

## Type Safety

### No Any Types
```typescript
// ❌ Bad
function processData(data: any): any {
  return data.value;
}

// ✅ Good
interface DataItem {
  value: number;
  timestamp: number;
}

function processData(data: DataItem): number {
  return data.value;
}

// For truly dynamic data, use unknown
function parseJSON(json: string): unknown {
  return JSON.parse(json);
}
```

### Discriminated Unions
```typescript
// API Response Types
type ApiResponse<T> = 
  | { status: 'success'; data: T }
  | { status: 'error'; error: string }
  | { status: 'loading' };

function handleResponse<T>(response: ApiResponse<T>) {
  switch (response.status) {
    case 'success':
      return response.data; // TypeScript knows data exists
    case 'error':
      throw new Error(response.error);
    case 'loading':
      return null;
  }
}
```

### Const Assertions
```typescript
// Define constants with literal types
const TRADING_PAIRS = ['BTC/USDT', 'ETH/USDT', 'SOL/USDT'] as const;
type TradingPair = typeof TRADING_PAIRS[number];

const ORDER_TYPES = {
  MARKET: 'market',
  LIMIT: 'limit',
  STOP_LOSS: 'stop_loss',
  TAKE_PROFIT: 'take_profit'
} as const;

type OrderType = typeof ORDER_TYPES[keyof typeof ORDER_TYPES];
```

## React Patterns

### Component Structure
```typescript
// components/OrderBook/OrderBook.tsx
import React, { memo, useMemo, useCallback } from 'react';
import { useOrderBookData } from '@/hooks/useOrderBookData';
import type { OrderBookProps } from './OrderBook.types';
import { OrderBookRow } from './OrderBookRow';
import styles from './OrderBook.module.css';

export const OrderBook = memo<OrderBookProps>(({ 
  symbol, 
  depth = 20,
  onPriceClick 
}) => {
  const { bids, asks, loading, error } = useOrderBookData(symbol);
  
  const handlePriceClick = useCallback((price: number, side: 'bid' | 'ask') => {
    onPriceClick?.(price, side);
  }, [onPriceClick]);
  
  const sortedBids = useMemo(() => 
    bids.slice(0, depth).sort((a, b) => b.price - a.price),
    [bids, depth]
  );
  
  if (loading) return <OrderBookSkeleton />;
  if (error) return <OrderBookError error={error} />;
  
  return (
    <div className={styles.container}>
      <OrderBookHeader />
      <div className={styles.asks}>
        {asks.map((ask) => (
          <OrderBookRow 
            key={ask.price} 
            {...ask} 
            side="ask"
            onClick={handlePriceClick}
          />
        ))}
      </div>
      <OrderBookSpread bids={bids} asks={asks} />
      <div className={styles.bids}>
        {sortedBids.map((bid) => (
          <OrderBookRow 
            key={bid.price} 
            {...bid} 
            side="bid"
            onClick={handlePriceClick}
          />
        ))}
      </div>
    </div>
  );
});

OrderBook.displayName = 'OrderBook';
```

### Custom Hooks
```typescript
// hooks/useWebSocket.ts
import { useEffect, useRef, useState, useCallback } from 'react';

interface UseWebSocketOptions {
  url: string;
  reconnectInterval?: number;
  maxReconnectAttempts?: number;
  onMessage?: (data: unknown) => void;
  onError?: (error: Event) => void;
}

interface UseWebSocketReturn<T> {
  data: T | null;
  isConnected: boolean;
  error: Error | null;
  send: (message: unknown) => void;
  reconnect: () => void;
}

export function useWebSocket<T = unknown>({
  url,
  reconnectInterval = 3000,
  maxReconnectAttempts = 5,
  onMessage,
  onError
}: UseWebSocketOptions): UseWebSocketReturn<T> {
  const [data, setData] = useState<T | null>(null);
  const [isConnected, setIsConnected] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  
  const ws = useRef<WebSocket | null>(null);
  const reconnectCount = useRef(0);
  const reconnectTimeout = useRef<NodeJS.Timeout>();
  
  const connect = useCallback(() => {
    try {
      ws.current = new WebSocket(url);
      
      ws.current.onopen = () => {
        setIsConnected(true);
        setError(null);
        reconnectCount.current = 0;
      };
      
      ws.current.onmessage = (event) => {
        try {
          const parsed = JSON.parse(event.data) as T;
          setData(parsed);
          onMessage?.(parsed);
        } catch (err) {
          setError(new Error('Failed to parse message'));
        }
      };
      
      ws.current.onerror = (event) => {
        setError(new Error('WebSocket error'));
        onError?.(event);
      };
      
      ws.current.onclose = () => {
        setIsConnected(false);
        
        if (reconnectCount.current < maxReconnectAttempts) {
          reconnectTimeout.current = setTimeout(() => {
            reconnectCount.current++;
            connect();
          }, reconnectInterval);
        }
      };
    } catch (err) {
      setError(err as Error);
    }
  }, [url, reconnectInterval, maxReconnectAttempts, onMessage, onError]);
  
  useEffect(() => {
    connect();
    
    return () => {
      if (reconnectTimeout.current) {
        clearTimeout(reconnectTimeout.current);
      }
      ws.current?.close();
    };
  }, [connect]);
  
  const send = useCallback((message: unknown) => {
    if (ws.current?.readyState === WebSocket.OPEN) {
      ws.current.send(JSON.stringify(message));
    }
  }, []);
  
  const reconnect = useCallback(() => {
    ws.current?.close();
    reconnectCount.current = 0;
    connect();
  }, [connect]);
  
  return { data, isConnected, error, send, reconnect };
}
```

### State Management
```typescript
// store/marketDataSlice.ts
import { createSlice, PayloadAction } from '@reduxjs/toolkit';

interface MarketData {
  symbol: string;
  price: number;
  volume24h: number;
  change24h: number;
  timestamp: number;
}

interface MarketDataState {
  data: Record<string, MarketData>;
  subscriptions: Set<string>;
  connectionStatus: 'connected' | 'connecting' | 'disconnected';
}

const initialState: MarketDataState = {
  data: {},
  subscriptions: new Set(),
  connectionStatus: 'disconnected'
};

const marketDataSlice = createSlice({
  name: 'marketData',
  initialState,
  reducers: {
    updateMarketData: (state, action: PayloadAction<MarketData>) => {
      const { symbol } = action.payload;
      state.data[symbol] = action.payload;
    },
    
    batchUpdateMarketData: (state, action: PayloadAction<MarketData[]>) => {
      action.payload.forEach(item => {
        state.data[item.symbol] = item;
      });
    },
    
    subscribe: (state, action: PayloadAction<string>) => {
      state.subscriptions.add(action.payload);
    },
    
    unsubscribe: (state, action: PayloadAction<string>) => {
      state.subscriptions.delete(action.payload);
    },
    
    setConnectionStatus: (state, action: PayloadAction<MarketDataState['connectionStatus']>) => {
      state.connectionStatus = action.payload;
    }
  }
});

// Selectors with memoization
export const selectMarketData = (symbol: string) => (state: RootState) => 
  state.marketData.data[symbol];

export const selectAllMarketData = (state: RootState) => 
  Object.values(state.marketData.data);

export const { 
  updateMarketData, 
  batchUpdateMarketData,
  subscribe,
  unsubscribe,
  setConnectionStatus 
} = marketDataSlice.actions;
```

## Performance Optimization

### React.memo with Custom Comparison
```typescript
interface PriceDisplayProps {
  price: number;
  previousPrice: number;
  decimals?: number;
}

export const PriceDisplay = memo<PriceDisplayProps>(
  ({ price, previousPrice, decimals = 2 }) => {
    const trend = price > previousPrice ? 'up' : price < previousPrice ? 'down' : 'neutral';
    
    return (
      <span className={`price price--${trend}`}>
        {price.toFixed(decimals)}
      </span>
    );
  },
  (prevProps, nextProps) => {
    // Only re-render if price actually changed
    return prevProps.price === nextProps.price &&
           prevProps.decimals === nextProps.decimals;
  }
);
```

### Virtual Scrolling
```typescript
import { FixedSizeList as List } from 'react-window';

interface VirtualTradeListProps {
  trades: Trade[];
  height: number;
}

export const VirtualTradeList: React.FC<VirtualTradeListProps> = ({ trades, height }) => {
  const Row = ({ index, style }: { index: number; style: React.CSSProperties }) => {
    const trade = trades[index];
    
    return (
      <div style={style} className="trade-row">
        <span className={`price ${trade.side}`}>{trade.price}</span>
        <span className="quantity">{trade.quantity}</span>
        <span className="time">{formatTime(trade.timestamp)}</span>
      </div>
    );
  };
  
  return (
    <List
      height={height}
      itemCount={trades.length}
      itemSize={24}
      width="100%"
    >
      {Row}
    </List>
  );
};
```

### Code Splitting
```typescript
// Lazy load heavy components
const TradingChart = lazy(() => import('@/components/TradingChart'));
const AdvancedOrderForm = lazy(() => import('@/components/AdvancedOrderForm'));

// Use with Suspense
function TradingView() {
  return (
    <Suspense fallback={<ChartSkeleton />}>
      <TradingChart symbol="BTCUSDT" />
    </Suspense>
  );
}
```

## Testing

### Component Testing
```typescript
// __tests__/OrderBook.test.tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { OrderBook } from '@/components/OrderBook';
import { mockOrderBookData } from '@/test/mocks';

describe('OrderBook', () => {
  it('renders bids and asks', () => {
    render(<OrderBook symbol="BTCUSDT" data={mockOrderBookData} />);
    
    expect(screen.getByText('Bids')).toBeInTheDocument();
    expect(screen.getByText('Asks')).toBeInTheDocument();
    expect(screen.getAllByTestId('order-row')).toHaveLength(40);
  });
  
  it('calls onPriceClick when price is clicked', () => {
    const handleClick = jest.fn();
    render(
      <OrderBook 
        symbol="BTCUSDT" 
        data={mockOrderBookData}
        onPriceClick={handleClick}
      />
    );
    
    const firstBid = screen.getAllByTestId('bid-price')[0];
    fireEvent.click(firstBid);
    
    expect(handleClick).toHaveBeenCalledWith(
      mockOrderBookData.bids[0].price,
      'bid'
    );
  });
});
```

### Hook Testing
```typescript
// __tests__/useWebSocket.test.ts
import { renderHook, act } from '@testing-library/react-hooks';
import WS from 'jest-websocket-mock';
import { useWebSocket } from '@/hooks/useWebSocket';

describe('useWebSocket', () => {
  let server: WS;
  
  beforeEach(() => {
    server = new WS('ws://localhost:8080');
  });
  
  afterEach(() => {
    WS.clean();
  });
  
  it('connects and receives messages', async () => {
    const { result } = renderHook(() => 
      useWebSocket({ url: 'ws://localhost:8080' })
    );
    
    await server.connected;
    expect(result.current.isConnected).toBe(true);
    
    act(() => {
      server.send(JSON.stringify({ type: 'price', value: 50000 }));
    });
    
    expect(result.current.data).toEqual({ type: 'price', value: 50000 });
  });
  
  it('reconnects on disconnect', async () => {
    const { result } = renderHook(() => 
      useWebSocket({ 
        url: 'ws://localhost:8080',
        reconnectInterval: 100
      })
    );
    
    await server.connected;
    server.close();
    
    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 150));
    });
    
    expect(server).toHaveReceivedMessages([]);
  });
});
```

## CSS/Styling Standards

### CSS Modules
```css
/* OrderBook.module.css */
.container {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-primary);
  border-radius: var(--radius-md);
  overflow: hidden;
}

.row {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  padding: var(--spacing-xs) var(--spacing-sm);
  cursor: pointer;
  transition: background-color 0.1s ease;
}

.row:hover {
  background-color: var(--bg-hover);
}

.price {
  font-family: var(--font-mono);
  font-weight: 500;
}

.price--bid {
  color: var(--color-success);
}

.price--ask {
  color: var(--color-danger);
}
```

### Design Tokens
```typescript
// styles/tokens.ts
export const tokens = {
  colors: {
    primary: '#007AFF',
    success: '#34C759',
    danger: '#FF3B30',
    warning: '#FF9500',
    
    bgPrimary: '#000000',
    bgSecondary: '#1C1C1E',
    bgTertiary: '#2C2C2E',
    
    textPrimary: '#FFFFFF',
    textSecondary: '#8E8E93',
    textTertiary: '#48484A'
  },
  
  spacing: {
    xs: '4px',
    sm: '8px',
    md: '16px',
    lg: '24px',
    xl: '32px'
  },
  
  typography: {
    fontMono: '"SF Mono", "Monaco", "Inconsolata", monospace',
    fontSans: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
    
    sizes: {
      xs: '12px',
      sm: '14px',
      md: '16px',
      lg: '18px',
      xl: '24px'
    }
  }
} as const;
```

## Build Configuration

### Vite Configuration
```typescript
// vite.config.ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { visualizer } from 'rollup-plugin-visualizer';

export default defineConfig({
  plugins: [
    react(),
    visualizer({
      filename: './dist/stats.html',
      open: true,
      gzipSize: true,
      brotliSize: true
    })
  ],
  
  resolve: {
    alias: {
      '@': '/src',
      '@components': '/src/components',
      '@hooks': '/src/hooks',
      '@utils': '/src/utils'
    }
  },
  
  build: {
    target: 'es2022',
    minify: 'terser',
    sourcemap: true,
    rollupOptions: {
      output: {
        manualChunks: {
          'react-vendor': ['react', 'react-dom'],
          'redux-vendor': ['@reduxjs/toolkit', 'react-redux'],
          'chart-vendor': ['lightweight-charts'],
          'utils': ['date-fns', 'decimal.js']
        }
      }
    }
  },
  
  server: {
    port: 3000,
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true
      },
      '/ws': {
        target: 'ws://localhost:8080',
        ws: true
      }
    }
  }
});
```

## Code Quality

### ESLint Configuration
```json
{
  "extends": [
    "eslint:recommended",
    "plugin:@typescript-eslint/recommended",
    "plugin:react/recommended",
    "plugin:react-hooks/recommended"
  ],
  "rules": {
    "@typescript-eslint/explicit-function-return-type": "error",
    "@typescript-eslint/no-explicit-any": "error",
    "@typescript-eslint/no-unused-vars": ["error", { 
      "argsIgnorePattern": "^_",
      "varsIgnorePattern": "^_"
    }],
    "react/prop-types": "off",
    "react/react-in-jsx-scope": "off",
    "react-hooks/rules-of-hooks": "error",
    "react-hooks/exhaustive-deps": "warn"
  }
}
```

### Pre-commit Hooks
```json
// .husky/pre-commit
{
  "hooks": {
    "pre-commit": "lint-staged"
  }
}

// .lintstagedrc.json
{
  "*.{ts,tsx}": [
    "eslint --fix",
    "prettier --write"
  ],
  "*.css": [
    "stylelint --fix",
    "prettier --write"
  ]
}
```

## Accessibility

### ARIA Labels
```typescript
export const OrderForm: React.FC = () => {
  return (
    <form aria-label="Place order form">
      <label htmlFor="price-input">
        Price
        <input
          id="price-input"
          type="number"
          aria-describedby="price-help"
          aria-invalid={errors.price ? 'true' : 'false'}
        />
      </label>
      <span id="price-help" className="help-text">
        Enter the limit price for your order
      </span>
      
      <button
        type="submit"
        aria-busy={isSubmitting}
        disabled={isSubmitting}
      >
        {isSubmitting ? 'Placing Order...' : 'Place Order'}
      </button>
    </form>
  );
};
```

### Keyboard Navigation
```typescript
export const TradingPanel: React.FC = () => {
  const handleKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case 'b':
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          focusBuyForm();
        }
        break;
      case 's':
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          focusSellForm();
        }
        break;
    }
  };
  
  return (
    <div onKeyDown={handleKeyDown} tabIndex={0}>
      {/* Trading panel content */}
    </div>
  );
};
```

This completes the comprehensive standards documentation for the Jackbot platform.