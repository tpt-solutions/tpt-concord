// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Step-level conformance: check an implementation run against a
//! [`Specification`].
//!
//! An implementation run is presented as a sequence of [`StepView`]s — the
//! transition name, pre-state, post-state and observed effects, mapped from
//! whatever native representation the implementation uses. Every obligation
//! of the spec is checked at every applicable point; the result is a
//! [`ConformanceReport`] of structured violations, never a boolean.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::collections::{BTreeMap, BTreeSet};
use tpt_concord_spec::{Effect, Specification, State, Violation};
use tpt_concord_trace::event::Event;

/// One observed implementation step, mapped into the spec's state space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepView {
    /// Name of the transition taken (must be declared in the spec).
    pub name: String,
    /// State before the step, in the spec's state space.
    pub before: State,
    /// State after the step, in the spec's state space.
    pub after: State,
    /// Effects observed during the step.
    pub effects: BTreeSet<Effect>,
    /// Per-operation resource usage observed during the step.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resources: BTreeMap<String, u64>,
}

impl StepView {
    pub fn new(name: impl Into<String>, before: State, after: State) -> Self {
        Self {
            name: name.into(),
            before,
            after,
            effects: BTreeSet::new(),
            resources: BTreeMap::new(),
        }
    }

    /// Canonical event payload for recording this step in a
    /// `tpt-concord-trace` trace. [`from_event_payload`] inverts this.
    pub fn to_event_payload(&self) -> Json {
        serde_json::json!({
            "name": self.name,
            "before": self.before,
            "after": self.after,
            "effects": self.effects.iter().map(|e| e.0.clone()).collect::<Vec<_>>(),
            "resources": self.resources,
        })
    }

    /// Rebuild a step from a payload produced by [`to_event_payload`].
    pub fn from_event_payload(payload: &Json) -> Result<Self, crate::CheckError> {
        let obj = payload.as_object().ok_or_else(|| {
            crate::CheckError::MalformedEventPayload("payload is not an object".into())
        })?;
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::CheckError::MalformedEventPayload("missing `name`".into()))?
            .to_string();
        let before: State =
            serde_json::from_value(obj.get("before").cloned().ok_or_else(|| {
                crate::CheckError::MalformedEventPayload("missing `before`".into())
            })?)
            .map_err(|e| crate::CheckError::MalformedEventPayload(format!("bad `before`: {e}")))?;
        let after: State =
            serde_json::from_value(obj.get("after").cloned().ok_or_else(|| {
                crate::CheckError::MalformedEventPayload("missing `after`".into())
            })?)
            .map_err(|e| crate::CheckError::MalformedEventPayload(format!("bad `after`: {e}")))?;
        let effects: BTreeSet<Effect> = match obj.get("effects") {
            Some(Json::Array(items)) => items
                .iter()
                .filter_map(|v| v.as_str().map(|s| Effect(s.to_string())))
                .collect(),
            _ => BTreeSet::new(),
        };
        let resources: BTreeMap<String, u64> = obj
            .get("resources")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        Ok(Self {
            name,
            before,
            after,
            effects,
            resources,
        })
    }
}

/// Outcome of checking a run against a specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceReport {
    /// Number of steps checked.
    pub checked_steps: usize,
    /// All violations, in check order.
    pub violations: Vec<StepViolation>,
    /// True iff no violations.
    pub conforms: bool,
}

/// A violation located at a specific step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepViolation {
    /// Index of the offending step in the run.
    pub step: usize,
    #[serde(flatten)]
    pub violation: Violation,
}

/// Check schema + invariants of individual states (no transitions).
pub fn check_states(spec: &Specification, states: &[State]) -> Vec<Violation> {
    let mut out = Vec::new();
    for state in states {
        out.extend(spec.check_state_schema(state));
        out.extend(spec.check_invariants(state));
    }
    out
}

/// Check a run of implementation steps against the full specification:
/// schema, global pre/postconditions, invariants, transition pre/post,
/// declared effects, permitted behaviours, resource bounds, and state
/// continuity between consecutive steps.
pub fn check_steps(spec: &Specification, steps: &[StepView]) -> ConformanceReport {
    let mut violations = Vec::new();
    // A spec with no declared transitions constrains only states (schema,
    // invariants, global pre/post); step names cannot be checked against
    // anything and are not violations.
    let check_transitions = !spec.transitions.is_empty();
    for (i, step) in steps.iter().enumerate() {
        // State continuity: the chain of states must be unbroken.
        if i > 0 && steps[i - 1].after != step.before {
            violations.push(StepViolation {
                step: i,
                violation: Violation::new(
                    tpt_concord_spec::ViolationKind::Schema,
                    "continuity",
                    format!(
                        "state discontinuity at step {i}: previous `after` does not match `before`"
                    ),
                ),
            });
        }
        push_all(&mut violations, i, spec.check_state_schema(&step.before));
        push_all(
            &mut violations,
            i,
            spec.check_global_preconditions(&step.before),
        );
        push_all(&mut violations, i, spec.check_invariants(&step.before));
        if check_transitions {
            push_all(
                &mut violations,
                i,
                spec.check_transition_pre(&step.name, &step.before),
            );
            push_all(
                &mut violations,
                i,
                spec.check_transition_post(&step.name, &step.after),
            );
            push_all(
                &mut violations,
                i,
                spec.check_effects(&step.name, &step.effects),
            );
            push_all(
                &mut violations,
                i,
                spec.check_permission(&step.name, &step.before, &step.effects),
            );
        }
        push_all(
            &mut violations,
            i,
            spec.check_global_postconditions(&step.after),
        );
        push_all(&mut violations, i, spec.check_invariants(&step.after));
        for (resource, used) in &step.resources {
            violations.extend(
                spec.check_resource_bound(
                    resource,
                    *used,
                    tpt_concord_spec::BoundScope::PerOperation,
                )
                .into_iter()
                .map(|v| StepViolation {
                    step: i,
                    violation: v,
                }),
            );
        }
    }
    // Final-state checks are covered per-step above (each `after` is checked);
    // if there are no steps there is nothing to check.
    ConformanceReport {
        checked_steps: steps.len(),
        conforms: violations.is_empty(),
        violations,
    }
}

fn push_all(out: &mut Vec<StepViolation>, step: usize, violations: Vec<Violation>) {
    out.extend(
        violations
            .into_iter()
            .map(|v| StepViolation { step, violation: v }),
    );
}

/// Convenience accessors on the report.
impl ConformanceReport {
    /// First violation, if any (useful for counterexample generation).
    pub fn first_violation(&self) -> Option<&StepViolation> {
        self.violations.first()
    }
}

/// Check a run recorded as a canonical trace, extracting [`StepView`]s from
/// each event payload.
pub fn check_trace(
    spec: &Specification,
    trace: &tpt_concord_trace::Trace,
) -> Result<ConformanceReport, crate::CheckError> {
    let mut steps = Vec::with_capacity(trace.events.len());
    for event in &trace.events {
        steps.push(StepView::from_event_payload(&event.payload)?);
    }
    Ok(check_steps(spec, &steps))
}

/// Build a trace event for a step (kind `"step"`).
pub fn step_to_event(
    seq: u64,
    parent: Option<tpt_concord_trace::EventID>,
    step: &StepView,
) -> Event {
    Event::new(seq, parent, "step", step.to_event_payload())
}
