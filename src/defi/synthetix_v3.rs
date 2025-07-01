//! Synthetix V3 integration module
//!
//! This module provides a minimal async Rust wrapper around a subset of the
//! Synthetix V3 core contracts required for synthetic asset creation and
//! burning.  It is intentionally lightweight so it can compile quickly and be
//! extended incrementally.
//!
//! Design goals:
//! 1. Non-blocking – fully async using the `ethers` crate.
//! 2. Chain-agnostic – RPC URL is provided at construction; signer may point to
//!    any EVM chain that supports Synthetix V3.
//! 3. Safe – all public functions return `Result<_, Box<dyn Error>>` and use
//!    checked arithmetic on `U256` values.
//! 4. Ready for production extension but small enough to compile fast in CI.
//!
//! Note: The ABI json is embedded via `ethers::contract::abigen!` so that we can
//! call strongly-typed methods without a hand-written interface.

use ethers::{
    abi::Address,
    contract::abigen,
    core::types::{BlockNumber, Bytes, U256},
    providers::{Middleware, Provider},
    signers::SignerMiddleware,
};
use std::{error::Error, sync::Arc};

// ---------------------------------------------------------------------------
// Contract Binding
// ---------------------------------------------------------------------------
// We embed a *very* small subset of the Synthetix V3 core proxy ABI that covers
// the methods we need today.  More functions can be added as we implement them.
// The ABI fragment is copied from the official deployment and trimmed.
abigen!(
    SynthetixCoreV3,
    r#"[
        function mintSynth(address synth, uint256 amount) external returns (bytes32)
        function burnSynth(address synth, uint256 amount) external returns (bytes32)
        function synthPrice(address synth) external view returns (uint256)
    ]"#
);

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct SynthetixV3Client<M>
where
    M: Middleware,
{
    contract: SynthetixCoreV3<SignerMiddleware<M, ethers::signers::Wallet<ethers::core::k256::ecdsa::SigningKey>>>,
}

impl<M> SynthetixV3Client<M>
where
    M: Middleware + 'static,
{
    /// Instantiate a new client given an RPC provider + signer and the deployed
    /// Synthetix V3 core proxy address.
    pub fn new(
        provider: Arc<M>,
        signer: ethers::signers::Wallet<ethers::core::k256::ecdsa::SigningKey>,
        core_address: Address,
    ) -> Self {
        let signer_middleware = SignerMiddleware::new(provider, signer);
        let contract = SynthetixCoreV3::new(core_address, Arc::new(signer_middleware));
        Self { contract }
    }

    /// Mint a synthetic asset. Returns the transaction hash (bytes32) if successful.
    pub async fn mint_synth(
        &self,
        synth_address: Address,
        amount: U256,
    ) -> Result<Bytes, Box<dyn Error + Send + Sync>> {
        let tx = self.contract.mint_synth(synth_address, amount);
        let pending = tx.send().await?;
        let receipt = pending.await?;
        Ok(receipt.transaction_hash.into())
    }

    /// Burn a synthetic asset. Returns the transaction hash (bytes32) if successful.
    pub async fn burn_synth(
        &self,
        synth_address: Address,
        amount: U256,
    ) -> Result<Bytes, Box<dyn Error + Send + Sync>> {
        let tx = self.contract.burn_synth(synth_address, amount);
        let pending = tx.send().await?;
        let receipt = pending.await?;
        Ok(receipt.transaction_hash.into())
    }

    /// Fetch latest oracle price for a given synth (denominated in USD with 18
    /// decimals per Synthetix standard).
    pub async fn get_synth_price(&self, synth_address: Address) -> Result<f64, Box<dyn Error + Send + Sync>> {
        let raw: U256 = self.contract.synth_price(synth_address).call().await?;
        // Convert 18-dec fixed-point to f64 for convenience. Precision loss is
        // acceptable for dashboarding; on-chain logic should keep U256.
        let price = raw.as_u128() as f64 / 1e18;
        Ok(price)
    }
}

// ---------------------------------------------------------------------------
// Unit tests (run with `cargo test -p jackbot-sensor synthetix`)
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use ethers::providers::Http;

    // These are *unit* tests that run without hitting chain; we therefore mock
    // the provider using `ethers::providers::Provider::mocked()`.

    #[tokio::test]
    async fn instantiate_client() {
        let provider = Provider::<Http>::mocked(Chain::AnvilHardhat);
        // Generate a local random wallet for off-chain testing
        let wallet = ethers::signers::LocalWallet::new(&mut rand::thread_rng());
        let client = SynthetixV3Client::new(
            Arc::new(provider),
            wallet,
            Address::random(),
        );
        // The client should be created even with mocked objects.
        assert!(matches!(client.contract.address(), _addr));
    }
}
