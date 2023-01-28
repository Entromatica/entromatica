use std::fmt::Formatter;
use std::fmt::{Debug, Display};
use std::hash::Hash;

use backtrace::Backtrace as trc;
use hashbrown::HashMap;
use petgraph::graph::NodeIndex;
use petgraph::Graph;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::prelude::*;

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub(self) struct RuleCache {
    condition: HashMap<StateHash, RuleApplies>,
    actions: HashMap<StateHash, StateHash>,
}

impl Display for RuleCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "RuleCache:")?;
        for (base_state_hash, applies) in &self.condition {
            if applies.is_true() {
                match self.condition(base_state_hash) {
                    Ok(new_state_hash) => {
                        writeln!(f, "Rule applies for {base_state_hash} -> {new_state_hash}")?
                    }
                    Err(error) => return std::fmt::Debug::fmt(&error, f),
                };
            } else {
                writeln!(f, "Rule does not apply for {base_state_hash}")?;
            }
        }
        Ok(())
    }
}

impl RuleCache {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            condition: HashMap::new(),
            actions: HashMap::new(),
        }
    }

    pub fn condition(&self, base_state_hash: &StateHash) -> Result<&RuleApplies, RuleCacheError> {
        self.condition
            .get(base_state_hash)
            .ok_or_else(|| RuleCacheError::ConditionNotFound {
                base_state_hash: *base_state_hash,
                context: get_backtrace(),
            })
    }

    pub fn action(&self, base_state_hash: &StateHash) -> Result<&StateHash, RuleCacheError> {
        self.actions
            .get(base_state_hash)
            .ok_or_else(|| RuleCacheError::ActionNotFound {
                base_state_hash: *base_state_hash,
                context: get_backtrace(),
            })
    }

    pub fn add_condition(
        &mut self,
        base_state_hash: StateHash,
        applies: RuleApplies,
    ) -> Result<(), RuleCacheError> {
        if self.condition.contains_key(&base_state_hash) {
            if self.condition.get(&base_state_hash) == Some(&applies) {
                return Ok(());
            } else {
                return Err(RuleCacheError::ConditionAlreadyExists {
                    base_state_hash,
                    applies,
                    context: get_backtrace(),
                });
            }
        }
        self.condition.insert(base_state_hash, applies);
        Ok(())
    }

    pub fn add_action(
        &mut self,
        base_state_hash: StateHash,
        new_state_hash: StateHash,
    ) -> Result<(), RuleCacheError> {
        if self.actions.contains_key(&base_state_hash) {
            if self.actions.get(&base_state_hash) == Some(&new_state_hash) {
                return Ok(());
            } else {
                return Err(RuleCacheError::ActionAlreadyExists {
                    base_state_hash,
                    new_state_hash,
                    context: get_backtrace(),
                });
            }
        }
        self.actions.insert(base_state_hash, new_state_hash);
        Ok(())
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Error)]
pub(self) enum RuleCacheError {
    #[error("Condition already exists: {base_state_hash:#?} -> {applies:#?}")]
    ConditionAlreadyExists {
        base_state_hash: StateHash,
        applies: RuleApplies,
        context: trc,
    },

    #[error("Action already exists: {base_state_hash:#?} -> {new_state_hash:#?}")]
    ActionAlreadyExists {
        base_state_hash: StateHash,
        new_state_hash: StateHash,
        context: trc,
    },

    #[error("Condition not found: {base_state_hash:#?}")]
    ConditionNotFound {
        base_state_hash: StateHash,
        context: trc,
    },

    #[error("Action not found: {base_state_hash:#?}")]
    ActionNotFound {
        base_state_hash: StateHash,
        context: trc,
    },
}

#[derive(Debug, Clone, Error)]
#[error(transparent)]
pub(crate) struct InternalCacheError(#[from] RuleCacheError);

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Cache {
    rules: HashMap<RuleName, RuleCache>,
}

impl Display for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Cache:")?;
        for (rule_name, rule_cache) in &self.rules {
            writeln!(f, "{rule_name}: {rule_cache}")?;
        }
        Ok(())
    }
}

impl Cache {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
        }
    }

    pub(self) fn rule(&self, rule_name: &RuleName) -> Result<&RuleCache, CacheError> {
        self.rules
            .get(rule_name)
            .ok_or_else(|| CacheError::RuleNotFound {
                rule_name: rule_name.clone(),
                context: get_backtrace(),
            })
    }

    pub(self) fn rule_mut(&mut self, rule_name: &RuleName) -> Result<&mut RuleCache, CacheError> {
        self.rules
            .get_mut(rule_name)
            .ok_or_else(|| CacheError::RuleNotFound {
                rule_name: rule_name.clone(),
                context: get_backtrace(),
            })
    }

    pub(self) fn add_rule(&mut self, rule_name: RuleName) -> Result<(), CacheError> {
        if self.rules.contains_key(&rule_name) {
            return Err(CacheError::RuleAlreadyExists {
                rule_name,
                context: get_backtrace(),
            });
        }
        self.rules.insert(rule_name, RuleCache::new());
        Ok(())
    }

    pub fn condition(
        &self,
        rule_name: &RuleName,
        base_state_hash: &StateHash,
    ) -> Result<&RuleApplies, CacheError> {
        Ok(self.rule(rule_name)?.condition(base_state_hash)?)
    }

    pub fn contains_condition(
        &self,
        rule_name: &RuleName,
        base_state_hash: &StateHash,
    ) -> Result<bool, CacheError> {
        match self.rule(rule_name) {
            Ok(rule_cache) => Ok(rule_cache.condition.contains_key(base_state_hash)),
            Err(CacheError::RuleNotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn contains_action(
        &self,
        rule_name: &RuleName,
        base_state_hash: &StateHash,
    ) -> Result<bool, CacheError> {
        match self.rule(rule_name) {
            Ok(rule_cache) => Ok(rule_cache.actions.contains_key(base_state_hash)),
            Err(CacheError::RuleNotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn action(
        &self,
        rule_name: &RuleName,
        base_state_hash: &StateHash,
    ) -> Result<StateHash, CacheError> {
        Ok(*self.rule(rule_name)?.action(base_state_hash)?)
    }

    pub fn add_action(
        &mut self,
        rule_name: RuleName,
        base_state_hash: StateHash,
        new_state_hash: StateHash,
    ) -> Result<(), CacheError> {
        match self.rule_mut(&rule_name) {
            Ok(rule_cache) => Ok(rule_cache.add_action(base_state_hash, new_state_hash)?),
            Err(cache_error) => {
                if let CacheError::RuleNotFound { rule_name, .. } = cache_error {
                    self.add_rule(rule_name.clone())?;
                    let rule_cache = self.rule_mut(&rule_name)?;
                    Ok(rule_cache.add_action(base_state_hash, new_state_hash)?)
                } else {
                    Err(cache_error)
                }
            }
        }
    }

    pub fn add_condition(
        &mut self,
        rule_name: RuleName,
        base_state_hash: StateHash,
        applies: RuleApplies,
    ) -> Result<(), CacheError> {
        match self.rule_mut(&rule_name) {
            Ok(rule_cache) => Ok(rule_cache.add_condition(base_state_hash, applies)?),
            Err(cache_error) => {
                if let CacheError::RuleNotFound { rule_name, .. } = cache_error {
                    self.add_rule(rule_name.clone())?;
                    let rule_cache = self.rule_mut(&rule_name)?;
                    Ok(rule_cache.add_condition(base_state_hash, applies)?)
                } else {
                    Err(cache_error)
                }
            }
        }
    }

    pub fn apply_condition_update(
        &mut self,
        update: ConditionCacheUpdate,
    ) -> Result<(), CacheError> {
        self.add_condition(update.rule_name, update.base_state_hash, update.applies)
    }

    pub fn apply_action_update(&mut self, update: ActionCacheUpdate) -> Result<(), CacheError> {
        self.add_action(
            update.rule_name,
            update.base_state_hash,
            update.new_state_hash,
        )
    }

    pub fn graph<T>(
        &self,
        possible_states: PossibleStates<T>,
    ) -> Result<Graph<StateHash, RuleName>, ErrorKind<T>>
    where
        T: Hash
            + Clone
            + PartialEq
            + Debug
            + Default
            + Serialize
            + Send
            + Sync
            + for<'a> Deserialize<'a>,
    {
        let mut graph = Graph::<StateHash, RuleName>::new();
        let mut nodes: HashMap<StateHash, NodeIndex> = HashMap::new();
        for (state_hash, _) in possible_states.iter() {
            let node_index = graph.add_node(*state_hash);
            nodes.insert(*state_hash, node_index);
        }
        for (base_state_hash, base_state_node) in &nodes {
            for (rule_name, _) in self.rules.iter() {
                if self.condition(rule_name, base_state_hash)?.applies() {
                    let new_state_hash = self.action(rule_name, base_state_hash)?;
                    let new_state_node = nodes.get(&new_state_hash).ok_or_else(|| {
                        ErrorKind::PossibleStatesError(PossibleStatesError::StateNotFound {
                            state_hash: new_state_hash,
                            context: get_backtrace(),
                        })
                    })?;
                    graph.add_edge(*base_state_node, *new_state_node, rule_name.clone());
                }
            }
        }
        Ok(graph)
    }

    pub fn merge(&mut self, cache: &Self) -> Result<(), CacheError> {
        for (rule_name, rule_cache) in cache.rules.iter() {
            for (base_state_hash, applies) in rule_cache.condition.iter() {
                self.add_condition(rule_name.clone(), *base_state_hash, *applies)?;
            }
            for (base_state_hash, new_state_hash) in rule_cache.actions.iter() {
                self.add_action(rule_name.clone(), *base_state_hash, *new_state_hash)?;
            }
        }
        Ok(())
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Error)]
pub(crate) enum CacheError {
    #[error("Rule already exists: {rule_name:#?}")]
    RuleAlreadyExists { rule_name: RuleName, context: trc },

    #[error("Rule not found: {rule_name:#?}")]
    RuleNotFound { rule_name: RuleName, context: trc },

    #[error("Internal cache error: {source:#?}")]
    InternalError {
        #[source]
        source: InternalCacheError,
        context: trc,
    },
}

impl From<RuleCacheError> for CacheError {
    fn from(source: RuleCacheError) -> Self {
        Self::InternalError {
            source: InternalCacheError(source),
            context: get_backtrace(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ConditionCacheUpdate {
    pub(self) rule_name: RuleName,
    pub(self) base_state_hash: StateHash,
    pub(self) applies: RuleApplies,
}

impl Display for ConditionCacheUpdate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ConditionCacheUpdate for base state {}: rule {} applies: {}",
            self.base_state_hash, self.rule_name, self.applies
        )
    }
}

impl ConditionCacheUpdate {
    #[allow(dead_code)]
    pub fn new(rule_name: RuleName, base_state_hash: StateHash, applies: RuleApplies) -> Self {
        Self {
            rule_name,
            base_state_hash,
            applies,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ActionCacheUpdate {
    pub(self) rule_name: RuleName,
    pub(self) base_state_hash: StateHash,
    pub(self) new_state_hash: StateHash,
}

impl Display for ActionCacheUpdate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ActionCacheUpdate for base state {}: rule {} new state: {}",
            self.base_state_hash, self.rule_name, self.new_state_hash
        )
    }
}

impl ActionCacheUpdate {
    pub fn new(rule_name: RuleName, base_state_hash: StateHash, new_state_hash: StateHash) -> Self {
        Self {
            rule_name,
            base_state_hash,
            new_state_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_add_should_work() {
        let mut cache = Cache::new();
        let rule_name = RuleName::new("test");
        let base_state_hash = StateHash::new::<i64>(&State::default());
        let new_state_hash = StateHash::new::<i64>(&State::default());
        let applies = RuleApplies::from(true);
        cache
            .add_condition(rule_name.clone(), base_state_hash, applies)
            .unwrap();
        cache
            .add_action(rule_name.clone(), base_state_hash, new_state_hash)
            .unwrap();
        assert_eq!(
            cache
                .condition(&rule_name, &base_state_hash)
                .cloned()
                .unwrap(),
            applies
        );
        assert_eq!(
            cache.action(&rule_name, &base_state_hash).unwrap(),
            new_state_hash
        );
    }

    #[test]
    fn cache_no_overwriting_values() {
        let mut cache = Cache::new();
        let rule_name = RuleName::new("test");
        let base_state_hash = StateHash::new::<i64>(&State::default());
        let new_state_hash = StateHash::new::<i64>(&State::default());
        let applies = RuleApplies::from(true);
        cache
            .add_condition(rule_name.clone(), base_state_hash, applies)
            .unwrap();
        cache
            .add_action(rule_name.clone(), base_state_hash, new_state_hash)
            .unwrap();
        let new_new_state_hash = StateHash::new(&State::new(vec![(
            EntityName::new("A"),
            Entity::new(vec![(ParameterName::new("Parameter"), Parameter::new(1))]),
        )]));
        let new_applies = RuleApplies::from(false);
        cache
            .add_condition(rule_name.clone(), base_state_hash, new_applies)
            .unwrap_err();
        cache
            .add_action(rule_name.clone(), base_state_hash, new_new_state_hash)
            .unwrap_err();
        assert_eq!(
            cache
                .condition(&rule_name, &base_state_hash)
                .cloned()
                .unwrap(),
            applies
        );
        assert_eq!(
            cache.action(&rule_name, &base_state_hash).unwrap(),
            new_state_hash
        );
    }

    #[test]
    fn cache_apply_updates() {
        let mut cache = Cache::new();
        let rule_name = RuleName::new("test");
        let base_state_hash = StateHash::new::<i64>(&State::default());
        let new_state_hash = StateHash::new::<i64>(&State::default());
        let applies = RuleApplies::from(true);
        let condition_update =
            ConditionCacheUpdate::new(rule_name.clone(), base_state_hash, applies);
        let action_update =
            ActionCacheUpdate::new(rule_name.clone(), base_state_hash, new_state_hash);
        cache.apply_condition_update(condition_update).unwrap();
        cache.apply_action_update(action_update).unwrap();
        assert_eq!(
            cache
                .condition(&rule_name, &base_state_hash)
                .cloned()
                .unwrap(),
            applies
        );
        assert_eq!(
            cache.action(&rule_name, &base_state_hash).unwrap(),
            new_state_hash
        );
    }
}
