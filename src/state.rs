use std::collections::hash_map::DefaultHasher;
use std::fmt::Display;
use std::hash::{Hash, Hasher};

#[allow(unused_imports)]
use hashbrown::{HashMap, HashSet};
#[allow(unused_imports)]
use itertools::Itertools;

use derive_more::*;
use rayon::prelude::*;

use crate::error::{AlreadyExistsError, NotFoundError, OutOfRangeError};
use crate::resource::*;
use crate::rules::*;
use crate::units::*;

/// A single entity in the simulation.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Entity {
    resources: HashMap<ResourceName, Amount>,
}

impl Display for Entity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Entity:")?;
        for (resource_name, amount) in &self.resources {
            writeln!(f, "  {}: {}", resource_name, amount)?;
        }
        Ok(())
    }
}

impl Entity {
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
        }
    }

    pub fn from_resources(resources: Vec<(ResourceName, Amount)>) -> Self {
        Self {
            resources: resources.into_iter().collect(),
        }
    }

    pub fn resource(
        &self,
        resource_name: &ResourceName,
    ) -> Result<&Amount, NotFoundError<ResourceName, Entity>> {
        self.resources
            .get(resource_name)
            .ok_or_else(|| NotFoundError::new(resource_name.clone(), self.clone()))
    }

    pub fn resource_mut(
        &mut self,
        resource_name: &ResourceName,
    ) -> Result<&mut Amount, NotFoundError<ResourceName, Entity>> {
        let err = NotFoundError::new(resource_name.clone(), self.clone());
        self.resources.get_mut(resource_name).ok_or(err)
    }

    pub fn iter_resources(&self) -> impl Iterator<Item = (&ResourceName, &Amount)> {
        self.resources.iter()
    }

    pub fn iter_resources_mut(&mut self) -> impl Iterator<Item = (&ResourceName, &mut Amount)> {
        self.resources.iter_mut()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Display, Default, From, Into, AsRef, AsMut)]
pub struct EntityName(pub String);

impl EntityName {
    pub fn new() -> Self {
        Self("".to_string())
    }
}

/// A possible state in the markov chain of the simulation, which is only dependent on
/// the configuration of the entities in the simulation.
#[derive(Clone, Debug, Default, From, Into)]
pub struct State {
    entities: HashMap<EntityName, Entity>,
}

impl Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "State:")?;
        for (entity_name, entity) in &self.entities {
            writeln!(f, "  {entity_name}:")?;
            for (resource_name, amount) in &entity.resources {
                writeln!(
                    f,
                    "    {resource_name}: {amount}",
                    resource_name = resource_name,
                    amount = amount
                )?;
            }
        }
        Ok(())
    }
}

impl Hash for State {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for (name, entity) in &self.entities {
            for (resource_name, amount) in &entity.resources {
                (name.clone(), resource_name.clone(), *amount).hash(state);
            }
        }
    }
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        let self_hasher = &mut DefaultHasher::new();
        self.hash(self_hasher);
        let other_hasher = &mut DefaultHasher::new();
        other.hash(other_hasher);
        self_hasher.finish() == other_hasher.finish()
    }
}

impl Eq for State {}

impl State {
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
        }
    }

    pub fn from_entities(entities: Vec<(EntityName, Entity)>) -> Self {
        Self {
            entities: entities.into_iter().collect(),
        }
    }

    pub fn entity(
        &self,
        entity_name: &EntityName,
    ) -> Result<&Entity, NotFoundError<EntityName, State>> {
        self.entities
            .get(entity_name)
            .ok_or_else(|| NotFoundError::new(entity_name.clone(), self.clone()))
    }

    pub fn entity_mut(
        &mut self,
        entity_name: &EntityName,
    ) -> Result<&mut Entity, NotFoundError<EntityName, State>> {
        let err = NotFoundError::new(entity_name.clone(), self.clone());
        self.entities.get_mut(entity_name).ok_or(err)
    }

    pub fn iter_entities(&self) -> impl Iterator<Item = (&EntityName, &Entity)> {
        self.entities.iter()
    }

    pub fn iter_entities_mut(&mut self) -> impl Iterator<Item = (&EntityName, &mut Entity)> {
        self.entities.iter_mut()
    }

    // TODO: check for multiple actions applying to one resource
    pub(crate) fn apply_actions(&self, actions: HashMap<ActionName, Action>) -> State {
        let mut new_state = self.clone();
        for (_, action) in actions {
            new_state
                .entities
                .get_mut(action.target())
                .expect("Entity {action.entity} not found in state")
                .resources
                .insert(action.resource().clone(), action.amount());
        }
        new_state
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Display, Default)]
pub struct StateHash(u64);

impl StateHash {
    pub fn new() -> Self {
        Self(Self::from_state(&State::new()).0)
    }

    pub fn from_state(state: &State) -> Self {
        let mut hasher = &mut DefaultHasher::new();
        state.hash(&mut hasher);
        Self(hasher.finish())
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default, From, Into, AsRef, AsMut, Index)]
pub struct PossibleStates(HashMap<StateHash, State>);

impl Display for PossibleStates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (state_hash, state) in &self.0 {
            writeln!(
                f,
                "{state_hash}: {state}",
                state_hash = state_hash,
                state = state
            )?;
        }
        Ok(())
    }
}

impl PossibleStates {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub(crate) fn append_state(
        &mut self,
        state_hash: StateHash,
        state: State,
    ) -> Result<(), AlreadyExistsError<StateHash, State>> {
        if self.state(&state_hash).is_some() {
            return Err(AlreadyExistsError::new(state_hash, state));
        }
        self.0.insert(state_hash, state);
        Ok(())
    }

    pub(crate) fn append_states(
        &mut self,
        states: &PossibleStates,
    ) -> Result<(), AlreadyExistsError<StateHash, State>> {
        for (state_hash, state) in states.iter() {
            self.append_state(*state_hash, state.clone())?;
        }
        Ok(())
    }

    pub fn state(&self, state_hash: &StateHash) -> Option<&State> {
        self.0.get(state_hash)
    }

    pub fn iter(&self) -> hashbrown::hash_map::Iter<StateHash, State> {
        self.0.iter()
    }

    pub fn values(&self) -> hashbrown::hash_map::Values<StateHash, State> {
        self.0.values()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, state_hash: &StateHash) -> bool {
        self.0.contains_key(state_hash)
    }
}

#[derive(Clone, PartialEq, Debug, Default, From, Into, AsRef, AsMut, Index)]
pub struct ReachableStates(HashMap<StateHash, Probability>);

impl Display for ReachableStates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (state_hash, probability) in &self.0 {
            writeln!(
                f,
                "{state_hash}: {probability}",
                state_hash = state_hash,
                probability = probability
            )?;
        }
        Ok(())
    }
}

impl ReachableStates {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn append_state(
        &mut self,
        state_hash: StateHash,
        state_probability: Probability,
    ) -> Result<(), OutOfRangeError<Probability>> {
        match self.0.get_mut(&state_hash) {
            Some(probability) => {
                if *probability + state_probability > Probability::from(1.) {
                    return Err(OutOfRangeError::new(
                        *probability + state_probability,
                        Probability::from(0.),
                        Probability::from(1.),
                    ));
                }
                *probability += state_probability;
            }
            None => {
                self.0.insert(state_hash, state_probability);
            }
        }
        Ok(())
    }

    pub fn append_states(
        &mut self,
        states: &ReachableStates,
    ) -> Result<(), OutOfRangeError<Probability>> {
        for (state_hash, state_probability) in states.iter() {
            self.append_state(*state_hash, *state_probability)?;
        }
        Ok(())
    }

    pub fn values(&self) -> hashbrown::hash_map::Values<StateHash, Probability> {
        self.0.values()
    }

    pub fn iter(&self) -> hashbrown::hash_map::Iter<StateHash, Probability> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> hashbrown::hash_map::IterMut<StateHash, Probability> {
        self.0.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, state_hash: &StateHash) -> bool {
        self.0.contains_key(state_hash)
    }

    pub fn probability_sum(&self) -> Probability {
        Probability::from(
            self.iter()
                .par_bridge()
                .map(|(_, probability)| probability.to_f64())
                .sum::<f64>(),
        )
    }

    /// Gets the entropy of the current probability distribution.
    pub fn entropy(&self) -> Entropy {
        Entropy::from(
            self.0
                .par_iter()
                .map(|(_, probability)| {
                    if *probability > Probability::from(0.) {
                        f64::from(*probability) * -f64::from(*probability).log2()
                    } else {
                        0.
                    }
                })
                .sum::<f64>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_get_resource_should_return_value_on_present_resource() {
        let resources = vec![(ResourceName::from("resource".to_string()), Amount::from(1.))];
        let entity = Entity::from_resources(resources);
        assert_eq!(
            entity
                .resource(&ResourceName::from("resource".to_string()))
                .cloned(),
            Result::Ok(Amount::from(1.))
        );
    }

    #[test]
    fn entity_get_resource_should_return_error_on_missing_resource() {
        let resources = vec![(ResourceName::from("resource".to_string()), Amount::from(1.))];
        let entity = Entity::from_resources(resources);
        assert_eq!(
            entity.resource(&ResourceName::from("missing_resource".to_string())),
            Result::Err(NotFoundError::new(
                ResourceName::from("missing_resource".to_string()),
                entity.clone()
            ))
        );
    }

    #[test]
    fn state_partial_equal_works_as_expected() {
        let state_a_0 = State::from_entities(vec![(
            EntityName::from("A".to_string()),
            Entity::from_resources(vec![(
                ResourceName::from("Resource".to_string()),
                Amount::from(0.),
            )]),
        )]);
        let state_a_1 = State::from_entities(vec![(
            EntityName::from("A".to_string()),
            Entity::from_resources(vec![(
                ResourceName::from("Resource".to_string()),
                Amount::from(0.),
            )]),
        )]);
        let state_b = State::from_entities(vec![(
            EntityName::from("A".to_string()),
            Entity::from_resources(vec![(
                ResourceName::from("Resource".to_string()),
                Amount::from(1.),
            )]),
        )]);
        let state_c = State::from_entities(vec![(
            EntityName::from("B".to_string()),
            Entity::from_resources(vec![(
                ResourceName::from("Resource".to_string()),
                Amount::from(1.),
            )]),
        )]);
        assert_eq!(state_a_0, state_a_1);
        assert_ne!(state_a_0, state_b);
        assert_ne!(state_a_1, state_b);
        assert_ne!(state_b, state_c);
    }

    #[test]
    fn state_get_entity_should_return_value_on_present_entity() {
        let state = State::from_entities(vec![(
            EntityName::from("A".to_string()),
            Entity::from_resources(vec![(
                ResourceName::from("Resource".to_string()),
                Amount::from(0.),
            )]),
        )]);

        assert_eq!(
            state.entity(&EntityName::from("A".to_string()),).cloned(),
            Ok(Entity::from_resources(vec![(
                ResourceName::from("Resource".to_string()),
                Amount::from(0.)
            )]))
        );
    }

    #[test]
    fn state_get_entity_should_return_error_on_missing_entity() {
        let state = State::from_entities(vec![(
            EntityName::from("A".to_string()),
            Entity::from_resources(vec![(
                ResourceName::from("Resource".to_string()),
                Amount::from(0.),
            )]),
        )]);
        assert_eq!(
            state
                .entity(&EntityName::from("missing_entity".to_string()))
                .cloned(),
            Err(NotFoundError::new(
                EntityName::from("missing_entity".to_string()),
                state
            ))
        );
    }

    #[test]
    fn state_get_mut_entity_should_return_value_on_present_entity() {
        let mut state = State::from_entities(vec![(
            EntityName::from("A".to_string()),
            Entity::from_resources(vec![(
                ResourceName::from("Resource".to_string()),
                Amount::from(0.),
            )]),
        )]);

        assert_eq!(
            state.entity_mut(&EntityName::from("A".to_string()),),
            Ok(&mut Entity::from_resources(vec![(
                ResourceName::from("Resource".to_string()),
                Amount::from(0.)
            )]))
        );
    }

    #[test]
    fn state_get_mut_entity_should_return_error_on_missing_entity() {
        let mut state = State::from_entities(vec![(
            EntityName::from("A".to_string()),
            Entity::from_resources(vec![(
                ResourceName::from("Resource".to_string()),
                Amount::from(0.),
            )]),
        )]);
        assert_eq!(
            state
                .entity_mut(&EntityName::from("missing_entity".to_string()))
                .cloned(),
            Err(NotFoundError::new(
                EntityName::from("missing_entity".to_string()),
                state
            ))
        );
    }

    #[test]
    fn apply_actions_should_apply_actions_to_state() {
        let state = State::from_entities(vec![(
            EntityName::from("A".to_string()),
            Entity::from_resources(vec![
                (ResourceName::from("Resource".to_string()), Amount::from(0.)),
                (
                    ResourceName::from("Resource2".to_string()),
                    Amount::from(0.),
                ),
            ]),
        )]);
        let actions = HashMap::from([
            (
                ActionName::from("Action 1".to_string()),
                Action::from(
                    ResourceName::from("Resource".to_string()),
                    EntityName::from("A".to_string()),
                    Amount::from(1.),
                ),
            ),
            (
                ActionName::from("Action 2".to_string()),
                Action::from(
                    ResourceName::from("Resource2".to_string()),
                    EntityName::from("A".to_string()),
                    Amount::from(2.),
                ),
            ),
        ]);
        let new_state = state.apply_actions(actions);
        assert_eq!(
            new_state,
            State::from_entities(vec![(
                EntityName::from("A".to_string()),
                Entity::from_resources(vec![
                    (ResourceName::from("Resource".to_string()), Amount::from(1.)),
                    (
                        ResourceName::from("Resource2".to_string()),
                        Amount::from(2.)
                    ),
                ]),
            )])
        );
    }

    #[test]
    fn possible_states_append_state() {
        let state = State::from_entities(vec![(
            EntityName::from("A".to_string()),
            Entity::from_resources(vec![
                (ResourceName::from("Resource".to_string()), Amount::from(0.)),
                (
                    ResourceName::from("Resource2".to_string()),
                    Amount::from(0.),
                ),
            ]),
        )]);
        let state_hash = StateHash::from_state(&state);
        let mut possible_states = PossibleStates::new();
        possible_states
            .append_state(state_hash, state.clone())
            .unwrap();
        let expected = HashMap::from([(state_hash, state.clone())]);
        assert_eq!(possible_states.0, expected);

        possible_states.append_state(state_hash, state).unwrap_err();
        assert_eq!(possible_states.0, expected);
    }

    #[test]
    fn reachable_states_append_state() {
        let mut reachable_states = ReachableStates::new();
        let state_hash = StateHash::new();
        let probability = Probability::from(1.);
        reachable_states
            .append_state(state_hash, probability)
            .unwrap();
        let expected = HashMap::from([(state_hash, probability)]);
        assert_eq!(reachable_states.0, expected);

        reachable_states
            .append_state(state_hash, probability)
            .unwrap_err();
        assert_eq!(reachable_states.0, expected);
    }

    #[test]
    fn reachable_states_probability_sum() {
        let mut reachable_states = ReachableStates::new();
        let state_hash = StateHash::new();
        let probability = Probability::from(0.2);
        reachable_states
            .append_state(state_hash, probability)
            .unwrap();
        let state_hash = StateHash::from_state(&State::from_entities(vec![(
            EntityName::from("A".to_string()),
            Entity::from_resources(vec![(
                ResourceName::from("Resource".to_string()),
                Amount::from(0.),
            )]),
        )]));
        let probability = Probability::from(0.5);
        reachable_states
            .append_state(state_hash, probability)
            .unwrap();
        assert_eq!(reachable_states.probability_sum(), Probability::from(0.7));
    }

    #[test]
    fn reachable_states_entropy() {
        let mut reachable_states = ReachableStates::new();
        assert_eq!(reachable_states.entropy(), Entropy::from(0.));
        let state_hash = StateHash::new();
        let probability = Probability::from(0.5);
        reachable_states
            .append_state(state_hash, probability)
            .unwrap();
        let state_hash = StateHash::from_state(&State::from_entities(vec![(
            EntityName::from("A".to_string()),
            Entity::from_resources(vec![(
                ResourceName::from("Resource".to_string()),
                Amount::from(0.),
            )]),
        )]));
        let probability = Probability::from(0.5);
        reachable_states
            .append_state(state_hash, probability)
            .unwrap();
        assert_eq!(reachable_states.entropy(), Entropy::from(1.));
    }
}
