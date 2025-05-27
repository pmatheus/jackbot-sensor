# General Build Tasks

- Resolve formatting errors preventing `cargo fmt --all` from completing successfully across the workspace.
- Address clippy warnings in `jackbot-integration` so `cargo clippy --all-targets --all-features -- -D warnings` passes.
- Fix formatting issues in `jackbot/src/risk/exposure.rs`, the Crypto.com client module, and TA tests.
- Repair failing tests in `jackbot-data` (missing `SubscriptionId`, tokio time utilities).
- Fix clippy warnings in `jackbot-integration` caused by deprecated `rand` APIs and large `SocketError` variants.
- Resolve syntax errors preventing `cargo fmt --all` from succeeding:
  - `jackbot/src/risk/exposure.rs` mismatched closing brace.
  - `jackbot-execution/src/client/cryptocom.rs` conflicts with `cryptocom/mod.rs`.
  - `jackbot-execution/tests/advanced_orders_compile.rs` uses the reserved keyword `mod`.
  - `jackbot-ta/tests/integration.rs` uses the reserved keyword `gen`.
