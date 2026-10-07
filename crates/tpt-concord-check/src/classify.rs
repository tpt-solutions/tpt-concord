// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Deviation classification: when conformance fails, *what kind* of
//! disagreement is it? Classification keeps diagnostics honest — "the
//! implementation did something the model forbids" is different from "the
//! data disagree" — and feeds counterexample metadata.

use serde::{Deserialize, Serialize};
use tpt_concord_spec::{Violation, ViolationKind};
use tpt_concord_trace::divergence::{Divergence, DivergenceKind};
use tpt_concord_trace::Trace;

/// The kind of observed deviation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviationClass {
    /// The reference trace has an event the candidate trace lacks.
    MissingEvent,
    /// The candidate produced something the reference does not.
    UnexpectedEvent,
    /// Same event, different values.
    ValueMismatch,
    /// Causal ordering disagrees.
    OrderingViolation,
    /// Implementation state jumped where no transition connects.
    StateDiscontinuity,
    InvariantViolation,
    PreconditionViolation,
    PostconditionViolation,
    EffectViolation,
    PermissionViolation,
    ResourceBoundViolation,
    SchemaViolation,
}

impl DeviationClass {
    /// Short human-readable explanation.
    pub fn describe(&self) -> &'static str {
        match self {
            DeviationClass::MissingEvent => {
                "candidate is missing an event present in the reference"
            }
            DeviationClass::UnexpectedEvent => "candidate produced an unexpected event",
            DeviationClass::ValueMismatch => "same event kind, different values",
            DeviationClass::OrderingViolation => "causal ordering disagrees",
            DeviationClass::StateDiscontinuity => "state sequence is discontinuous",
            DeviationClass::InvariantViolation => "an invariant was violated",
            DeviationClass::PreconditionViolation => "a precondition was violated",
            DeviationClass::PostconditionViolation => "a postcondition was violated",
            DeviationClass::EffectViolation => "an undeclared effect was produced",
            DeviationClass::PermissionViolation => {
                "a transition was taken that no permitted behaviour allows"
            }
            DeviationClass::ResourceBoundViolation => "a resource bound was exceeded",
            DeviationClass::SchemaViolation => "state does not match the declared schema",
        }
    }
}

/// Classify a trace divergence, given both traces for context.
///
/// `reference` is the model's trace, `candidate` the implementation's.
pub fn classify_divergence(reference: &Trace, candidate: &Trace, d: &Divergence) -> DeviationClass {
    match &d.kind {
        DivergenceKind::LengthMismatch { .. } => {
            if reference.events.len() > candidate.events.len() {
                DeviationClass::MissingEvent
            } else {
                DeviationClass::UnexpectedEvent
            }
        }
        DivergenceKind::KindMismatch { .. } => DeviationClass::UnexpectedEvent,
        DivergenceKind::PayloadMismatch { .. } => DeviationClass::ValueMismatch,
        DivergenceKind::ParentMismatch { .. } => DeviationClass::OrderingViolation,
    }
}

/// Classify a spec violation.
pub fn classify_violation(v: &Violation) -> DeviationClass {
    match v.kind {
        ViolationKind::Invariant => DeviationClass::InvariantViolation,
        ViolationKind::GlobalPrecondition | ViolationKind::TransitionPrecondition => {
            DeviationClass::PreconditionViolation
        }
        ViolationKind::GlobalPostcondition | ViolationKind::TransitionPostcondition => {
            DeviationClass::PostconditionViolation
        }
        ViolationKind::Effect => DeviationClass::EffectViolation,
        ViolationKind::Permission => DeviationClass::PermissionViolation,
        ViolationKind::ResourceBound => DeviationClass::ResourceBoundViolation,
        ViolationKind::Schema => DeviationClass::SchemaViolation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::StepView;
    use serde_json::json;
    use tpt_concord_spec::State;
    use tpt_concord_trace::TraceBuilder;

    #[test]
    fn longer_reference_is_missing_event() {
        let mut rb = TraceBuilder::new();
        rb.event("a", json!({}));
        rb.event("b", json!({}));
        let reference = rb.finish();
        let mut cb = TraceBuilder::new();
        cb.event("a", json!({}));
        let candidate = cb.finish();

        let divs = tpt_concord_trace::divergence::compare(&reference, &candidate);
        assert_eq!(
            classify_divergence(&reference, &candidate, &divs[0]),
            DeviationClass::MissingEvent
        );
    }

    #[test]
    fn longer_candidate_is_unexpected_event() {
        let mut rb = TraceBuilder::new();
        rb.event("a", json!({}));
        let reference = rb.finish();
        let mut cb = TraceBuilder::new();
        cb.event("a", json!({}));
        cb.event("b", json!({}));
        let candidate = cb.finish();

        let divs = tpt_concord_trace::divergence::compare(&reference, &candidate);
        assert_eq!(
            classify_divergence(&reference, &candidate, &divs[0]),
            DeviationClass::UnexpectedEvent
        );
    }

    #[test]
    fn violation_classification_covers_all_kinds() {
        for (kind, expected) in [
            (ViolationKind::Invariant, DeviationClass::InvariantViolation),
            (ViolationKind::Effect, DeviationClass::EffectViolation),
            (
                ViolationKind::Permission,
                DeviationClass::PermissionViolation,
            ),
            (ViolationKind::Schema, DeviationClass::SchemaViolation),
            (
                ViolationKind::ResourceBound,
                DeviationClass::ResourceBoundViolation,
            ),
        ] {
            let v = Violation::new(kind, "x", "detail");
            assert_eq!(classify_violation(&v), expected);
        }
    }

    #[test]
    fn step_views_roundtrip_through_payload() {
        let mut s = State::new();
        s.insert("x".to_string(), tpt_concord_spec::Value::Int(1));
        let mut step = StepView::new("t", s.clone(), s);
        step.effects.insert(tpt_concord_spec::Effect::new("e"));
        step.resources.insert("steps".into(), 2);
        let payload = step.to_event_payload();
        let back = StepView::from_event_payload(&payload).unwrap();
        assert_eq!(back, step);
    }
}
