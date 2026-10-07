// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `tpt-concord-sim` — deterministic simulation (spec §5).
//!
//! - **virtual time**: a [`Scheduler`] orders events by `(time, seq)` — no
//!   wall clock anywhere;
//! - **controlled scheduling**: the event order is data, so the same
//!   schedule always drives the same simulation;
//! - **fault injection**: faults are scheduled objects, applied at exact
//!   steps and recorded in the log;
//! - **state-space exploration**: breadth-first sweep of a deterministic
//!   machine with bad-state detection and a minimal witness path;
//! - **replay**: identical (inputs, faults) → identical logs, verified.

pub mod explore;
pub mod scheduler;

pub use explore::{explore, ExplorationReport};
pub use scheduler::{ScheduledEvent, Scheduler};

use serde::{Deserialize, Serialize};

/// A fault injected at an exact step of a simulation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fault {
    /// Zero-based step index the fault applies to.
    pub at_step: usize,
    /// Machine-interpretable fault kind (e.g. `"disk.truncate"`, `"net.drop"`).
    pub kind: String,
}

/// One recorded step of a simulation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRecord<S, I> {
    pub step: usize,
    pub input: I,
    /// Fault applied at this step, if any.
    pub fault: Option<String>,
    pub before: S,
    pub after: S,
}

/// Log of a full simulation — the replay artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationLog<S, I> {
    pub steps: Vec<StepRecord<S, I>>,
    /// Kinds of faults actually applied, in order.
    pub faults_applied: Vec<String>,
}

/// Run a deterministic simulation with fault injection.
///
/// `step` receives the current state, the input, and the fault scheduled for
/// this step (if any) — machines decide what a fault means. Determinism:
/// identical (initial, inputs, faults, step-semantics) → identical log.
pub fn run_with_faults<S, I, F, E>(
    initial: S,
    inputs: &[I],
    faults: &[Fault],
    mut step: F,
) -> Result<SimulationLog<S, I>, E>
where
    I: Clone,
    S: Clone,
    F: FnMut(S, &I, Option<&Fault>) -> Result<S, E>,
{
    let mut state = initial;
    let mut steps = Vec::with_capacity(inputs.len());
    let mut faults_applied = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        let fault = faults.iter().find(|f| f.at_step == i);
        let before = state.clone();
        state = step(state, input, fault)?;
        if let Some(f) = fault {
            faults_applied.push(f.kind.clone());
        }
        steps.push(StepRecord {
            step: i,
            input: input.clone(),
            fault: fault.map(|f| f.kind.clone()),
            before,
            after: state.clone(),
        });
    }
    Ok(SimulationLog {
        steps,
        faults_applied,
    })
}

/// Replay a simulation: same log twice from the same inputs and faults is
/// bit-identical, or the replay diverges (reported as `Err`).
pub fn verify_replay<S, I, F, E>(
    initial: S,
    inputs: &[I],
    faults: &[Fault],
    step: F,
) -> Result<(), String>
where
    I: Clone + PartialEq,
    S: Clone + PartialEq + std::fmt::Debug,
    E: std::fmt::Display,
    F: FnMut(S, &I, Option<&Fault>) -> Result<S, E> + Clone,
{
    let mut a = Some(step.clone());
    let b = step;
    let log_a = run_with_faults(initial.clone(), inputs, faults, a.take().unwrap())
        .map_err(|e| e.to_string())?;
    let log_b = run_with_faults(initial, inputs, faults, b).map_err(|e| e.to_string())?;
    if log_a == log_b {
        Ok(())
    } else {
        Err(format!(
            "replay diverged: {} steps vs {} steps",
            log_a.steps.len(),
            log_b.steps.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny node: a counter with a reset, where an injected
    /// `"reset.lost"` fault makes the reset a no-op.
    fn counter_step(state: i64, input: &i64, fault: Option<&Fault>) -> Result<i64, String> {
        let faulted = fault.is_some_and(|f| f.kind == "reset.lost");
        Ok(match input {
            0 if !faulted => 0,
            0 => state, // fault: reset lost
            n => state + n,
        })
    }

    #[test]
    fn faults_apply_at_exact_steps() {
        let inputs = vec![1, 1, 0, 1];
        let faults = vec![Fault {
            at_step: 2,
            kind: "reset.lost".into(),
        }];
        let log = run_with_faults(0, &inputs, &faults, counter_step).unwrap();
        // Counter reaches 2, the faulted reset is swallowed (stays 2), then +1.
        assert_eq!(log.steps[2].after, 2, "reset swallowed by fault");
        assert_eq!(log.steps[3].after, 3);
        assert_eq!(log.faults_applied, vec!["reset.lost".to_string()]);
    }

    #[test]
    fn replay_is_bit_identical() {
        let inputs = vec![1, 1, 0, 1, 1];
        let faults = vec![Fault {
            at_step: 2,
            kind: "reset.lost".into(),
        }];
        assert!(verify_replay(0, &inputs, &faults, counter_step).is_ok());
    }

    #[test]
    fn logs_serialize_for_evidence() {
        let inputs = vec![1, 0];
        let log = run_with_faults(0, &inputs, &[], counter_step).unwrap();
        let json = serde_json::to_string(&log).unwrap();
        let back: SimulationLog<i64, i64> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, log);
    }
}
