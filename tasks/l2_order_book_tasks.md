# L2 Order Book Tasks

- Ensure all tests pass for every exchange after changes to L2 order book modules.
- Resolve clippy warnings in `jackbot-integration` related to the order book modules.
- Fix formatting issues preventing `cargo fmt` from running on order book code.
- Address failing `jackbot-data` tests for L2 order book (e.g., missing `SubscriptionId`, tokio time utilities).
