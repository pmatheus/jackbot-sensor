// Uniswap V3 Integration for Jackbot-Sensor
// Advanced concentrated liquidity and swap functionality

use ethers::prelude::*;
use ethers::core::abi::{Abi, Token};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// Uniswap V3 contract addresses
const UNISWAP_V3_FACTORY: &str = "0x1F98431c8aD98523631AE4a59f267346ea31F984";
const UNISWAP_V3_ROUTER: &str = "0xE592427A0AEce92De3Edee1F18E0157C05861564";
const UNISWAP_V3_QUOTER: &str = "0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6";

#[derive(Debug, Clone)]
pub struct UniswapV3Client {
    provider: Arc<Provider<Http>>,
    factory: Contract<Provider<Http>>,
    router: Contract<Provider<Http>>,
    quoter: Contract<Provider<Http>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SwapParams {
    pub token_in: Address,
    pub token_out: Address,
    pub fee: u32,
    pub amount_in: U256,
    pub amount_out_minimum: U256,
    pub sqrt_price_limit_x96: U256,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LiquidityParams {
    pub token0: Address,
    pub token1: Address,
    pub fee: u32,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub amount0_desired: U256,
    pub amount1_desired: U256,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PoolInfo {
    pub address: Address,
    pub token0: Address,
    pub token1: Address,
    pub fee: u32,
    pub liquidity: u128,
    pub sqrt_price_x96: U256,
    pub tick: i32,
    pub observation_cardinality: u16,
}

impl UniswapV3Client {
    pub async fn new(provider: Arc<Provider<Http>>) -> Result<Self, Box<dyn std::error::Error>> {
        // Load ABIs (simplified - in production, load from files)
        let factory_abi: Abi = serde_json::from_str(include_str!("../abi/uniswap_v3_factory.json"))?;
        let router_abi: Abi = serde_json::from_str(include_str!("../abi/uniswap_v3_router.json"))?;
        let quoter_abi: Abi = serde_json::from_str(include_str!("../abi/uniswap_v3_quoter.json"))?;

        let factory = Contract::new(
            UNISWAP_V3_FACTORY.parse::<Address>()?,
            factory_abi,
            provider.clone(),
        );

        let router = Contract::new(
            UNISWAP_V3_ROUTER.parse::<Address>()?,
            router_abi,
            provider.clone(),
        );

        let quoter = Contract::new(
            UNISWAP_V3_QUOTER.parse::<Address>()?,
            quoter_abi,
            provider.clone(),
        );

        Ok(Self {
            provider,
            factory,
            router,
            quoter,
        })
    }

    pub async fn get_pool(&self, token0: Address, token1: Address, fee: u32) -> Result<PoolInfo, Box<dyn std::error::Error>> {
        // Get pool address from factory
        let pool_address: Address = self.factory
            .method::<_, Address>("getPool", (token0, token1, fee))?
            .call()
            .await?;

        // Get pool info
        let pool_abi: Abi = serde_json::from_str(include_str!("../abi/uniswap_v3_pool.json"))?;
        let pool = Contract::new(pool_address, pool_abi, self.provider.clone());

        let liquidity: u128 = pool.method::<_, u128>("liquidity", ())?.call().await?;
        let slot0: (U256, i32, u16, u16, u16, u8, bool) = pool
            .method::<_, (U256, i32, u16, u16, u16, u8, bool)>("slot0", ())?
            .call()
            .await?;

        Ok(PoolInfo {
            address: pool_address,
            token0,
            token1,
            fee,
            liquidity,
            sqrt_price_x96: slot0.0,
            tick: slot0.1,
            observation_cardinality: slot0.2,
        })
    }

    pub async fn quote_exact_input(&self, params: &SwapParams) -> Result<U256, Box<dyn std::error::Error>> {
        let amount_out: U256 = self.quoter
            .method::<_, U256>(
                "quoteExactInputSingle",
                (
                    params.token_in,
                    params.token_out,
                    params.fee,
                    params.amount_in,
                    params.sqrt_price_limit_x96,
                ),
            )?
            .call()
            .await?;

        Ok(amount_out)
    }

    pub async fn swap(&self, params: SwapParams, wallet: &LocalWallet) -> Result<TransactionReceipt, Box<dyn std::error::Error>> {
        let client = SignerMiddleware::new(self.provider.clone(), wallet.clone());
        let router_with_signer = self.router.connect(Arc::new(client));

        let tx = router_with_signer
            .method::<_, ()>(
                "exactInputSingle",
                (
                    params.token_in,
                    params.token_out,
                    params.fee,
                    wallet.address(),
                    U256::from(u64::MAX), // deadline
                    params.amount_in,
                    params.amount_out_minimum,
                    params.sqrt_price_limit_x96,
                ),
            )?
            .send()
            .await?
            .await?;

        Ok(tx.unwrap())
    }

    pub async fn add_liquidity(&self, params: LiquidityParams, wallet: &LocalWallet) -> Result<TransactionReceipt, Box<dyn std::error::Error>> {
        let client = SignerMiddleware::new(self.provider.clone(), wallet.clone());
        let router_with_signer = self.router.connect(Arc::new(client));

        // Mint new position
        let tx = router_with_signer
            .method::<_, ()>(
                "mint",
                (
                    params.token0,
                    params.token1,
                    params.fee,
                    params.tick_lower,
                    params.tick_upper,
                    params.amount0_desired,
                    params.amount1_desired,
                    U256::zero(), // amount0Min
                    U256::zero(), // amount1Min
                    wallet.address(),
                    U256::from(u64::MAX), // deadline
                ),
            )?
            .send()
            .await?
            .await?;

        Ok(tx.unwrap())
    }

    pub async fn calculate_impermanent_loss(&self, pool: &PoolInfo, initial_price: f64) -> f64 {
        // Calculate current price from sqrt_price_x96
        let sqrt_price = pool.sqrt_price_x96.as_u128() as f64 / (1u128 << 96) as f64;
        let current_price = sqrt_price * sqrt_price;

        // IL = 2 * sqrt(price_ratio) / (1 + price_ratio) - 1
        let price_ratio = current_price / initial_price;
        let il = 2.0 * price_ratio.sqrt() / (1.0 + price_ratio) - 1.0;

        il * 100.0 // Return as percentage
    }

    pub async fn optimize_range(&self, pool: &PoolInfo, volatility: f64) -> (i32, i32) {
        // Calculate optimal tick range based on volatility
        let tick_spacing = match pool.fee {
            500 => 10,
            3000 => 60,
            10000 => 200,
            _ => 60,
        };

        let range_multiplier = 2.0 * volatility.sqrt();
        let tick_range = (range_multiplier * 1000.0) as i32;

        let tick_lower = ((pool.tick - tick_range) / tick_spacing) * tick_spacing;
        let tick_upper = ((pool.tick + tick_range) / tick_spacing) * tick_spacing;

        (tick_lower, tick_upper)
    }
}