use jackbot_execution::order::id::StrategyId;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct StrategyRegistry<S> {
    strategies: HashMap<StrategyId, S>,
}

impl<S> StrategyRegistry<S> {
    pub fn new() -> Self {
        Self {
            strategies: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: StrategyId, strategy: S) {
        self.strategies.insert(id, strategy);
    }

    pub fn get(&self, id: &StrategyId) -> Option<&S> {
        self.strategies.get(id)
    }

    pub fn remove(&mut self, id: &StrategyId) -> Option<S> {
        self.strategies.remove(id)
    }
}

#[derive(Debug)]
pub struct DefaultStrategy {
    pub id: StrategyId,
}

impl Default for DefaultStrategy {
    fn default() -> Self {
        Self {
            id: StrategyId::new("default"),
        }
    }
}

#[test]
fn test_register_and_get() {
    let mut reg = StrategyRegistry::new();
    let strat = DefaultStrategy {
        id: StrategyId::new("test-strategy"),
    };
    let id = strat.id.clone();
    reg.register(id.clone(), strat);
    assert!(reg.get(&id).is_some());
    assert!(reg.remove(&id).is_some());
    assert!(reg.get(&id).is_none());
}
