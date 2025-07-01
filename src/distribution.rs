use anyhow::Result;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use tracing::{info, warn, debug};

use crate::config::DeploymentConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionPlan {
    pub total_pairs: usize,
    pub total_instances: usize,
    pub pairs_per_instance: usize,
    pub distribution: HashMap<String, Vec<String>>, // instance_id -> pairs
    pub efficiency: f64, // 0.0 to 1.0
    pub load_balance_score: f64, // 0.0 to 1.0 (1.0 = perfectly balanced)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalanceMetrics {
    pub min_pairs_per_instance: usize,
    pub max_pairs_per_instance: usize,
    pub avg_pairs_per_instance: f64,
    pub standard_deviation: f64,
    pub efficiency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalanceOperation {
    pub from_instance: String,
    pub to_instance: String,
    pub pairs_to_move: Vec<String>,
    pub reason: String,
    pub priority: RebalancePriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RebalancePriority {
    Critical,  // Instance failure
    High,      // Severe imbalance
    Medium,    // Normal rebalancing
    Low,       // Optimization
}

#[derive(Clone)]
pub struct PairDistributor {
    config: DeploymentConfig,
}

impl PairDistributor {
    pub async fn new(config: DeploymentConfig) -> Result<Self> {
        Ok(Self { config })
    }

    pub async fn distribute_pairs(
        &self,
        pairs: Vec<String>,
        target_instances: usize,
    ) -> Result<HashMap<String, Vec<String>>> {
        info!("Distributing {} pairs across {} instances", pairs.len(), target_instances);

        let plan = self.create_distribution_plan(pairs, target_instances).await?;
        
        info!("Distribution plan created: efficiency={:.2}%, balance_score={:.2}%", 
              plan.efficiency * 100.0, plan.load_balance_score * 100.0);

        Ok(plan.distribution)
    }

    async fn create_distribution_plan(
        &self,
        pairs: Vec<String>,
        target_instances: usize,
    ) -> Result<DistributionPlan> {
        let total_pairs = pairs.len();
        let pairs_per_instance = if total_pairs > 0 {
            (total_pairs as f64 / target_instances as f64).ceil() as usize
        } else {
            0
        };

        // Group related pairs for cache efficiency
        let grouped_pairs = self.group_related_pairs(pairs);
        
        // Distribute groups across instances
        let distribution = self.distribute_groups(grouped_pairs, target_instances).await?;
        
        // Calculate metrics
        let metrics = self.calculate_load_metrics(&distribution);
        
        let plan = DistributionPlan {
            total_pairs,
            total_instances: target_instances,
            pairs_per_instance,
            distribution,
            efficiency: metrics.efficiency,
            load_balance_score: self.calculate_balance_score(&metrics),
        };

        Ok(plan)
    }

    fn group_related_pairs(&self, pairs: Vec<String>) -> Vec<Vec<String>> {
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        
        // Group by base asset for cache efficiency
        for pair in pairs {
            if let Some(base_asset) = self.extract_base_asset(&pair) {
                groups.entry(base_asset).or_default().push(pair);
            } else {
                // If we can't extract base asset, create single-pair group
                groups.entry(pair.clone()).or_default().push(pair);
            }
        }

        // Convert to vector of groups, sort by group size descending
        let mut grouped_pairs: Vec<Vec<String>> = groups.into_values().collect();
        grouped_pairs.sort_by(|a, b| b.len().cmp(&a.len()));
        
        debug!("Created {} groups from pairs", grouped_pairs.len());
        grouped_pairs
    }

    fn extract_base_asset(&self, pair: &str) -> Option<String> {
        // Extract base asset from symbol like "BTC/USDT" -> "BTC"
        if let Some(pos) = pair.find('/') {
            Some(pair[..pos].to_string())
        } else {
            None
        }
    }

    async fn distribute_groups(
        &self,
        groups: Vec<Vec<String>>,
        target_instances: usize,
    ) -> Result<HashMap<String, Vec<String>>> {
        let mut distribution: HashMap<String, Vec<String>> = HashMap::new();
        let mut instance_loads: Vec<usize> = vec![0; target_instances];

        // Initialize instances
        for i in 0..target_instances {
            let instance_id = format!("instance-{:03}", i + 1);
            distribution.insert(instance_id, Vec::new());
        }

        // Distribute groups using a greedy algorithm (largest groups first, lightest instance first)
        for group in groups {
            // Find the instance with the lightest load
            let lightest_instance_idx = instance_loads
                .iter()
                .enumerate()
                .min_by_key(|(_, &load)| load)
                .map(|(idx, _)| idx)
                .unwrap_or(0);

            let instance_id = format!("instance-{:03}", lightest_instance_idx + 1);
            
            // Add group to this instance
            if let Some(instance_pairs) = distribution.get_mut(&instance_id) {
                instance_pairs.extend(group.clone());
                instance_loads[lightest_instance_idx] += group.len();
            }

            debug!("Assigned group of {} pairs to {}", group.len(), instance_id);
        }

        // Apply fine-tuning to balance loads
        self.fine_tune_distribution(&mut distribution, &mut instance_loads).await;

        Ok(distribution)
    }

    async fn fine_tune_distribution(
        &self,
        distribution: &mut HashMap<String, Vec<String>>,
        instance_loads: &mut [usize],
    ) {
        let target_load = self.config.pairs_per_instance;
        let tolerance = (target_load as f64 * 0.1) as usize; // 10% tolerance

        // Find overloaded and underloaded instances
        let mut overloaded: Vec<(String, usize)> = Vec::new();
        let mut underloaded: Vec<(String, usize)> = Vec::new();

        for (instance_id, pairs) in distribution.iter() {
            let load = pairs.len();
            if load > target_load + tolerance {
                overloaded.push((instance_id.clone(), load));
            } else if load < target_load.saturating_sub(tolerance) {
                underloaded.push((instance_id.clone(), load));
            }
        }

        // Sort by severity
        overloaded.sort_by(|a, b| b.1.cmp(&a.1)); // Most overloaded first
        underloaded.sort_by(|a, b| a.1.cmp(&b.1)); // Least loaded first

        // Move pairs from overloaded to underloaded instances
        for (over_instance, _) in overloaded {
            for (under_instance, _) in &underloaded {
                // First get the lengths to avoid borrowing conflicts
                let over_load = distribution.get(&over_instance).map(|pairs| pairs.len()).unwrap_or(0);
                let under_load = distribution.get(under_instance).map(|pairs| pairs.len()).unwrap_or(0);
                
                if over_load > 0 && under_load >= 0 {
                    
                    if over_load > target_load + tolerance && under_load < target_load - tolerance {
                        // Calculate how many pairs to move
                        let pairs_to_move = std::cmp::min(
                            over_load - target_load,
                            target_load - under_load,
                        );

                        if pairs_to_move > 0 {
                            // Move pairs (take from the end to avoid reshuffling base asset groups)
                            if let Some(over_pairs) = distribution.get_mut(&over_instance) {
                                let moved_pairs: Vec<String> = over_pairs
                                    .drain(over_pairs.len().saturating_sub(pairs_to_move)..)
                                    .collect();

                            if let Some(under_pairs) = distribution.get_mut(under_instance) {
                                under_pairs.extend(moved_pairs);
                                debug!("Moved {} pairs from {} to {}", 
                                      pairs_to_move, over_instance, under_instance);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn calculate_load_metrics(&self, distribution: &HashMap<String, Vec<String>>) -> LoadBalanceMetrics {
        let loads: Vec<usize> = distribution.values().map(|pairs| pairs.len()).collect();
        
        if loads.is_empty() {
            return LoadBalanceMetrics {
                min_pairs_per_instance: 0,
                max_pairs_per_instance: 0,
                avg_pairs_per_instance: 0.0,
                standard_deviation: 0.0,
                efficiency: 0.0,
            };
        }

        let min_load = *loads.iter().min().unwrap();
        let max_load = *loads.iter().max().unwrap();
        let avg_load = loads.iter().sum::<usize>() as f64 / loads.len() as f64;
        
        // Calculate standard deviation
        let variance = loads
            .iter()
            .map(|&load| {
                let diff = load as f64 - avg_load;
                diff * diff
            })
            .sum::<f64>() / loads.len() as f64;
        let std_dev = variance.sqrt();

        // Calculate efficiency (utilization relative to target)
        let target = self.config.pairs_per_instance as f64;
        let efficiency = if target > 0.0 {
            (avg_load / target).min(1.0)
        } else {
            1.0
        };

        LoadBalanceMetrics {
            min_pairs_per_instance: min_load,
            max_pairs_per_instance: max_load,
            avg_pairs_per_instance: avg_load,
            standard_deviation: std_dev,
            efficiency,
        }
    }

    fn calculate_balance_score(&self, metrics: &LoadBalanceMetrics) -> f64 {
        // Perfect balance = 1.0, poor balance = 0.0
        if metrics.avg_pairs_per_instance == 0.0 {
            return 1.0;
        }

        // Use coefficient of variation (std_dev / mean) to measure balance
        let cv = metrics.standard_deviation / metrics.avg_pairs_per_instance;
        
        // Convert to a 0-1 score where lower CV = higher score
        // CV of 0 = perfect balance (score 1.0)
        // CV of 0.2 = reasonable balance (score 0.8)
        // CV of 0.5+ = poor balance (score approaches 0)
        (1.0 - (cv * 2.0)).max(0.0)
    }

    pub async fn calculate_rebalance_operations(
        &self,
        current_distribution: &HashMap<String, Vec<String>>,
        target_instances: usize,
    ) -> Result<Vec<RebalanceOperation>> {
        let metrics = self.calculate_load_metrics(current_distribution);
        let target_load = self.config.pairs_per_instance;
        let mut operations = Vec::new();

        // Find instances that need rebalancing
        for (instance_id, pairs) in current_distribution {
            let load = pairs.len();
            
            if load > target_load + (target_load / 10) { // 10% tolerance
                let excess = load - target_load;
                
                // Find underloaded instances to move pairs to
                for (other_instance, other_pairs) in current_distribution {
                    if other_instance != instance_id && other_pairs.len() < target_load - (target_load / 10) {
                        let capacity = target_load - other_pairs.len();
                        let pairs_to_move = std::cmp::min(excess, capacity);
                        
                        if pairs_to_move > 0 {
                            let operation = RebalanceOperation {
                                from_instance: instance_id.clone(),
                                to_instance: other_instance.clone(),
                                pairs_to_move: pairs[pairs.len().saturating_sub(pairs_to_move)..]
                                    .to_vec(),
                                reason: format!("Load balancing: {} -> {}", load, other_pairs.len()),
                                priority: if load > target_load * 2 {
                                    RebalancePriority::High
                                } else {
                                    RebalancePriority::Medium
                                },
                            };
                            operations.push(operation);
                        }
                    }
                }
            }
        }

        info!("Generated {} rebalance operations", operations.len());
        Ok(operations)
    }

    pub async fn handle_instance_failure(
        &self,
        failed_instance: &str,
        current_distribution: &HashMap<String, Vec<String>>,
    ) -> Result<Vec<RebalanceOperation>> {
        info!("Handling failure of instance: {}", failed_instance);

        let failed_pairs = current_distribution.get(failed_instance)
            .map(|pairs| pairs.clone())
            .unwrap_or_default();

        if failed_pairs.is_empty() {
            return Ok(Vec::new());
        }

        warn!("Redistributing {} pairs from failed instance {}", 
              failed_pairs.len(), failed_instance);

        let mut operations = Vec::new();
        let target_load = self.config.pairs_per_instance;

        // Find healthy instances with capacity
        let mut available_instances: Vec<(String, usize)> = current_distribution
            .iter()
            .filter(|(id, _)| *id != failed_instance)
            .map(|(id, pairs)| (id.clone(), pairs.len()))
            .filter(|(_, load)| *load < target_load)
            .collect();

        // Sort by current load (lightest first)
        available_instances.sort_by_key(|(_, load)| *load);

        // Distribute failed pairs among healthy instances
        let mut remaining_pairs = failed_pairs;
        
        for (instance_id, current_load) in available_instances {
            if remaining_pairs.is_empty() {
                break;
            }

            let capacity = target_load.saturating_sub(current_load);
            if capacity > 0 {
                let pairs_to_assign = std::cmp::min(capacity, remaining_pairs.len());
                let assigned_pairs: Vec<String> = remaining_pairs
                    .drain(..pairs_to_assign)
                    .collect();

                let operation = RebalanceOperation {
                    from_instance: failed_instance.to_string(),
                    to_instance: instance_id,
                    pairs_to_move: assigned_pairs,
                    reason: format!("Instance failure recovery for {}", failed_instance),
                    priority: RebalancePriority::Critical,
                };

                operations.push(operation);
            }
        }

        // If there are still remaining pairs, we need to overload some instances temporarily
        if !remaining_pairs.is_empty() {
            warn!("Still have {} pairs to redistribute, overloading instances", 
                  remaining_pairs.len());

            // Distribute remaining pairs evenly among all healthy instances
            let healthy_instances: Vec<String> = current_distribution
                .keys()
                .filter(|&id| id != failed_instance)
                .cloned()
                .collect();

            let pairs_per_instance = (remaining_pairs.len() as f64 / healthy_instances.len() as f64).ceil() as usize;

            for instance_id in healthy_instances {
                if remaining_pairs.is_empty() {
                    break;
                }

                let pairs_to_assign = std::cmp::min(pairs_per_instance, remaining_pairs.len());
                let assigned_pairs: Vec<String> = remaining_pairs
                    .drain(..pairs_to_assign)
                    .collect();

                let operation = RebalanceOperation {
                    from_instance: failed_instance.to_string(),
                    to_instance: instance_id,
                    pairs_to_move: assigned_pairs,
                    reason: format!("Emergency redistribution from {}", failed_instance),
                    priority: RebalancePriority::Critical,
                };

                operations.push(operation);
            }
        }

        info!("Created {} operations to handle instance failure", operations.len());
        Ok(operations)
    }

    pub async fn optimize_distribution(
        &self,
        current_distribution: &HashMap<String, Vec<String>>,
    ) -> Result<Vec<RebalanceOperation>> {
        info!("Optimizing distribution for better performance");

        let mut operations = Vec::new();
        let target_load = self.config.pairs_per_instance;

        // Group pairs by base asset to improve cache locality
        let mut base_asset_groups: HashMap<String, Vec<(String, String)>> = HashMap::new(); // base -> [(pair, instance)]

        for (instance_id, pairs) in current_distribution {
            for pair in pairs {
                if let Some(base_asset) = self.extract_base_asset(pair) {
                    base_asset_groups
                        .entry(base_asset)
                        .or_default()
                        .push((pair.clone(), instance_id.clone()));
                }
            }
        }

        // Find groups that are spread across multiple instances
        for (base_asset, pairs_and_instances) in base_asset_groups {
            let mut instances_for_base: HashMap<String, Vec<String>> = HashMap::new();
            
            for (pair, instance) in pairs_and_instances {
                instances_for_base.entry(instance).or_default().push(pair);
            }

            // If this base asset is spread across multiple instances, consolidate it
            if instances_for_base.len() > 1 {
                // Find the instance with the most pairs for this base asset
                let primary_instance = instances_for_base
                    .iter()
                    .max_by_key(|(_, pairs)| pairs.len())
                    .map(|(instance, _)| instance.clone());

                if let Some(primary_instance) = primary_instance {
                    // Check if primary instance has capacity
                    let primary_load = current_distribution.get(&primary_instance)
                        .map(|pairs| pairs.len())
                        .unwrap_or(0);

                    if primary_load < target_load {
                        // Move pairs from other instances to primary instance
                        for (instance_id, pairs) in instances_for_base {
                            if instance_id != primary_instance && !pairs.is_empty() {
                                let capacity = target_load - primary_load;
                                let pairs_to_move = std::cmp::min(pairs.len(), capacity);
                                
                                if pairs_to_move > 0 {
                                    let operation = RebalanceOperation {
                                        from_instance: instance_id,
                                        to_instance: primary_instance.clone(),
                                        pairs_to_move: pairs[..pairs_to_move].to_vec(),
                                        reason: format!("Consolidating {} base asset for cache efficiency", base_asset),
                                        priority: RebalancePriority::Low,
                                    };
                                    operations.push(operation);
                                }
                            }
                        }
                    }
                }
            }
        }

        info!("Generated {} optimization operations", operations.len());
        Ok(operations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DeploymentConfig;

    #[tokio::test]
    async fn test_pair_distribution() {
        let config = DeploymentConfig {
            region: "us-east-1".to_string(),
            instance_type: "t4g.nano".to_string(),
            max_instances: 10,
            min_instances: 2,
            pairs_per_instance: 5,
            auto_scaling: true,
            health_check_interval: 30,
        };

        let distributor = PairDistributor::new(config).await.unwrap();
        
        let pairs = vec![
            "BTC/USDT".to_string(),
            "BTC/BUSD".to_string(),
            "ETH/USDT".to_string(),
            "ETH/BTC".to_string(),
            "ADA/USDT".to_string(),
            "SOL/USDT".to_string(),
            "DOT/USDT".to_string(),
            "AVAX/USDT".to_string(),
            "MATIC/USDT".to_string(),
            "LINK/USDT".to_string(),
        ];

        let distribution = distributor.distribute_pairs(pairs, 3).await.unwrap();
        
        // Should have 3 instances
        assert_eq!(distribution.len(), 3);
        
        // All pairs should be distributed
        let total_distributed: usize = distribution.values().map(|pairs| pairs.len()).sum();
        assert_eq!(total_distributed, 10);
        
        // No instance should be empty
        assert!(distribution.values().all(|pairs| !pairs.is_empty()));
    }

    #[tokio::test]
    async fn test_group_related_pairs() {
        let config = DeploymentConfig {
            region: "us-east-1".to_string(),
            instance_type: "t4g.nano".to_string(),
            max_instances: 10,
            min_instances: 2,
            pairs_per_instance: 5,
            auto_scaling: true,
            health_check_interval: 30,
        };

        let distributor = PairDistributor::new(config).await.unwrap();
        
        let pairs = vec![
            "BTC/USDT".to_string(),
            "BTC/BUSD".to_string(),
            "BTC/EUR".to_string(),
            "ETH/USDT".to_string(),
            "ETH/BTC".to_string(),
            "ADA/USDT".to_string(),
        ];

        let groups = distributor.group_related_pairs(pairs);
        
        // Should group BTC pairs together, ETH pairs together, etc.
        assert!(groups.len() <= 3); // BTC, ETH, ADA groups
        
        // BTC group should be largest (3 pairs)
        let btc_group_size = groups.iter()
            .map(|group| group.iter().filter(|pair| pair.starts_with("BTC")).count())
            .max()
            .unwrap_or(0);
        assert_eq!(btc_group_size, 3);
    }

    #[tokio::test]
    async fn test_load_balance_metrics() {
        let config = DeploymentConfig {
            region: "us-east-1".to_string(),
            instance_type: "t4g.nano".to_string(),
            max_instances: 10,
            min_instances: 2,
            pairs_per_instance: 5,
            auto_scaling: true,
            health_check_interval: 30,
        };

        let distributor = PairDistributor::new(config).await.unwrap();
        
        let mut distribution = HashMap::new();
        distribution.insert("instance-1".to_string(), vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()]);
        distribution.insert("instance-2".to_string(), vec!["ADA/USDT".to_string(), "SOL/USDT".to_string(), "DOT/USDT".to_string()]);
        distribution.insert("instance-3".to_string(), vec!["AVAX/USDT".to_string()]);

        let metrics = distributor.calculate_load_metrics(&distribution);
        
        assert_eq!(metrics.min_pairs_per_instance, 1);
        assert_eq!(metrics.max_pairs_per_instance, 3);
        assert!((metrics.avg_pairs_per_instance - 2.0).abs() < 0.1);
        assert!(metrics.standard_deviation > 0.0);
        assert!(metrics.efficiency > 0.0);
    }

    #[tokio::test]
    async fn test_instance_failure_handling() {
        let config = DeploymentConfig {
            region: "us-east-1".to_string(),
            instance_type: "t4g.nano".to_string(),
            max_instances: 10,
            min_instances: 2,
            pairs_per_instance: 5,
            auto_scaling: true,
            health_check_interval: 30,
        };

        let distributor = PairDistributor::new(config).await.unwrap();
        
        let mut distribution = HashMap::new();
        distribution.insert("instance-1".to_string(), vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()]);
        distribution.insert("instance-2".to_string(), vec!["ADA/USDT".to_string(), "SOL/USDT".to_string()]);
        distribution.insert("instance-3".to_string(), vec!["DOT/USDT".to_string(), "AVAX/USDT".to_string(), "MATIC/USDT".to_string()]);

        let operations = distributor.handle_instance_failure("instance-3", &distribution).await.unwrap();
        
        // Should create operations to redistribute the 3 pairs from instance-3
        assert!(!operations.is_empty());
        assert!(operations.iter().all(|op| op.from_instance == "instance-3"));
        assert!(operations.iter().all(|op| matches!(op.priority, RebalancePriority::Critical)));
        
        let total_redistributed: usize = operations.iter().map(|op| op.pairs_to_move.len()).sum();
        assert_eq!(total_redistributed, 3);
    }
}