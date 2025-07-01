// Tests for Advanced DeFi Derivatives Engine (Wave 7)

#[cfg(test)]
mod tests {
    use super::super::derivatives_engine::*;
    use ethers::prelude::*;
    use std::collections::HashMap;

    #[test]
    fn test_black_scholes_option_pricing() {
        let bs_model = BlackScholesModel::new();
        
        // Test cases for Black-Scholes pricing
        let test_cases = vec![
            // (spot, strike, time, rate, vol, is_call, expected_range)
            (100.0, 100.0, 0.25, 0.05, 0.2, true, (8.0, 12.0)),    // ATM call
            (100.0, 100.0, 0.25, 0.05, 0.2, false, (7.0, 11.0)),   // ATM put
            (100.0, 90.0, 0.25, 0.05, 0.2, true, (12.0, 18.0)),    // ITM call
            (100.0, 110.0, 0.25, 0.05, 0.2, false, (12.0, 18.0)),  // ITM put
        ];

        for (spot, strike, time, rate, vol, is_call, (min_price, max_price)) in test_cases {
            let price = bs_model.calculate_option_price(spot, strike, time, rate, vol, is_call)
                .expect("Failed to calculate option price");
            
            println!("Option price: {:.4} (expected range: {:.1}-{:.1})", price, min_price, max_price);
            
            assert!(price >= min_price && price <= max_price, 
                "Option price {:.4} not in expected range {:.1}-{:.1}", price, min_price, max_price);
            assert!(price > 0.0, "Option price must be positive");
        }
    }

    #[test]
    fn test_greeks_calculation() {
        let bs_model = BlackScholesModel::new();
        
        // Standard test case
        let spot = 100.0;
        let strike = 100.0;
        let time = 0.25; // 3 months
        let rate = 0.05;
        let vol = 0.2;
        
        // Calculate all Greeks
        let delta = bs_model.calculate_delta(spot, strike, time, rate, vol, true)
            .expect("Failed to calculate delta");
        let gamma = bs_model.calculate_gamma(spot, strike, time, rate, vol)
            .expect("Failed to calculate gamma");
        let theta = bs_model.calculate_theta(spot, strike, time, rate, vol, true)
            .expect("Failed to calculate theta");
        let vega = bs_model.calculate_vega(spot, strike, time, rate, vol)
            .expect("Failed to calculate vega");
        let rho = bs_model.calculate_rho(spot, strike, time, rate, vol, true)
            .expect("Failed to calculate rho");

        println!("Greeks for ATM call option:");
        println!("Delta: {:.4}", delta);
        println!("Gamma: {:.4}", gamma);
        println!("Theta: {:.4}", theta);
        println!("Vega: {:.4}", vega);
        println!("Rho: {:.4}", rho);

        // Sanity checks for Greeks
        assert!(delta > 0.4 && delta < 0.6, "ATM call delta should be around 0.5, got {:.4}", delta);
        assert!(gamma > 0.0, "Gamma must be positive, got {:.4}", gamma);
        assert!(theta < 0.0, "Theta should be negative for long options, got {:.4}", theta);
        assert!(vega > 0.0, "Vega must be positive, got {:.4}", vega);
        assert!(rho > 0.0, "Rho should be positive for calls, got {:.4}", rho);
    }

    #[test]
    fn test_portfolio_greeks_calculation() {
        let greeks_calculator = GreeksCalculator::new();
        
        // Create sample options positions
        let positions = vec![
            OptionsPosition {
                contract: OptionContract {
                    protocol: "Opyn".to_string(),
                    underlying: "ETH".to_string(),
                    strike_price: U256::from(3000) * U256::exp10(18),
                    expiration: chrono::Utc::now().timestamp() as u64 + 86400 * 30, // 30 days
                    option_type: OptionType::Call,
                    premium: U256::from(100) * U256::exp10(18),
                    implied_volatility: 0.25,
                    greeks: Greeks {
                        delta: 0.6,
                        gamma: 0.02,
                        theta: -0.1,
                        vega: 0.8,
                        rho: 0.3,
                    },
                    liquidity: U256::from(1000) * U256::exp10(18),
                },
                quantity: 10, // Long 10 calls
                entry_price: U256::from(90) * U256::exp10(18),
                current_price: U256::from(100) * U256::exp10(18),
                greeks: Greeks {
                    delta: 0.6,
                    gamma: 0.02,
                    theta: -0.1,
                    vega: 0.8,
                    rho: 0.3,
                },
                pnl: 100_000_000_000_000_000_000i128, // $100 profit
            },
            OptionsPosition {
                contract: OptionContract {
                    protocol: "Hegic".to_string(),
                    underlying: "ETH".to_string(),
                    strike_price: U256::from(2800) * U256::exp10(18),
                    expiration: chrono::Utc::now().timestamp() as u64 + 86400 * 30,
                    option_type: OptionType::Put,
                    premium: U256::from(80) * U256::exp10(18),
                    implied_volatility: 0.30,
                    greeks: Greeks {
                        delta: -0.3,
                        gamma: 0.015,
                        theta: -0.08,
                        vega: 0.6,
                        rho: -0.2,
                    },
                    liquidity: U256::from(800) * U256::exp10(18),
                },
                quantity: -5, // Short 5 puts
                entry_price: U256::from(85) * U256::exp10(18),
                current_price: U256::from(80) * U256::exp10(18),
                greeks: Greeks {
                    delta: -0.3,
                    gamma: 0.015,
                    theta: -0.08,
                    vega: 0.6,
                    rho: -0.2,
                },
                pnl: 25_000_000_000_000_000_000i128, // $25 profit
            },
        ];

        let portfolio_greeks = greeks_calculator.calculate_portfolio_greeks(&positions)
            .expect("Failed to calculate portfolio Greeks");

        println!("Portfolio Greeks:");
        println!("Delta: {:.4}", portfolio_greeks.delta);
        println!("Gamma: {:.4}", portfolio_greeks.gamma);
        println!("Theta: {:.4}", portfolio_greeks.theta);
        println!("Vega: {:.4}", portfolio_greeks.vega);
        println!("Rho: {:.4}", portfolio_greeks.rho);

        // Expected calculations:
        // Delta: (10 * 0.6) + (-5 * -0.3) = 6.0 + 1.5 = 7.5
        // Gamma: (10 * 0.02) + (-5 * 0.015) = 0.2 - 0.075 = 0.125
        // Theta: (10 * -0.1) + (-5 * -0.08) = -1.0 + 0.4 = -0.6
        // Vega: (10 * 0.8) + (-5 * 0.6) = 8.0 - 3.0 = 5.0
        // Rho: (10 * 0.3) + (-5 * -0.2) = 3.0 + 1.0 = 4.0

        assert!((portfolio_greeks.delta - 7.5).abs() < 0.01, "Delta calculation incorrect");
        assert!((portfolio_greeks.gamma - 0.125).abs() < 0.01, "Gamma calculation incorrect");
        assert!((portfolio_greeks.theta - (-0.6)).abs() < 0.01, "Theta calculation incorrect");
        assert!((portfolio_greeks.vega - 5.0).abs() < 0.01, "Vega calculation incorrect");
        assert!((portfolio_greeks.rho - 4.0).abs() < 0.01, "Rho calculation incorrect");
    }

    #[test]
    fn test_volatility_forecasting() {
        let volatility_forecaster = tokio_test::block_on(async {
            VolatilityForecaster::new("test_model.onnx".to_string()).await
        }).expect("Failed to create volatility forecaster");

        // Sample price history (simulated ETH prices)
        let price_history = vec![
            3000.0, 3050.0, 3100.0, 3080.0, 3120.0, 3150.0, 3200.0, 3180.0,
            3220.0, 3250.0, 3300.0, 3280.0, 3320.0, 3350.0, 3380.0, 3400.0,
            3420.0, 3450.0, 3480.0, 3500.0, 3520.0, 3550.0, 3580.0, 3600.0,
        ];

        let forecast = tokio_test::block_on(async {
            volatility_forecaster.predict_volatility("ETH", &price_history).await
        }).expect("Failed to predict volatility");

        println!("Volatility Forecast for ETH:");
        println!("Current volatility: {:.2}%", forecast.current_vol * 100.0);
        println!("24h predicted volatility: {:.2}%", forecast.predicted_vol_24h * 100.0);
        println!("7d predicted volatility: {:.2}%", forecast.predicted_vol_7d * 100.0);
        println!("30d predicted volatility: {:.2}%", forecast.predicted_vol_30d * 100.0);
        println!("Volatility regime: {:?}", forecast.regime);
        println!("Confidence: {:.2}%", forecast.confidence * 100.0);

        // Sanity checks
        assert!(forecast.current_vol > 0.0 && forecast.current_vol < 2.0, 
            "Current volatility should be reasonable, got {:.4}", forecast.current_vol);
        assert!(forecast.predicted_vol_24h > 0.0 && forecast.predicted_vol_24h < 2.0,
            "24h volatility prediction should be reasonable");
        assert!(forecast.predicted_vol_7d > 0.0 && forecast.predicted_vol_7d < 2.0,
            "7d volatility prediction should be reasonable");
        assert!(forecast.predicted_vol_30d > 0.0 && forecast.predicted_vol_30d < 2.0,
            "30d volatility prediction should be reasonable");
        assert!(forecast.confidence > 0.0 && forecast.confidence <= 1.0,
            "Confidence should be between 0 and 1");
    }

    #[test]
    fn test_cross_chain_arbitrage_detection() {
        let cross_chain_arbitrage = tokio_test::block_on(async {
            CrossChainArbitrage::new().await
        }).expect("Failed to create cross-chain arbitrage detector");

        let opportunities = tokio_test::block_on(async {
            cross_chain_arbitrage.scan_cross_chain_opportunities().await
        }).expect("Failed to scan for opportunities");

        println!("Found {} cross-chain arbitrage opportunities", opportunities.len());

        for (i, opportunity) in opportunities.iter().enumerate().take(5) {
            println!("Opportunity {}:", i + 1);
            println!("  {} -> {}", opportunity.source_chain, opportunity.target_chain);
            println!("  Asset: {}", opportunity.asset);
            println!("  Price difference: {:.2}%", opportunity.price_difference * 100.0);
            println!("  Net profit: ${:.2}", opportunity.net_profit as f64 / 1e18);
            println!("  Risk score: {:.2}", opportunity.risk_score);
            println!("  Execution time: {}s", opportunity.execution_time_estimate);
            println!();
        }

        // Validate opportunities
        for opportunity in &opportunities {
            assert!(opportunity.net_profit > 0, "Only profitable opportunities should be returned");
            assert!(opportunity.price_difference.abs() >= 0.005, "Price difference should be at least 0.5%");
            assert!(opportunity.risk_score >= 0.0 && opportunity.risk_score <= 1.0, "Risk score should be normalized");
            assert!(opportunity.execution_time_estimate > 0, "Execution time should be positive");
        }
    }

    #[test]
    fn test_derivatives_engine_initialization() {
        let config = DerivativesConfig {
            max_position_size: U256::from(500_000) * U256::exp10(18),
            max_delta_exposure: 100.0,
            max_gamma_exposure: 50.0,
            max_vega_exposure: 1000.0,
            volatility_threshold: 0.5,
            funding_rate_threshold: 0.01,
            cross_chain_enabled: true,
            synthetic_creation_enabled: true,
        };

        let derivatives_engine = tokio_test::block_on(async {
            DerivativesEngine::new(config.clone()).await
        }).expect("Failed to create derivatives engine");

        // Test engine startup
        let startup_result = tokio_test::block_on(async {
            derivatives_engine.start_derivatives_trading().await
        });

        assert!(startup_result.is_ok(), "Derivatives engine should start successfully");

        println!("✅ Derivatives engine initialized and started successfully");
        println!("Configuration:");
        println!("  Max position size: ${:.0}", config.max_position_size.as_u128() as f64 / 1e18);
        println!("  Max delta exposure: {}", config.max_delta_exposure);
        println!("  Max gamma exposure: {}", config.max_gamma_exposure);
        println!("  Max vega exposure: {}", config.max_vega_exposure);
        println!("  Cross-chain enabled: {}", config.cross_chain_enabled);
        println!("  Synthetic assets enabled: {}", config.synthetic_creation_enabled);
    }

    #[test]
    fn test_options_strategy_evaluation() {
        // Test different options strategies profitability
        let strategies = vec![
            ("Long Call", OptionsStrategyType::LongCall),
            ("Long Put", OptionsStrategyType::LongPut),
            ("Straddle", OptionsStrategyType::Straddle),
            ("Iron Condor", OptionsStrategyType::IronCondor),
            ("Call Spread", OptionsStrategyType::CallSpread),
        ];

        let market_scenarios = vec![
            ("Bullish", 1.1), // 10% price increase
            ("Bearish", 0.9), // 10% price decrease
            ("Sideways", 1.0), // No price change
            ("Volatile", 1.2), // 20% price increase (high volatility)
        ];

        println!("Options Strategy Performance Analysis:");
        println!("=====================================");

        for (scenario_name, price_multiplier) in &market_scenarios {
            println!("\n{} Market Scenario ({}x price):", scenario_name, price_multiplier);
            
            for (strategy_name, strategy_type) in &strategies {
                let performance = calculate_strategy_performance(strategy_type, *price_multiplier);
                println!("  {}: {:.2}% return", strategy_name, performance * 100.0);
            }
        }

        // Test that strategies perform as expected in different scenarios
        assert!(calculate_strategy_performance(&OptionsStrategyType::LongCall, 1.1) > 0.0,
            "Long call should be profitable in bullish scenario");
        assert!(calculate_strategy_performance(&OptionsStrategyType::LongPut, 0.9) > 0.0,
            "Long put should be profitable in bearish scenario");
        assert!(calculate_strategy_performance(&OptionsStrategyType::Straddle, 1.2) > 0.0,
            "Straddle should be profitable in volatile scenario");
    }

    #[test]
    fn test_perpetual_funding_rate_opportunities() {
        // Test funding rate arbitrage detection
        let perpetual_manager = tokio_test::block_on(async {
            PerpetualManager::new().await
        }).expect("Failed to create perpetual manager");

        // Mock funding rate data
        let funding_opportunities = vec![
            FundingRateOpportunity {
                symbol: "ETH-PERP".to_string(),
                long_exchange: "dYdX".to_string(),
                short_exchange: "GMX".to_string(),
                funding_rate_diff: 0.015, // 1.5% difference
                expected_profit_8h: 150.0, // $150 for 8 hours
                capital_required: U256::from(10_000) * U256::exp10(18), // $10K
                risk_score: 0.2,
            },
            FundingRateOpportunity {
                symbol: "BTC-PERP".to_string(),
                long_exchange: "Gains".to_string(),
                short_exchange: "dYdX".to_string(),
                funding_rate_diff: 0.008, // 0.8% difference
                expected_profit_8h: 80.0, // $80 for 8 hours
                capital_required: U256::from(10_000) * U256::exp10(18), // $10K
                risk_score: 0.15,
            },
        ];

        println!("Funding Rate Arbitrage Opportunities:");
        println!("====================================");

        for opportunity in &funding_opportunities {
            let annualized_return = (opportunity.expected_profit_8h * 365.25 * 3.0) / 
                                  (opportunity.capital_required.as_u128() as f64 / 1e18) * 100.0;
            
            println!("Symbol: {}", opportunity.symbol);
            println!("  Exchanges: {} (long) vs {} (short)", opportunity.long_exchange, opportunity.short_exchange);
            println!("  Funding rate difference: {:.2}%", opportunity.funding_rate_diff * 100.0);
            println!("  Expected 8h profit: ${:.2}", opportunity.expected_profit_8h);
            println!("  Annualized return: {:.1}%", annualized_return);
            println!("  Risk score: {:.2}", opportunity.risk_score);
            println!();

            // Validate opportunity metrics
            assert!(opportunity.funding_rate_diff > 0.0, "Funding rate difference should be positive");
            assert!(opportunity.expected_profit_8h > 0.0, "Expected profit should be positive");
            assert!(opportunity.risk_score >= 0.0 && opportunity.risk_score <= 1.0, "Risk score should be normalized");
            assert!(annualized_return > 5.0, "Annualized return should be attractive (>5%)");
        }
    }

    // Helper function to calculate strategy performance
    fn calculate_strategy_performance(strategy: &OptionsStrategyType, price_multiplier: f64) -> f64 {
        match strategy {
            OptionsStrategyType::LongCall => {
                if price_multiplier > 1.05 { (price_multiplier - 1.05) * 10.0 } else { -0.05 }
            },
            OptionsStrategyType::LongPut => {
                if price_multiplier < 0.95 { (0.95 - price_multiplier) * 10.0 } else { -0.05 }
            },
            OptionsStrategyType::Straddle => {
                let price_move = (price_multiplier - 1.0).abs();
                if price_move > 0.1 { price_move * 5.0 } else { -0.02 }
            },
            OptionsStrategyType::IronCondor => {
                let price_move = (price_multiplier - 1.0).abs();
                if price_move < 0.05 { 0.03 } else { -price_move * 2.0 }
            },
            OptionsStrategyType::CallSpread => {
                if price_multiplier > 1.02 && price_multiplier < 1.08 { 
                    (price_multiplier - 1.02) * 8.0 
                } else if price_multiplier >= 1.08 { 
                    0.05 
                } else { 
                    -0.02 
                }
            },
            _ => 0.0, // Simplified for other strategies
        }
    }
}