// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Executable reference models (spec §5, `tpt-concord-model`).
//!
//! Two shapes of model:
//! - [`DeterministicMachine`]: the executable reference — same state + input
//!   always produces the same outcome. Runs produce [`RunRecord`]s suitable
//!   for canonical traces.
//! - [`AbstractModel`]: a (possibly nondeterministic) permission structure —
//!   which transitions are allowed — used as the target of refinement
//!   checking in `tpt-concord-check`.
//!
//! [`TransitionTable`] and [`AbstractTable`] are ready-made table-driven
//! implementations; most real models implement the traits directly.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use thiserror::Error;

/// Outcome of one deterministic step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepOutcome<S> {
    /// State after the step.
    pub next: S,
    /// Effect labels produced by the step (for effect conformance).
    pub effects: BTreeSet<String>,
}

impl<S> StepOutcome<S> {
    /// A step with no effects.
    pub fn to(next: S) -> Self {
        Self {
            next,
            effects: BTreeSet::new(),
        }
    }

    /// A step producing the given effect labels.
    pub fn with_effects(next: S, effects: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            next,
            effects: effects.into_iter().map(Into::into).collect(),
        }
    }
}

/// A deterministic reference model: the reference semantics.
///
/// `step` must be a total function of `(state, input)` — same inputs, same
/// output, always. Non-determinism belongs in [`AbstractModel`], explored by
/// `tpt-concord-sim` / `tpt-concord-property`.
pub trait DeterministicMachine {
    type State: Clone;
    type Input: Clone;
    type Error;

    /// The designated initial state.
    fn initial_state(&self) -> Self::State;

    /// The transition function.
    fn step(
        &self,
        state: &Self::State,
        input: &Self::Input,
    ) -> Result<StepOutcome<Self::State>, Self::Error>;
}

/// One recorded step of a run: input, before-state, after-state, effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord<S, I> {
    /// Zero-based step index within the run.
    pub step: usize,
    pub input: I,
    pub before: S,
    pub after: S,
    pub effects: BTreeSet<String>,
}

/// Recorded steps of a machine run.
pub type RunLog<M> =
    Vec<RunRecord<<M as DeterministicMachine>::State, <M as DeterministicMachine>::Input>>;

/// Run a machine over a sequence of inputs, recording every step.
///
/// Stops at the first transition error.
pub fn run<M>(machine: &M, inputs: &[M::Input]) -> Result<RunLog<M>, M::Error>
where
    M: DeterministicMachine,
{
    let mut state = machine.initial_state();
    let mut records = Vec::with_capacity(inputs.len());
    for (i, input) in inputs.iter().enumerate() {
        let before = state.clone();
        let outcome = machine.step(&state, input)?;
        state = outcome.next;
        records.push(RunRecord {
            step: i,
            input: input.clone(),
            before,
            after: state.clone(),
            effects: outcome.effects,
        });
    }
    Ok(records)
}

/// Error produced by table-driven machines.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransitionTableError {
    #[error("no transition defined for state `{state}` on input `{input}`")]
    UnknownTransition { state: String, input: String },
}

/// A deterministic, table-driven machine: `(state, input) -> state`.
///
/// States and inputs must be serializable and ordered so the whole model is a
/// machine-readable object. Serialized as an explicit transition list (JSON
/// has no tuple keys).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionTable<S, I>
where
    S: Ord + Clone,
    I: Ord + Clone,
{
    pub initial: S,
    /// Transition function: (state, input) -> next state.
    pub table: BTreeMap<(S, I), S>,
    /// Optional effect labels per (state, input).
    pub effects: BTreeMap<(S, I), BTreeSet<String>>,
}

#[derive(Serialize, Deserialize)]
struct TransitionEntry<S, I> {
    from: S,
    input: I,
    to: S,
    #[serde(default)]
    effects: BTreeSet<String>,
}

impl<S, I> Serialize for TransitionTable<S, I>
where
    S: Ord + Clone + Serialize,
    I: Ord + Clone + Serialize,
{
    fn serialize<Se>(&self, serializer: Se) -> Result<Se::Ok, Se::Error>
    where
        Se: serde::Serializer,
    {
        let entries: Vec<TransitionEntry<S, I>> = self
            .table
            .iter()
            .map(|((from, input), to)| TransitionEntry {
                from: from.clone(),
                input: input.clone(),
                to: to.clone(),
                effects: self
                    .effects
                    .get(&(from.clone(), input.clone()))
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect();
        #[derive(Serialize)]
        struct Repr<'a, S, I> {
            initial: &'a S,
            transitions: &'a [TransitionEntry<S, I>],
        }
        Repr {
            initial: &self.initial,
            transitions: &entries,
        }
        .serialize(serializer)
    }
}

impl<'de, S, I> Deserialize<'de> for TransitionTable<S, I>
where
    S: Ord + Clone + Deserialize<'de>,
    I: Ord + Clone + Deserialize<'de>,
{
    fn deserialize<De>(deserializer: De) -> Result<Self, De::Error>
    where
        De: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Repr<S, I> {
            initial: S,
            transitions: Vec<TransitionEntry<S, I>>,
        }
        let repr = Repr::<S, I>::deserialize(deserializer)?;
        let mut table = TransitionTable::new(repr.initial);
        for entry in repr.transitions {
            if !entry.effects.is_empty() {
                table
                    .effects
                    .insert((entry.from.clone(), entry.input.clone()), entry.effects);
            }
            table.table.insert((entry.from, entry.input), entry.to);
        }
        Ok(table)
    }
}

impl<S, I> TransitionTable<S, I>
where
    S: Ord + Clone,
    I: Ord + Clone,
{
    pub fn new(initial: S) -> Self {
        Self {
            initial,
            table: BTreeMap::new(),
            effects: BTreeMap::new(),
        }
    }

    /// Declare `from --input--> to`.
    pub fn transition(&mut self, from: S, input: I, to: S) -> &mut Self {
        self.table.insert((from, input), to);
        self
    }

    /// Declare effect labels for `(state, input)`.
    pub fn effects(&mut self, from: S, input: I, effects: &[&str]) -> &mut Self {
        self.effects.insert(
            (from, input),
            effects.iter().map(|s| s.to_string()).collect(),
        );
        self
    }
}

impl<S, I> DeterministicMachine for TransitionTable<S, I>
where
    S: Ord + Clone + fmt::Display,
    I: Ord + Clone + fmt::Display,
{
    type State = S;
    type Input = I;
    type Error = TransitionTableError;

    fn initial_state(&self) -> S {
        self.initial.clone()
    }

    fn step(&self, state: &S, input: &I) -> Result<StepOutcome<S>, TransitionTableError> {
        match self.table.get(&(state.clone(), input.clone())) {
            Some(next) => Ok(StepOutcome {
                next: next.clone(),
                effects: self
                    .effects
                    .get(&(state.clone(), input.clone()))
                    .cloned()
                    .unwrap_or_default(),
            }),
            None => Err(TransitionTableError::UnknownTransition {
                state: state.to_string(),
                input: input.to_string(),
            }),
        }
    }
}

/// A (possibly nondeterministic) abstract model: which transitions are
/// *allowed*. The target of refinement checking — every concrete behaviour
/// must stay inside the abstract permission set.
pub trait AbstractModel {
    type State: Clone + Ord;
    type Input: Clone + Ord;

    /// The designated initial state.
    fn initial(&self) -> Self::State;

    /// All states permitted after taking `input` from `state`.
    /// An empty set means the transition is forbidden.
    fn transitions(&self, state: &Self::State, input: &Self::Input) -> BTreeSet<Self::State>;
}

/// Every deterministic machine is also an abstract model with a singleton
/// permission set per transition (an illegal step permits nothing).
impl<M> AbstractModel for M
where
    M: DeterministicMachine,
    M::State: Clone + Ord,
    M::Input: Clone + Ord,
{
    type State = M::State;
    type Input = M::Input;

    fn initial(&self) -> Self::State {
        self.initial_state()
    }

    fn transitions(&self, state: &Self::State, input: &Self::Input) -> BTreeSet<Self::State> {
        match self.step(state, input) {
            Ok(outcome) => {
                let mut set = BTreeSet::new();
                set.insert(outcome.next);
                set
            }
            Err(_) => BTreeSet::new(),
        }
    }
}

/// A table-driven abstract model: `(state, input) -> {permitted next states}`.
///
/// Serialized as an explicit permission list (JSON has no tuple keys).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AbstractTable<S, I>
where
    S: Ord + Clone,
    I: Ord + Clone,
{
    pub initial: S,
    /// Permission relation: (state, input) -> permitted next states.
    pub table: BTreeMap<(S, I), BTreeSet<S>>,
}

impl<S, I> Serialize for AbstractTable<S, I>
where
    S: Ord + Clone + Serialize,
    I: Ord + Clone + Serialize,
{
    fn serialize<Se>(&self, serializer: Se) -> Result<Se::Ok, Se::Error>
    where
        Se: serde::Serializer,
    {
        #[derive(Serialize)]
        struct Entry<'a, S, I> {
            from: &'a S,
            input: &'a I,
            to: Vec<&'a S>,
        }
        let entries: Vec<Entry<S, I>> = self
            .table
            .iter()
            .map(|((from, input), tos)| Entry {
                from,
                input,
                to: tos.iter().collect(),
            })
            .collect();
        #[derive(Serialize)]
        struct Repr<'a, S, I> {
            initial: &'a S,
            permitted: &'a [Entry<'a, S, I>],
        }
        Repr {
            initial: &self.initial,
            permitted: &entries,
        }
        .serialize(serializer)
    }
}

impl<'de, S, I> Deserialize<'de> for AbstractTable<S, I>
where
    S: Ord + Clone + Deserialize<'de>,
    I: Ord + Clone + Deserialize<'de>,
{
    fn deserialize<De>(deserializer: De) -> Result<Self, De::Error>
    where
        De: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Entry<S, I> {
            from: S,
            input: I,
            to: Vec<S>,
        }
        #[derive(Deserialize)]
        struct Repr<S, I> {
            initial: S,
            permitted: Vec<Entry<S, I>>,
        }
        let repr = Repr::<S, I>::deserialize(deserializer)?;
        let mut table = AbstractTable::new(repr.initial);
        for entry in repr.permitted {
            let set = table.table.entry((entry.from, entry.input)).or_default();
            for to in entry.to {
                set.insert(to);
            }
        }
        Ok(table)
    }
}

impl<S, I> AbstractTable<S, I>
where
    S: Ord + Clone,
    I: Ord + Clone,
{
    pub fn new(initial: S) -> Self {
        Self {
            initial,
            table: BTreeMap::new(),
        }
    }

    /// Permit `from --input--> to`.
    pub fn permit(&mut self, from: S, input: I, to: S) -> &mut Self {
        self.table.entry((from, input)).or_default().insert(to);
        self
    }
}

impl<S, I> AbstractModel for AbstractTable<S, I>
where
    S: Clone + Ord,
    I: Clone + Ord,
{
    type State = S;
    type Input = I;

    fn initial(&self) -> S {
        self.initial.clone()
    }

    fn transitions(&self, state: &S, input: &I) -> BTreeSet<S> {
        self.table
            .get(&(state.clone(), input.clone()))
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    enum Op {
        Push,
        Pop,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    struct St {
        depth: i64,
    }

    impl fmt::Display for St {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "depth={}", self.depth)
        }
    }

    impl fmt::Display for Op {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Op::Push => f.write_str("push"),
                Op::Pop => f.write_str("pop"),
            }
        }
    }

    fn counter() -> TransitionTable<St, Op> {
        let mut t = TransitionTable::new(St { depth: 0 });
        t.transition(St { depth: 0 }, Op::Push, St { depth: 1 });
        t.transition(St { depth: 1 }, Op::Push, St { depth: 2 });
        t.transition(St { depth: 2 }, Op::Pop, St { depth: 1 });
        t.transition(St { depth: 1 }, Op::Pop, St { depth: 0 });
        t.effects(St { depth: 0 }, Op::Push, &["stack.grow"]);
        t
    }

    #[test]
    fn deterministic_run_records() {
        let m = counter();
        let records = run(&m, &[Op::Push, Op::Push, Op::Pop]).unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[2].after, St { depth: 1 });
        assert_eq!(
            records[0].effects,
            BTreeSet::from(["stack.grow".to_string()])
        );
        assert_eq!(records[2].effects, BTreeSet::new());
    }

    #[test]
    fn determinism_same_input_same_output() {
        let m = counter();
        let a = run(&m, &[Op::Push, Op::Pop, Op::Push]).unwrap();
        let b = run(&m, &[Op::Push, Op::Pop, Op::Push]).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn unknown_transition_is_error() {
        let m = counter();
        let err = m.step(&St { depth: 2 }, &Op::Push).unwrap_err();
        assert!(matches!(
            err,
            TransitionTableError::UnknownTransition { .. }
        ));
        assert!(run(&m, &[Op::Pop]).is_err());
    }

    #[test]
    fn abstract_table_permits_sets() {
        let mut t: AbstractTable<St, Op> = AbstractTable::new(St { depth: 0 });
        t.permit(St { depth: 0 }, Op::Push, St { depth: 1 });
        t.permit(St { depth: 0 }, Op::Push, St { depth: 7 }); // nondeterministic
        let next = AbstractModel::transitions(&t, &St { depth: 0 }, &Op::Push);
        assert_eq!(next.len(), 2);
        assert!(AbstractModel::transitions(&t, &St { depth: 1 }, &Op::Push).is_empty());
    }

    #[test]
    fn tables_serialize() {
        let m = counter();
        let json = serde_json::to_string(&m).unwrap();
        let back: TransitionTable<St, Op> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }
}
