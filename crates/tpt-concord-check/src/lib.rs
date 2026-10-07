// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `tpt-concord-check` — the conformance engine (spec §5).
//!
//! Bridges implementations and executable reference models:
//! - step-level conformance against a [`Specification`],
//! - invariant checking over states and traces,
//! - refinement checking against abstract models,
//! - deviation classification,
//! - first-class [`Counterexample`]s with a re-check workflow.

pub mod classify;
pub mod conformance;
pub mod counterexample;
pub mod refinement;

pub use classify::{classify_divergence, classify_violation, DeviationClass};
pub use conformance::{check_states, check_steps, check_trace, ConformanceReport, StepView};
pub use counterexample::{Counterexample, CounterexampleStatus, RecheckOutcome};
pub use refinement::{refinement_check, RefinementFailure, RefinementReport};

/// Errors from conformance checking.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CheckError {
    #[error(
        "input/state arity mismatch: expected {expected} states for {inputs} inputs, got {got}"
    )]
    ArityMismatch {
        expected: usize,
        inputs: usize,
        got: usize,
    },
    #[error("malformed trace event payload: {0}")]
    MalformedEventPayload(String),
}
