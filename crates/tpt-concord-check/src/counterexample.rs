// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! First-class counterexamples (spec §3.6 / §9).
//!
//! A failure becomes an addressable object carrying everything needed to
//! reproduce and re-check it: the specification and implementation involved,
//! the triggering input/state, the trace up to and including the violation,
//! the environment (toolchain, platform, crash model, ...), and the observed
//! violation itself.
//!
//! The lifecycle: `Open` → (re-run against a fixed implementation)
//! `Resolved` | `Reproduced` | stays `Open`/`NotReproducible`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tpt_concord_core::{ImplementationID, ModelID, SpecificationID};
use tpt_concord_spec::Violation;
use tpt_concord_trace::Trace;

/// Lifecycle status of a counterexample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterexampleStatus {
    /// Recorded, not yet re-checked.
    Open,
    /// Re-run against the (fixed?) implementation and the violation still occurs.
    Reproduced,
    /// Re-run against the implementation and the violation is gone.
    Resolved,
    /// The stored input no longer reaches the violation (e.g. environment moved on).
    NotReproducible,
}

/// Outcome of re-running a counterexample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RecheckOutcome {
    /// The violation still occurs (with its new form).
    ViolationReproduced(Violation),
    /// The input ran cleanly.
    NoViolation,
    /// The input could not be executed (state no longer reachable, etc.).
    NotReproducible(String),
}

/// A smallest-known violation of a specification, as an addressable object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Counterexample {
    /// Stable identifier (e.g. `cx-0007`).
    pub id: String,
    pub spec_ref: SpecificationID,
    pub impl_ref: ImplementationID,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<ModelID>,
    /// The triggering input/state — machine-readable, replayable.
    pub input: serde_json::Value,
    /// Canonical trace up to and including the violation.
    pub trace: Trace,
    /// Environment: platform, toolchain versions, seeds, crash model, ...
    pub environment: BTreeMap<String, String>,
    /// The observed violation.
    pub violation: Violation,
    pub status: CounterexampleStatus,
}

impl Counterexample {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        spec_ref: SpecificationID,
        impl_ref: ImplementationID,
        model_ref: Option<ModelID>,
        input: serde_json::Value,
        trace: Trace,
        environment: BTreeMap<String, String>,
        violation: Violation,
    ) -> Self {
        Self {
            id: id.into(),
            spec_ref,
            impl_ref,
            model_ref,
            input,
            trace,
            environment,
            violation,
            status: CounterexampleStatus::Open,
        }
    }

    /// Serialize to canonical JSON for storage in evidence bundles.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from stored JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Content address of this counterexample (SHA-256 of its canonical JSON).
    pub fn digest(&self) -> Result<String, serde_json::Error> {
        use sha2::{Digest, Sha256};
        let json = self.to_json()?;
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        Ok(hex::encode(hasher.finalize()))
    }

    /// Re-check workflow (§9): run the exact counterexample against the
    /// (possibly fixed) implementation and record the outcome.
    ///
    /// `runner` receives the stored counterexample and replays its input
    /// against the implementation; it reports whether the violation still
    /// occurs. The counterexample's status is updated in place.
    pub fn recheck(
        &mut self,
        runner: impl FnOnce(&Counterexample) -> RecheckOutcome,
    ) -> CounterexampleStatus {
        let outcome = runner(self);
        self.status = match outcome {
            RecheckOutcome::ViolationReproduced(_) => CounterexampleStatus::Reproduced,
            RecheckOutcome::NoViolation => CounterexampleStatus::Resolved,
            RecheckOutcome::NotReproducible(_) => CounterexampleStatus::NotReproducible,
        };
        self.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::StepView;
    use serde_json::json;
    use tpt_concord_spec::{State, Value, ViolationKind};
    use tpt_concord_trace::TraceBuilder;

    fn cx() -> Counterexample {
        let mut b = TraceBuilder::new();
        let mut before = State::new();
        before.insert("committed_len".into(), Value::Int(0));
        let mut after = State::new();
        after.insert("committed_len".into(), Value::Int(-1));
        let step = StepView::new("recover", before, after);
        b.event("step", step.to_event_payload());
        Counterexample::new(
            "cx-0001",
            SpecificationID::new("wal-spec").unwrap(),
            ImplementationID::new("naive-wal").unwrap(),
            Some(ModelID::new("wal-model").unwrap()),
            json!({"inputs": ["append", "crash", "recover"]}),
            b.finish(),
            BTreeMap::from([
                ("platform".to_string(), "windows".to_string()),
                ("crash_model".to_string(), "prefix-crash".to_string()),
            ]),
            Violation::new(
                ViolationKind::Invariant,
                "committed_within_log",
                "committed_len negative",
            ),
        )
    }

    #[test]
    fn serializes_with_all_lifecycle_fields() {
        let c = cx();
        let json = c.to_json().unwrap();
        let back = Counterexample::from_json(&json).unwrap();
        assert_eq!(back, c);
        assert_eq!(back.status, CounterexampleStatus::Open);
        assert_eq!(back.environment["crash_model"], "prefix-crash");
    }

    #[test]
    fn digest_is_stable_and_content_sensitive() {
        let a = cx().digest().unwrap();
        let b = cx().digest().unwrap();
        assert_eq!(a, b);
        let mut c = cx();
        c.id = "cx-0002".into();
        assert_ne!(c.digest().unwrap(), a);
    }

    #[test]
    fn recheck_workflow_transitions_status() {
        let mut c = cx();
        let status = c.recheck(|_| {
            RecheckOutcome::ViolationReproduced(Violation::new(
                ViolationKind::Invariant,
                "committed_within_log",
                "still broken",
            ))
        });
        assert_eq!(status, CounterexampleStatus::Reproduced);

        let status = c.recheck(|_| RecheckOutcome::NoViolation);
        assert_eq!(status, CounterexampleStatus::Resolved);

        let status = c.recheck(|_| RecheckOutcome::NotReproducible("env changed".into()));
        assert_eq!(status, CounterexampleStatus::NotReproducible);
    }
}
