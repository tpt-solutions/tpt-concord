// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! The Archon-style WAL domain: specification, reference model, and two
//! simulated implementations (one conformant, one deliberately broken).
//!
//! Claim under test (scope: *prefix-crash model* — a crash may truncate the
//! log at any point, but never corrupt surviving records):
//!
//! > WAL recovery preserves committed transactions: after recovery, the
//! > committed prefix contains exactly the records committed before the
//! > crash, and no uncommitted record enters the committed prefix.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tpt_concord_core::{ModelID, SpecificationID, Version};
use tpt_concord_model::{DeterministicMachine, StepOutcome};
use tpt_concord_spec::{Effect, Expr, Specification, Transition, ValueType};

/// Concrete WAL state of the reference model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WalState {
    /// Log records in write order.
    pub log: Vec<String>,
    /// Number of leading records that are committed (the watermark).
    pub committed_len: usize,
    /// True between a crash and the following recovery.
    pub crashed: bool,
}

/// WAL operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WalInput {
    /// Append an uncommitted record.
    Append(String),
    /// Commit everything appended so far.
    Commit,
    /// Crash: the surviving disk prefix is `log[..k]`.
    CrashTo(usize),
    /// Recover from the crashed state.
    Recover,
}

impl std::fmt::Display for WalInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WalInput::Append(r) => write!(f, "append({r})"),
            WalInput::Commit => f.write_str("commit"),
            WalInput::CrashTo(k) => write!(f, "crash_to({k})"),
            WalInput::Recover => f.write_str("recover"),
        }
    }
}

/// Model transition errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WalModelError {
    #[error("operation `{op}` is illegal while crashed")]
    IllegalWhileCrashed { op: &'static str },
    #[error("recover requires a crashed state")]
    RecoverWithoutCrash,
    #[error("crash point {k} exceeds log length {len}")]
    CrashBeyondLog { k: usize, len: usize },
}

/// The reference model: what a correct WAL does.
///
/// Recovery restores `committed_len = min(committed_len_before, surviving
/// log length)` — committed transactions survive; uncommitted tail records
/// are discarded.
#[derive(Debug, Clone, Default)]
pub struct WalModel;

impl DeterministicMachine for WalModel {
    type State = WalState;
    type Input = WalInput;
    type Error = WalModelError;

    fn initial_state(&self) -> WalState {
        WalState::default()
    }

    fn step(
        &self,
        state: &WalState,
        input: &WalInput,
    ) -> Result<StepOutcome<WalState>, WalModelError> {
        let mut next = state.clone();
        let effects = match input {
            WalInput::Append(record) => {
                if state.crashed {
                    return Err(WalModelError::IllegalWhileCrashed { op: "append" });
                }
                next.log.push(record.clone());
                BTreeSet::from(["wal.append".to_string()])
            }
            WalInput::Commit => {
                if state.crashed {
                    return Err(WalModelError::IllegalWhileCrashed { op: "commit" });
                }
                next.committed_len = next.log.len();
                BTreeSet::from(["wal.commit".to_string()])
            }
            WalInput::CrashTo(k) => {
                if state.crashed {
                    return Err(WalModelError::IllegalWhileCrashed { op: "crash_to" });
                }
                if *k > next.log.len() {
                    return Err(WalModelError::CrashBeyondLog {
                        k: *k,
                        len: next.log.len(),
                    });
                }
                next.log.truncate(*k);
                next.crashed = true;
                BTreeSet::from(["wal.crash".to_string()])
            }
            WalInput::Recover => {
                if !state.crashed {
                    return Err(WalModelError::RecoverWithoutCrash);
                }
                // The committed watermark can never exceed the surviving log;
                // committed records within the surviving prefix are preserved.
                next.committed_len = next.committed_len.min(next.log.len());
                next.crashed = false;
                BTreeSet::from(["wal.recover".to_string()])
            }
        };
        Ok(StepOutcome { next, effects })
    }
}

/// The claim property, checked directly on model/impl states: the committed
/// prefix after recovery must equal the committed prefix that existed before
/// the crash (under the prefix-crash scope, i.e. the crash truncation point
/// is >= the committed watermark at crash time).
pub fn committed_prefix_preserved(before: &WalState, after: &WalState) -> bool {
    let before_prefix = &before.log[..before.committed_len.min(before.log.len())];
    let after_prefix = &after.log[..after.committed_len.min(after.log.len())];
    before_prefix == after_prefix
}

/// A conformant implementation: same observable semantics as the model,
/// written independently (recovery via explicit prefix rebuild).
#[derive(Debug, Clone, Default)]
pub struct ConformantWal;

impl DeterministicMachine for ConformantWal {
    type State = WalState;
    type Input = WalInput;
    type Error = WalModelError;

    fn initial_state(&self) -> WalState {
        WalState::default()
    }

    fn step(
        &self,
        state: &WalState,
        input: &WalInput,
    ) -> Result<StepOutcome<WalState>, WalModelError> {
        match input {
            WalInput::Append(record) => {
                if state.crashed {
                    return Err(WalModelError::IllegalWhileCrashed { op: "append" });
                }
                let mut log = state.log.clone();
                log.push(record.clone());
                Ok(StepOutcome::with_effects(
                    WalState {
                        log,
                        committed_len: state.committed_len,
                        crashed: false,
                    },
                    ["wal.append"],
                ))
            }
            WalInput::Commit => {
                if state.crashed {
                    return Err(WalModelError::IllegalWhileCrashed { op: "commit" });
                }
                Ok(StepOutcome::with_effects(
                    WalState {
                        committed_len: state.log.len(),
                        ..state.clone()
                    },
                    ["wal.commit"],
                ))
            }
            WalInput::CrashTo(k) => {
                if state.crashed {
                    return Err(WalModelError::IllegalWhileCrashed { op: "crash_to" });
                }
                if *k > state.log.len() {
                    return Err(WalModelError::CrashBeyondLog {
                        k: *k,
                        len: state.log.len(),
                    });
                }
                Ok(StepOutcome::with_effects(
                    WalState {
                        log: state.log[..*k].to_vec(),
                        committed_len: state.committed_len,
                        crashed: true,
                    },
                    ["wal.crash"],
                ))
            }
            WalInput::Recover => {
                if !state.crashed {
                    return Err(WalModelError::RecoverWithoutCrash);
                }
                // Independent formulation: the durable prefix is exactly the
                // records committed before the crash; the watermark is
                // clamped to the surviving log.
                let committed_len = state.committed_len.min(state.log.len());
                Ok(StepOutcome::with_effects(
                    WalState {
                        log: state.log.clone(),
                        committed_len,
                        crashed: false,
                    },
                    ["wal.recover"],
                ))
            }
        }
    }
}

/// A deliberately broken implementation: recovery treats the *whole
/// surviving log* as committed, letting uncommitted records into the
/// committed prefix. The classic WAL recovery bug.
#[derive(Debug, Clone, Default)]
pub struct NaiveWal;

impl DeterministicMachine for NaiveWal {
    type State = WalState;
    type Input = WalInput;
    type Error = WalModelError;

    fn initial_state(&self) -> WalState {
        WalState::default()
    }

    fn step(
        &self,
        state: &WalState,
        input: &WalInput,
    ) -> Result<StepOutcome<WalState>, WalModelError> {
        match input {
            WalInput::Recover => {
                if !state.crashed {
                    return Err(WalModelError::RecoverWithoutCrash);
                }
                // BUG: marks everything surviving as committed.
                Ok(StepOutcome::with_effects(
                    WalState {
                        committed_len: state.log.len(),
                        ..state.clone()
                    },
                    ["wal.recover"],
                ))
            }
            other => WalModel.step(state, other),
        }
    }
}

/// Map a [`WalState`] into the specification's state space.
pub fn wal_state_to_spec(state: &WalState) -> tpt_concord_spec::State {
    let mut s = tpt_concord_spec::State::new();
    s.insert(
        "committed_len".to_string(),
        tpt_concord_spec::Value::Int(state.committed_len as i128),
    );
    s.insert(
        "log".to_string(),
        tpt_concord_spec::Value::Seq(
            state
                .log
                .iter()
                .map(|r| tpt_concord_spec::Value::Str(r.clone()))
                .collect(),
        ),
    );
    s.insert(
        "crashed".to_string(),
        tpt_concord_spec::Value::Bool(state.crashed),
    );
    s
}

/// The machine-readable WAL specification.
pub fn wal_specification() -> Specification {
    let mut spec = Specification::new(
        SpecificationID::new("archon.wal.spec").unwrap(),
        Version::new("0.1.0").unwrap(),
        "Archon WAL/transaction behaviour",
    );
    spec.description =
        "Write-ahead log with explicit commit watermark and crash recovery under the \
         prefix-crash model: a crash truncates the log at an arbitrary point but never \
         corrupts surviving records."
            .to_string();

    spec.state_vars = BTreeMap::from([
        ("committed_len".to_string(), ValueType::Int),
        ("log".to_string(), ValueType::Seq(Box::new(ValueType::Str))),
        ("crashed".to_string(), ValueType::Bool),
    ]);

    spec.invariants = vec![
        (
            "committed_nonnegative",
            Expr::ge(Expr::var("committed_len"), Expr::int(0)),
        ),
        (
            "committed_within_log",
            Expr::le(Expr::var("committed_len"), Expr::len(Expr::var("log"))),
        ),
    ]
    .into_iter()
    .map(|(name, expr)| tpt_concord_spec::Named::new(name, expr))
    .collect();

    // Transitions with preconditions and declared effects.
    let mut append = Transition::new("append");
    append.pre = vec![tpt_concord_spec::Named::new(
        "not_crashed",
        !(Expr::var("crashed")),
    )];
    append.effects = BTreeSet::from([Effect::new("wal.append")]);

    let mut commit = Transition::new("commit");
    commit.pre = vec![tpt_concord_spec::Named::new(
        "not_crashed",
        !(Expr::var("crashed")),
    )];
    commit.post = vec![tpt_concord_spec::Named::new(
        "watermark_nonnegative",
        Expr::ge(Expr::var("committed_len"), Expr::int(0)),
    )];
    commit.effects = BTreeSet::from([Effect::new("wal.commit")]);

    let mut crash_to = Transition::new("crash_to");
    crash_to.pre = vec![tpt_concord_spec::Named::new(
        "not_crashed",
        !(Expr::var("crashed")),
    )];
    crash_to.effects = BTreeSet::from([Effect::new("wal.crash")]);

    let mut recover = Transition::new("recover");
    recover.pre = vec![tpt_concord_spec::Named::new(
        "was_crashed",
        Expr::var("crashed"),
    )];
    recover.post = vec![tpt_concord_spec::Named::new(
        "clean_after_recovery",
        !(Expr::var("crashed")),
    )];
    recover.effects = BTreeSet::from([Effect::new("wal.recover")]);

    spec.transitions = vec![append, commit, crash_to, recover];

    // Resource bounds (per run, enforced by the caller's counters).
    spec.resource_bounds = vec![tpt_concord_spec::ResourceBound {
        resource: "steps".to_string(),
        max: 10_000,
        scope: tpt_concord_spec::BoundScope::PerRun,
    }];

    spec
}

/// The executable model as a registered object.
pub fn wal_model_id() -> ModelID {
    ModelID::new("archon.wal.model").unwrap()
}

/// Convenience: full spec ID.
pub fn wal_spec_id() -> SpecificationID {
    SpecificationID::new("archon.wal.spec").unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_concord_model::run;

    fn inputs() -> Vec<WalInput> {
        vec![
            WalInput::Append("t1".into()),
            WalInput::Commit,
            WalInput::Append("t2-uncommitted".into()),
            WalInput::CrashTo(2),
            WalInput::Recover,
        ]
    }

    #[test]
    fn model_preserves_committed_and_discards_uncommitted() {
        let records = run(&WalModel, &inputs()).unwrap();
        let final_state = records.last().unwrap().after.clone();
        // The uncommitted record survives in the log but stays uncommitted.
        assert_eq!(
            final_state.log,
            vec!["t1".to_string(), "t2-uncommitted".to_string()]
        );
        assert_eq!(final_state.committed_len, 1);
        assert!(!final_state.crashed);
        // The committed prefix is exactly the pre-crash committed prefix.
        assert!(committed_prefix_preserved(&records[2].after, &final_state));
    }

    #[test]
    fn conformant_impl_matches_model() {
        let model_records = run(&WalModel, &inputs()).unwrap();
        let impl_records = run(&ConformantWal, &inputs()).unwrap();
        assert_eq!(model_records, impl_records);
    }

    #[test]
    fn naive_impl_lets_uncommitted_into_committed_prefix() {
        let records = run(&NaiveWal, &inputs()).unwrap();
        let final_state = records.last().unwrap().after.clone();
        // BUG demonstrated: uncommitted `t2` is now "committed".
        assert_eq!(final_state.committed_len, 2);
        assert_ne!(records.last().unwrap(), &model_last());
    }

    fn model_last() -> tpt_concord_model::RunRecord<WalState, WalInput> {
        run(&WalModel, &inputs()).unwrap().pop().unwrap()
    }

    #[test]
    fn spec_invariants_hold_on_model_states() {
        let spec = wal_specification();
        for record in run(&WalModel, &inputs()).unwrap() {
            assert!(
                spec.check_invariants(&wal_state_to_spec(&record.after))
                    .is_empty(),
                "invariant violated in model state {record:?}"
            );
        }
    }

    #[test]
    fn spec_transition_rules_match_model_behaviour() {
        let spec = wal_specification();
        for record in run(&WalModel, &inputs()).unwrap() {
            let name = match record.input {
                WalInput::Append(_) => "append",
                WalInput::Commit => "commit",
                WalInput::CrashTo(_) => "crash_to",
                WalInput::Recover => "recover",
            };
            let before = wal_state_to_spec(&record.before);
            let after = wal_state_to_spec(&record.after);
            assert!(
                spec.check_transition_pre(name, &before).is_empty(),
                "{name} pre"
            );
            assert!(
                spec.check_transition_post(name, &after).is_empty(),
                "{name} post"
            );
            assert!(
                spec.check_effects(
                    name,
                    &record.effects.iter().map(|e| Effect(e.clone())).collect()
                )
                .is_empty(),
                "{name} effects"
            );
        }
    }

    #[test]
    fn illegal_operations_are_model_errors() {
        let mut state = WalState::default();
        assert!(matches!(
            WalModel.step(&state, &WalInput::Recover),
            Err(WalModelError::RecoverWithoutCrash)
        ));
        state.crashed = true;
        assert!(matches!(
            WalModel.step(&state, &WalInput::Append("x".into())),
            Err(WalModelError::IllegalWhileCrashed { .. })
        ));
    }
}
