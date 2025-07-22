# Jackbot Sensor Documentation

> **Note**: Documentation has been consolidated to improve navigation and reduce redundancy.

## 📍 Documentation Location

All Jackbot documentation is now centralized in the main `/docs` directory:

### Quick Links

- **[Architecture](../../docs/architecture/sensor.md)** - Sensor system design and data flow
- **[Exchange Integration](../../docs/exchanges/)** - How sensor connects to exchanges
- **[Development Guide](../../docs/deployment/development.md#sensor-development)** - Sensor development setup
- **[Performance Guide](../../docs/guides/performance.md)** - Optimizing market data collection

### Component-Specific Docs

For sensor-specific implementation:
- `src/` - Rust source code with inline documentation
- `config/` - Exchange configuration examples
- `benches/` - Performance benchmarks

## 🚀 Quick Start

```bash
# Build sensor
cargo build --release

# Configure exchanges
cp config/exchanges.example.toml config/exchanges.toml
# Edit with your API keys

# Run sensor
KAFKA_BROKERS=localhost:9092 cargo run --release
```

For detailed instructions, see the [Development Setup Guide](../../docs/deployment/development.md#sensor-development).

## 📚 Main Documentation

Visit the [main documentation hub](../../docs/) for comprehensive guides on:
- System architecture
- Exchange integrations
- Deployment procedures
- Performance tuning
- Monitoring

---

*This README serves as a pointer to the centralized documentation. For the most up-to-date information, always refer to the main `/docs` directory.*