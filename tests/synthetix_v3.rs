//! Integration test for Synthetix V3 client against a live (or forked) RPC.
//!
//! The test is **ignored by default** because it requires:
//! 1. A working Ethereum RPC node that has access to the Synthetix V3 deployment
//!    (e.g. mainnet, Optimism, or a local Anvil fork)
//! 2. Two environment variables:
//!    * `ETH_RPC_URL` – the RPC endpoint
//!    * `PRIVATE_KEY` – a funded test private key on that network
//! 3. `SYNTHETIX_CORE_ADDRESS` – address of the deployed Synthetix V3 core proxy.
//!
//! Run with:
//! ```bash
//! ETH_RPC_URL=... PRIVATE_KEY=... SYNTHETIX_CORE_ADDRESS=... \
//!     cargo test --test synthetix_v3 -- --ignored --nocapture
//! ```
//!
//! The test mints 1 wei of sUSD (or other synth) then burns it, asserting the
//! calls do not revert.

use ethers::{
    core::types::{Address, U256},
    providers::{Http, Provider},
    signers::LocalWallet,
};
use jackbot_sensor::defi::synthetix_v3::SynthetixV3Client;
use std::{env, sync::Arc};

#[tokio::test]
#[ignore]
async fn mint_and_burn_synth_live() {
    // Skip if env vars not present
    let rpc_url = env::var("ETH_RPC_URL").expect("ETH_RPC_URL not set");
    let priv_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY not set");
    let core_addr: Address = env::var("SYNTHETIX_CORE_ADDRESS")
        .expect("SYNTHETIX_CORE_ADDRESS not set")
        .parse()
        .expect("Invalid address");
    let synth_addr: Address = env::var("SYNTH_ADDRESS")
        .unwrap_or_else(|_| "0x8c6f28f2F1A3C87F3093f2C13fa9a41f385A636f".to_string()) // sUSD on OP mainnet
        .parse()
        .expect("Invalid synth address");

    let provider = Provider::<Http>::try_from(rpc_url).expect("provider");
    let wallet: LocalWallet = priv_key.parse().expect("wallet");

    let client = SynthetixV3Client::new(Arc::new(provider), wallet, core_addr);

    // Mint 1 wei to keep gas usage negligible
    let _tx_hash = client
        .mint_synth(synth_addr, U256::from(1u64))
        .await
        .expect("mint failed");

    let _tx_hash2 = client
        .burn_synth(synth_addr, U256::from(1u64))
        .await
        .expect("burn failed");
}
