// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! State-space exploration of deterministic machines.
//!
//! Breadth-first sweep over all legal input sequences up to a depth bound,
//! with a visited set (states are compared for equality, not re-explored)
//! and bad-state detection carrying a minimal witness path.

use std::collections::{HashSet, VecDeque};
use tpt_concord_model::DeterministicMachine;

/// Outcome of an exploration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorationReport<S, I> {
    /// Distinct states reached.
    pub states_reached: usize,
    /// Legal transitions actually exercised.
    pub transitions_explored: usize,
    /// Deepest BFS level reached (initial state = depth 0).
    pub max_depth_reached: usize,
    /// A bad state, with the minimal input path reaching it.
    pub bad_state: Option<(S, Vec<I>)>,
}

impl<S, I> ExplorationReport<S, I> {
    pub fn found_bad_state(&self) -> bool {
        self.bad_state.is_some()
    }
}

/// Explore the reachable state space of `machine` up to `max_depth` steps
/// using `alphabet` as the input language. Illegal transitions (machine
/// errors) lead outside the legal space and are skipped. `is_bad` marks
/// failure states; exploration stops at the first one found
/// (BFS ⇒ minimal witness).
pub fn explore<M>(
    machine: &M,
    alphabet: &[M::Input],
    max_depth: usize,
    is_bad: impl Fn(&M::State) -> bool,
) -> Result<ExplorationReport<M::State, M::Input>, M::Error>
where
    M: DeterministicMachine,
    M::State: Eq + std::hash::Hash + Clone,
    M::Input: Clone,
{
    explore_inner(machine, alphabet, max_depth, usize::MAX, is_bad)
}

/// Like [`explore`] with an additional global budget on exercised
/// transitions — for large or infinite spaces.
pub fn explore_bounded<M>(
    machine: &M,
    alphabet: &[M::Input],
    max_depth: usize,
    max_transitions: usize,
    is_bad: impl Fn(&M::State) -> bool,
) -> Result<ExplorationReport<M::State, M::Input>, M::Error>
where
    M: DeterministicMachine,
    M::State: Eq + std::hash::Hash + Clone,
    M::Input: Clone,
{
    explore_inner(machine, alphabet, max_depth, max_transitions, is_bad)
}

fn explore_inner<M>(
    machine: &M,
    alphabet: &[M::Input],
    max_depth: usize,
    max_transitions: usize,
    is_bad: impl Fn(&M::State) -> bool,
) -> Result<ExplorationReport<M::State, M::Input>, M::Error>
where
    M: DeterministicMachine,
    M::State: Eq + std::hash::Hash + Clone,
    M::Input: Clone,
{
    let initial = machine.initial_state();
    let mut visited: HashSet<M::State> = HashSet::new();
    visited.insert(initial.clone());

    // (state, path) work queue.
    let mut frontier: VecDeque<(M::State, Vec<M::Input>)> = VecDeque::new();
    frontier.push_back((initial, Vec::new()));

    let mut states_reached = 1usize;
    let mut transitions_explored = 0usize;
    let mut max_depth_reached = 0usize;
    let mut budget = max_transitions;

    while let Some((state, path)) = frontier.pop_front() {
        if path.len() >= max_depth || budget == 0 {
            continue;
        }
        for input in alphabet {
            if budget == 0 {
                break;
            }
            budget -= 1;
            // Illegal transitions lead outside the legal space: skip.
            let Ok(outcome) = machine.step(&state, input) else {
                continue;
            };
            transitions_explored += 1;
            let next = outcome.next;
            if is_bad(&next) {
                let mut witness = path;
                witness.push(input.clone());
                return Ok(ExplorationReport {
                    states_reached,
                    transitions_explored,
                    max_depth_reached: max_depth_reached.max(witness.len()),
                    bad_state: Some((next, witness)),
                });
            }
            if visited.insert(next.clone()) {
                states_reached += 1;
                max_depth_reached = max_depth_reached.max(path.len() + 1);
                let mut next_path = path.clone();
                next_path.push(input.clone());
                frontier.push_back((next, next_path));
            }
        }
    }

    Ok(ExplorationReport {
        states_reached,
        transitions_explored,
        max_depth_reached,
        bad_state: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_concord_model::StepOutcome;

    /// Bounded counter: increments up to 3, reset to 0.
    struct Counter;
    impl DeterministicMachine for Counter {
        type State = i64;
        type Input = i64;
        type Error = std::convert::Infallible;
        fn initial_state(&self) -> i64 {
            0
        }
        fn step(&self, s: &i64, i: &i64) -> Result<StepOutcome<i64>, Self::Error> {
            match i {
                1 => Ok(StepOutcome::to((*s + 1).min(3))),
                _ => Ok(StepOutcome::to(0)),
            }
        }
    }

    #[test]
    fn explores_the_whole_finite_space() {
        let report = explore(&Counter, &[1, 0], 10, |_| false).unwrap();
        // States: 0,1,2,3.
        assert_eq!(report.states_reached, 4);
        assert!(!report.found_bad_state());
        assert_eq!(report.max_depth_reached, 3, "deepest new state is 3");
    }

    #[test]
    fn finds_bad_state_with_minimal_witness() {
        // Bad: counter at 3 (the "overflow").
        let report = explore(&Counter, &[1, 0], 10, |s| *s == 3).unwrap();
        assert!(report.found_bad_state());
        let (state, witness) = report.bad_state.unwrap();
        assert_eq!(state, 3);
        assert_eq!(witness, vec![1, 1, 1], "BFS gives the minimal path");
    }

    #[test]
    fn depth_bound_stops_exploration() {
        let report = explore(&Counter, &[1], 2, |_| false).unwrap();
        assert_eq!(report.max_depth_reached, 2);
        assert!(report.transitions_explored <= 4);
    }

    #[test]
    fn transition_budget_is_respected() {
        let report = explore_bounded(&Counter, &[1, 0], 10, 3, |_| false).unwrap();
        assert!(report.transitions_explored <= 3);
    }
}
