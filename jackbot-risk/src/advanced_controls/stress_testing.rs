use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::collections::{HashMap, VecDeque};
use tokio::time::Duration;

use super::config::StressTestScenario;

/// Stress testing engine
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are part of the stress testing system architecture
pub struct StressTestingEngine {
    /// Stress test scenarios
    scenarios: Vec<StressTestScenario>,
    /// Test scheduler
    test_scheduler: StressTestScheduler,
    /// Test results
    test_results: StressTestResults,
    /// Test configuration
    test_config: StressTestConfiguration,
}

#[derive(Debug, Clone)]
pub struct StressTestScheduler {
    /// Next test time
    pub next_test: DateTime<Utc>,
    /// Test frequency
    pub frequency: Duration,
    /// Scenario rotation
    pub scenario_rotation: Vec<String>,
    /// Current scenario index
    pub current_scenario_index: usize,
}

#[derive(Debug, Clone)]
pub struct StressTestResults {
    /// Results by scenario
    pub scenario_results: HashMap<String, ScenarioResult>,
    /// Overall stress test summary
    pub summary: StressTestSummary,
    /// Historical results
    pub historical_results: VecDeque<(DateTime<Utc>, StressTestSummary)>,
}

#[derive(Debug, Clone)]
pub struct ScenarioResult {
    /// Scenario name
    pub scenario_name: String,
    /// Portfolio P&L under stress
    pub stressed_pnl: Decimal,
    /// VaR under stress
    pub stressed_var: Decimal,
    /// Liquidity impact
    pub liquidity_impact: f64,
    /// Time to recovery
    pub recovery_time: Duration,
    /// Pass/fail status
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct StressTestSummary {
    /// Overall pass rate
    pub pass_rate: f64,
    /// Worst-case scenario
    pub worst_case_loss: Decimal,
    /// Average stressed VaR
    pub average_stressed_var: Decimal,
    /// Risk capacity utilization
    pub risk_capacity_utilization: f64,
    /// Recommendations
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StressTestConfiguration {
    /// Test parameters
    pub test_parameters: TestParameters,
    /// Scenario weights
    pub scenario_weights: HashMap<String, f64>,
    /// Confidence levels
    pub confidence_levels: Vec<f64>,
    /// Time horizons
    pub time_horizons: Vec<Duration>,
}

#[derive(Debug, Clone)]
pub struct TestParameters {
    /// Monte Carlo iterations
    pub monte_carlo_iterations: u32,
    /// Simulation time step
    pub time_step: Duration,
    /// Correlation shock magnitude
    pub correlation_shock: f64,
    /// Volatility shock magnitude
    pub volatility_shock: f64,
}

impl StressTestingEngine {
    pub fn new(scenarios: Vec<StressTestScenario>) -> Self {
        Self {
            scenarios,
            test_scheduler: StressTestScheduler::new(),
            test_results: StressTestResults::new(),
            test_config: StressTestConfiguration::default(),
        }
    }

    pub fn run_stress_test(&mut self, portfolio_data: &PortfolioData) -> StressTestSummary {
        let mut results = HashMap::new();
        
        for scenario in &self.scenarios {
            let result = self.run_scenario(scenario, portfolio_data);
            results.insert(scenario.to_string(), result);
        }

        self.test_results.scenario_results = results;
        let summary = self.calculate_summary();
        self.test_results.summary = summary.clone();
        
        // Store historical results
        self.test_results.historical_results.push_back((Utc::now(), summary.clone()));
        if self.test_results.historical_results.len() > 100 {
            self.test_results.historical_results.pop_front();
        }
        
        summary
    }

    fn run_scenario(&self, scenario: &StressTestScenario, portfolio_data: &PortfolioData) -> ScenarioResult {
        match scenario {
            StressTestScenario::MarketCrash { magnitude } => {
                self.simulate_market_crash(*magnitude, portfolio_data)
            }
            StressTestScenario::VolatilitySpike { multiplier } => {
                self.simulate_volatility_spike(*multiplier, portfolio_data)
            }
            StressTestScenario::LiquidityDrying { reduction } => {
                self.simulate_liquidity_drying(*reduction, portfolio_data)
            }
            StressTestScenario::CorrelationBreakdown => {
                self.simulate_correlation_breakdown(portfolio_data)
            }
            StressTestScenario::ExchangeOutage { exchanges } => {
                self.simulate_exchange_outage(exchanges, portfolio_data)
            }
            StressTestScenario::FlashCrash { duration_seconds } => {
                self.simulate_flash_crash(*duration_seconds, portfolio_data)
            }
        }
    }

    fn simulate_market_crash(&self, magnitude: f64, portfolio_data: &PortfolioData) -> ScenarioResult {
        let stressed_pnl = portfolio_data.total_value * Decimal::from_f64_retain(-magnitude).unwrap_or(Decimal::ZERO);
        let stressed_var = portfolio_data.current_var * Decimal::from_f64_retain(1.5).unwrap_or(Decimal::ONE);
        
        ScenarioResult {
            scenario_name: format!("Market Crash ({}%)", magnitude * 100.0),
            stressed_pnl,
            stressed_var,
            liquidity_impact: magnitude * 2.0,
            recovery_time: Duration::from_secs((magnitude * 86400.0) as u64),
            passed: stressed_pnl.abs() < portfolio_data.risk_limit,
        }
    }

    fn simulate_volatility_spike(&self, multiplier: f64, portfolio_data: &PortfolioData) -> ScenarioResult {
        let stressed_var = portfolio_data.current_var * Decimal::from_f64_retain(multiplier).unwrap_or(Decimal::ONE);
        let stressed_pnl = portfolio_data.total_value * Decimal::from_f64_retain(-0.1 * multiplier).unwrap_or(Decimal::ZERO);
        
        ScenarioResult {
            scenario_name: format!("Volatility Spike ({}x)", multiplier),
            stressed_pnl,
            stressed_var,
            liquidity_impact: multiplier * 0.5,
            recovery_time: Duration::from_secs(3600 * multiplier as u64),
            passed: stressed_var < portfolio_data.var_limit,
        }
    }

    fn simulate_liquidity_drying(&self, reduction: f64, portfolio_data: &PortfolioData) -> ScenarioResult {
        let liquidity_cost = portfolio_data.total_value * Decimal::from_f64_retain(reduction * 0.05).unwrap_or(Decimal::ZERO);
        let stressed_pnl = -liquidity_cost;
        
        ScenarioResult {
            scenario_name: format!("Liquidity Drying ({}% reduction)", reduction * 100.0),
            stressed_pnl,
            stressed_var: portfolio_data.current_var,
            liquidity_impact: reduction,
            recovery_time: Duration::from_secs((reduction * 28800.0) as u64),
            passed: liquidity_cost < portfolio_data.total_value * Decimal::from_str_exact("0.1").unwrap(),
        }
    }

    fn simulate_correlation_breakdown(&self, portfolio_data: &PortfolioData) -> ScenarioResult {
        let stressed_var = portfolio_data.current_var * Decimal::from_f64_retain(1.8).unwrap_or(Decimal::ONE);
        let stressed_pnl = portfolio_data.total_value * Decimal::from_str_exact("-0.15").unwrap();
        
        ScenarioResult {
            scenario_name: "Correlation Breakdown".to_string(),
            stressed_pnl,
            stressed_var,
            liquidity_impact: 0.3,
            recovery_time: Duration::from_secs(7200),
            passed: stressed_var < portfolio_data.var_limit * Decimal::from_f64_retain(1.5).unwrap_or(Decimal::ONE),
        }
    }

    fn simulate_exchange_outage(&self, exchanges: &[jackbot_instrument::exchange::ExchangeId], portfolio_data: &PortfolioData) -> ScenarioResult {
        let affected_exposure = Decimal::from_f64_retain(exchanges.len() as f64 * 0.2).unwrap_or(Decimal::ZERO);
        let stressed_pnl = portfolio_data.total_value * affected_exposure * Decimal::from_str_exact("-0.05").unwrap();
        
        ScenarioResult {
            scenario_name: format!("Exchange Outage ({} exchanges)", exchanges.len()),
            stressed_pnl,
            stressed_var: portfolio_data.current_var,
            liquidity_impact: affected_exposure.to_f64().unwrap_or(0.0),
            recovery_time: Duration::from_secs(14400),
            passed: true, // Exchange outages are typically recoverable
        }
    }

    fn simulate_flash_crash(&self, duration_seconds: u64, portfolio_data: &PortfolioData) -> ScenarioResult {
        let crash_magnitude = (duration_seconds as f64 / 300.0).min(0.5);
        let stressed_pnl = portfolio_data.total_value * Decimal::from_f64_retain(-crash_magnitude).unwrap_or(Decimal::ZERO);
        
        ScenarioResult {
            scenario_name: format!("Flash Crash ({} seconds)", duration_seconds),
            stressed_pnl,
            stressed_var: portfolio_data.current_var * Decimal::from_f64_retain(2.0).unwrap_or(Decimal::ONE),
            liquidity_impact: crash_magnitude * 3.0,
            recovery_time: Duration::from_secs(duration_seconds * 10),
            passed: duration_seconds < 60, // Only very short flash crashes are acceptable
        }
    }

    fn calculate_summary(&self) -> StressTestSummary {
        let total_scenarios = self.test_results.scenario_results.len() as f64;
        let passed_scenarios = self.test_results.scenario_results.values()
            .filter(|r| r.passed)
            .count() as f64;
        
        let pass_rate = if total_scenarios > 0.0 {
            passed_scenarios / total_scenarios
        } else {
            0.0
        };
        
        let worst_case_loss = self.test_results.scenario_results.values()
            .map(|r| r.stressed_pnl)
            .min()
            .unwrap_or(Decimal::ZERO);
            
        let average_stressed_var = if !self.test_results.scenario_results.is_empty() {
            let sum: Decimal = self.test_results.scenario_results.values()
                .map(|r| r.stressed_var)
                .sum();
            sum / Decimal::from(self.test_results.scenario_results.len())
        } else {
            Decimal::ZERO
        };
        
        let mut recommendations = Vec::new();
        
        if pass_rate < 0.8 {
            recommendations.push("Consider reducing position sizes to improve stress test pass rate".to_string());
        }
        
        if worst_case_loss.abs() > Decimal::from(100000) {
            recommendations.push("Worst-case losses exceed acceptable limits - implement additional risk controls".to_string());
        }
        
        StressTestSummary {
            pass_rate,
            worst_case_loss,
            average_stressed_var,
            risk_capacity_utilization: pass_rate,
            recommendations,
        }
    }

    pub fn schedule_next_test(&mut self) {
        self.test_scheduler.next_test = Utc::now() + chrono::Duration::from_std(self.test_scheduler.frequency).unwrap();
        self.test_scheduler.current_scenario_index = 
            (self.test_scheduler.current_scenario_index + 1) % self.test_scheduler.scenario_rotation.len();
    }

    pub fn get_latest_results(&self) -> &StressTestResults {
        &self.test_results
    }
}

impl Default for StressTestScheduler {
    fn default() -> Self {
        Self {
            next_test: Utc::now() + chrono::Duration::hours(1),
            frequency: Duration::from_secs(3600),
            scenario_rotation: vec![
                "MarketCrash".to_string(),
                "VolatilitySpike".to_string(),
                "LiquidityDrying".to_string(),
            ],
            current_scenario_index: 0,
        }
    }
}

impl StressTestScheduler {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for StressTestResults {
    fn default() -> Self {
        Self {
            scenario_results: HashMap::new(),
            summary: StressTestSummary::new(),
            historical_results: VecDeque::with_capacity(100),
        }
    }
}

impl StressTestResults {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for StressTestSummary {
    fn default() -> Self {
        Self {
            pass_rate: 1.0,
            worst_case_loss: Decimal::ZERO,
            average_stressed_var: Decimal::ZERO,
            risk_capacity_utilization: 0.0,
            recommendations: vec![],
        }
    }
}

impl StressTestSummary {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for StressTestConfiguration {
    fn default() -> Self {
        Self {
            test_parameters: TestParameters {
                monte_carlo_iterations: 10000,
                time_step: Duration::from_secs(300),
                correlation_shock: 0.3,
                volatility_shock: 2.0,
            },
            scenario_weights: {
                let mut weights = HashMap::new();
                weights.insert("MarketCrash".to_string(), 0.3);
                weights.insert("VolatilitySpike".to_string(), 0.2);
                weights.insert("LiquidityDrying".to_string(), 0.2);
                weights.insert("CorrelationBreakdown".to_string(), 0.15);
                weights.insert("ExchangeOutage".to_string(), 0.1);
                weights.insert("FlashCrash".to_string(), 0.05);
                weights
            },
            confidence_levels: vec![0.95, 0.99],
            time_horizons: vec![
                Duration::from_secs(3600),    // 1 hour
                Duration::from_secs(86400),   // 1 day
                Duration::from_secs(604800),  // 1 week
            ],
        }
    }
}

/// Portfolio data for stress testing
#[derive(Debug, Clone)]
pub struct PortfolioData {
    pub total_value: Decimal,
    pub current_var: Decimal,
    pub var_limit: Decimal,
    pub risk_limit: Decimal,
}