# Jackbot Sensor - High-Performance Crypto Market Data & Execution Engine

[![Status](https://img.shields.io/badge/status-under%20development-yellow.svg?style=for-the-badge)](https://github.com/pmatheus/jackbot-sensor)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Exchanges](https://img.shields.io/badge/exchanges-11%20planned-blue.svg?style=for-the-badge)](https://github.com/pmatheus/jackbot-sensor)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

## 🚀 Executive Summary

> ⚠️ **Status: Under Active Development** - This project is a work in progress. While the architecture and design are production-ready, implementation is ongoing.

**Jackbot Sensor** is the planned market data and execution layer of the larger Jackbot Terminal project - a professional crypto trading platform. Being built in Rust for performance and reliability, it will provide real-time connectivity to 11 major exchanges with advanced market making capabilities.

**Planned Key Features**:
- **Horizontal Scalability**: Designed for t4g.nano/micro instances
- **Market Making Focus**: Advanced order management and spread optimization
- **Exchange Interface**: Clean separation of concerns
- **Open Source**: Complete transparency in execution and data handling

## 💼 Market Making Excellence

### Planned Market Making Features
- **Dynamic Spread Calculation**: Will adjust spreads based on volatility and market conditions
- **Inventory Management**: Will skew prices to maintain balanced positions
- **Adverse Selection Protection**: Will detect and avoid toxic order flow
- **Competitive Quoting**: Will monitor and match competitor spreads
- **Multi-Exchange Coordination**: Will manage positions across all connected exchanges
- **Multi-Exchange Arbitrage**: Will identify and execute profitable arbitrage opportunities across different exchanges

### Production Architecture
- **Efficient Resource Usage**: Each t4g.nano handles multiple symbols (Rust's low memory footprint)
- **Horizontal Scaling**: Deploy additional instances as symbol count grows
- **Kafka Cluster**: 3x t4g.micro instances for reliable messaging
- **Microservice Design**: Each sensor instance handles a group of related symbols
- **Cost Efficient**: t4g.nano instances maximize performance per dollar


## 🎯 Market Making Capabilities

### Market Making Strategy Features
- **Multi-Exchange Market Making**: Simultaneous quoting across 11 exchanges
- **Inventory Management**: Real-time position tracking and risk limits
- **Smart Order Placement**: Queue position optimization
- **Competitive Quoting**: Dynamic spread adjustment based on competition
- **Adverse Selection Protection**: Toxic flow detection and avoidance
- **Cross-Exchange Hedging**: Automatic position balancing

## 🔧 Exchange Integration

### Planned Exchange Support
| Exchange | REST API | WebSocket | Market Making | Order Types | Status |
|----------|----------|-----------|---------------|-------------|---------|
| Binance | 🚧 | 🚧 | 🚧 | Limit, Market, Stop | In Progress |
| Coinbase | 📋 | 📋 | 📋 | Limit, Market, Stop | Planned |
| Bybit | 📋 | 📋 | 📋 | Limit, Market, Stop | Planned |
| OKX | 📋 | 📋 | 📋 | Limit, Market, Stop | Planned |
| Kraken | 📋 | 📋 | 📋 | Limit, Market, Stop | Planned |
| Bitget | 📋 | 📋 | 📋 | Limit, Market, Stop | Planned |
| KuCoin | 📋 | 📋 | 📋 | Limit, Market, Stop | Planned |
| MEXC | 📋 | 📋 | 📋 | Limit, Market, Stop | Planned |
| Gate.io | 📋 | 📋 | 📋 | Limit, Market, Stop | Planned |
| Crypto.com | 📋 | 📋 | 📋 | Limit, Market, Stop | Planned |
| Hyperliquid | 📋 | 📋 | 📋 | Limit, Market, Stop | Planned |

🚧 = In Development | 📋 = Planned | ✅ = Completed

### Technical Design Principles
- **Efficient Symbol Grouping**: Each t4g.nano handles 10-30+ symbols based on activity
- **Rust Memory Efficiency**: Low footprint allows multiple symbols per instance
- **Kafka Integration**: Reliable message delivery to backend systems
- **Smart Scaling**: Add instances based on load, not just symbol count

## 🚀 Getting Started

### Prerequisites
- Rust 1.70+ with cargo
- Docker and Docker Compose
- Exchange API keys (testnet recommended for development)

### Quick Start
**Development Setup**: Standard Rust development environment with exchange configuration and symbol selection.

**Deployment Options**: Local development build, Docker containerization, or cloud deployment with horizontal scaling.

**Context7 Usage**: Use Context7 MCP to research Rust project setup, Docker deployment patterns, and microservices architecture.


## 🏗️ Infrastructure Design

### Horizontal Scaling Architecture
- **Resource Optimization**: Each t4g.nano efficiently handles 10-30+ symbols
- **Logical Grouping**: Instances organized by symbol type (majors, alts, stables, etc.)
- **Kafka Cluster**: 3x t4g.micro for message bus redundancy
- **Load Balancing**: Distribute high-activity symbols across instances
- **Fault Isolation**: Instance failure only affects its symbol group

### Integration with Jackbot Terminal
The sensors are designed as the data collection and execution layer for the larger Jackbot Terminal project.

## 📊 Real-World Use Cases

### Market Making Operations
- Deploy sensors based on logical symbol groupings and activity levels
- Each t4g.nano instance efficiently manages order books for 10-30+ symbols
- Group high-activity pairs together for load distribution
- Rust's efficiency allows handling multiple symbols without performance degradation
- Scale by adding instances only when existing ones approach capacity

### Arbitrage Detection
- Sensors stream normalized market data to Kafka
- Backend systems identify cross-exchange opportunities
- Execute trades through sensor's order management system
- Sub-second detection to execution pipeline

### Portfolio Management
- Aggregate positions across all exchanges
- Real-time P&L calculation and risk metrics
- Automated rebalancing through sensor execution layer
- Historical data storage for performance analysis

## 📚 Project Documentation

### Core Documentation
- [Market Making Guide](docs/MARKET_MAKING_ENGINE.md) - Advanced MM strategies
- [Exchange Integration](docs/EXCHANGE_INTEGRATION_GUIDE.md) - Adding new exchanges
- [Performance Guide](docs/PERFORMANCE_GUIDE.md) - Optimization techniques
- [Strategy Framework](docs/STRATEGY_FRAMEWORK.md) - Building custom strategies

### Architecture
- [System Architecture](docs/README.md) - Overall design
- [Order Management](docs/ADVANCED_EXECUTION_ABSTRACTION.md) - Execution engine
- [Risk Framework](docs/RISK_FRAMEWORK.md) - Risk controls

## 🎓 Why This Project Matters

### For Wall Street
- **Production Architecture**: Designed with institutional requirements in mind
- **Market Making Focus**: Sophisticated MM capabilities planned for open source
- **Clean Design**: Microservice architecture that scales horizontally
- **Professional Standards**: Built following best practices for financial systems

### Technical Vision
- **Rust Performance**: Zero-copy parsing, lock-free data structures
- **Fault Tolerance**: Isolated failures, automatic recovery design
- **Horizontal Scaling**: Add capacity by deploying additional t4g.nano instances as needed
- **Open Source**: Complete transparency in execution layer when complete

> **Note**: This README represents the planned architecture and capabilities. Current implementation status varies by component.

## 🤝 Contributing

We welcome contributions from the community! Here's how you can help:

### Pull Requests
- Fork the repository and create your feature branch
- Write clear commit messages and PR descriptions
- Follow our coding standards and include tests
- Update documentation as needed
- Submit PRs against the `main` branch

### Issues & Support
- **Bug Reports**: Open an issue with detailed reproduction steps
- **Feature Requests**: Use the issue template to propose new features
- **Questions**: Check existing issues or start a discussion
- **Support**: [GitHub Issues](https://github.com/pmatheus/jackbot-sensor/issues)

### Guidelines
- Be respectful and constructive in discussions
- Search existing issues before creating new ones
- Follow the code of conduct
- Help others and share knowledge

---

*Built for Wall Street. Open-sourced for everyone.*