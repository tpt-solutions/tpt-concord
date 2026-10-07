// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Checking predicates from a [`Specification`] against states and steps.
//!
//! These are the spec-side primitives the conformance engine
//! (`tpt-concord-check`) builds on. Every check returns structured
//! [`Violation`]s — never panics, never silently passes on evaluation errors.

use crate::expr::{EvalError, State};
use crate::spec::{BoundScope, Effect, Specification};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// What kind of specification obligation was violated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    Invariant,
    GlobalPrecondition,
    GlobalPostcondition,
    TransitionPrecondition,
    TransitionPostcondition,
    Schema,
    Effect,
    Permission,
    ResourceBound,
}

/// A single violation of the specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    pub kind: ViolationKind,
    /// Name of the violated obligation (invariant name, transition name, ...).
    pub name: String,
    /// Human-readable detail, including the failing predicate when applicable.
    pub detail: String,
}

impl Violation {
    pub fn new(kind: ViolationKind, name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} `{}`: {}", self.kind, self.name, self.detail)
    }
}

fn eval_error_detail(context: &str, predicate: &str, err: &EvalError) -> String {
    format!("{context}: predicate `{predicate}` could not be evaluated: {err}")
}

impl Specification {
    /// Check declared state variables against an actual state.
    /// Returns violations for missing, extra, or wrongly-typed variables.
    pub fn check_state_schema(&self, state: &State) -> Vec<Violation> {
        let mut out = Vec::new();
        for (name, expected) in &self.state_vars {
            match state.get(name) {
                None => out.push(Violation::new(
                    ViolationKind::Schema,
                    name.clone(),
                    format!("declared variable `{name}` missing from state"),
                )),
                Some(v) if !expected.matches(v) => out.push(Violation::new(
                    ViolationKind::Schema,
                    name.clone(),
                    format!(
                        "variable `{name}` has type {}, expected {}",
                        v.type_name(),
                        expected.as_str()
                    ),
                )),
                Some(_) => {}
            }
        }
        for name in state.keys() {
            if !self.state_vars.contains_key(name) {
                out.push(Violation::new(
                    ViolationKind::Schema,
                    name.clone(),
                    format!("state contains undeclared variable `{name}`"),
                ));
            }
        }
        out
    }

    /// Check all invariants against one state.
    pub fn check_invariants(&self, state: &State) -> Vec<Violation> {
        self.invariants
            .iter()
            .filter_map(|inv| match inv.expr.eval_bool(state) {
                Ok(true) => None,
                Ok(false) => Some(Violation::new(
                    ViolationKind::Invariant,
                    inv.name.clone(),
                    format!("invariant `{}` false: {}", inv.name, inv.expr),
                )),
                Err(e) => Some(Violation::new(
                    ViolationKind::Invariant,
                    inv.name.clone(),
                    eval_error_detail("invariant", &inv.expr.to_string(), &e),
                )),
            })
            .collect()
    }

    /// Check global preconditions against a state.
    pub fn check_global_preconditions(&self, state: &State) -> Vec<Violation> {
        self.preconditions
            .iter()
            .filter_map(|p| match p.expr.eval_bool(state) {
                Ok(true) => None,
                Ok(false) => Some(Violation::new(
                    ViolationKind::GlobalPrecondition,
                    p.name.clone(),
                    format!("precondition `{}` false: {}", p.name, p.expr),
                )),
                Err(e) => Some(Violation::new(
                    ViolationKind::GlobalPrecondition,
                    p.name.clone(),
                    eval_error_detail("precondition", &p.expr.to_string(), &e),
                )),
            })
            .collect()
    }

    /// Check global postconditions against the post-state.
    pub fn check_global_postconditions(&self, state: &State) -> Vec<Violation> {
        self.postconditions
            .iter()
            .filter_map(|p| match p.expr.eval_bool(state) {
                Ok(true) => None,
                Ok(false) => Some(Violation::new(
                    ViolationKind::GlobalPostcondition,
                    p.name.clone(),
                    format!("postcondition `{}` false: {}", p.name, p.expr),
                )),
                Err(e) => Some(Violation::new(
                    ViolationKind::GlobalPostcondition,
                    p.name.clone(),
                    eval_error_detail("postcondition", &p.expr.to_string(), &e),
                )),
            })
            .collect()
    }

    /// Check a named transition's preconditions against the pre-state.
    /// Unknown transitions yield a violation (not an error): the engine
    /// decides how to classify it.
    pub fn check_transition_pre(&self, name: &str, state: &State) -> Vec<Violation> {
        let Some(t) = self.transition(name) else {
            return vec![Violation::new(
                ViolationKind::Permission,
                name,
                format!("unknown transition `{name}`"),
            )];
        };
        t.pre
            .iter()
            .filter_map(|p| match p.expr.eval_bool(state) {
                Ok(true) => None,
                Ok(false) => Some(Violation::new(
                    ViolationKind::TransitionPrecondition,
                    format!("{}.{}", t.name, p.name),
                    format!("precondition `{}` false: {}", p.name, p.expr),
                )),
                Err(e) => Some(Violation::new(
                    ViolationKind::TransitionPrecondition,
                    format!("{}.{}", t.name, p.name),
                    eval_error_detail("precondition", &p.expr.to_string(), &e),
                )),
            })
            .collect()
    }

    /// Check a named transition's postconditions against the post-state.
    pub fn check_transition_post(&self, name: &str, state: &State) -> Vec<Violation> {
        let Some(t) = self.transition(name) else {
            return vec![Violation::new(
                ViolationKind::Permission,
                name,
                format!("unknown transition `{name}`"),
            )];
        };
        t.post
            .iter()
            .filter_map(|p| match p.expr.eval_bool(state) {
                Ok(true) => None,
                Ok(false) => Some(Violation::new(
                    ViolationKind::TransitionPostcondition,
                    format!("{}.{}", t.name, p.name),
                    format!("postcondition `{}` false: {}", p.name, p.expr),
                )),
                Err(e) => Some(Violation::new(
                    ViolationKind::TransitionPostcondition,
                    format!("{}.{}", t.name, p.name),
                    eval_error_detail("postcondition", &p.expr.to_string(), &e),
                )),
            })
            .collect()
    }

    /// Check that observed effects of a transition are within the declared
    /// effects of that transition.
    pub fn check_effects(&self, name: &str, observed: &BTreeSet<Effect>) -> Vec<Violation> {
        let Some(t) = self.transition(name) else {
            return vec![Violation::new(
                ViolationKind::Permission,
                name,
                format!("unknown transition `{name}`"),
            )];
        };
        let undeclared: Vec<_> = observed.difference(&t.effects).collect();
        if undeclared.is_empty() {
            Vec::new()
        } else {
            let list: Vec<String> = undeclared.iter().map(|e| e.to_string()).collect();
            vec![Violation::new(
                ViolationKind::Effect,
                t.name.clone(),
                format!(
                    "transition `{}` produced undeclared effects: {}",
                    t.name,
                    list.join(", ")
                ),
            )]
        }
    }

    /// Check whether a transition from `pre_state` with `observed` effects is
    /// permitted by the declared permitted behaviours.
    ///
    /// If the specification declares no permitted behaviours, all transitions
    /// known to the spec are considered permitted.
    pub fn check_permission(
        &self,
        name: &str,
        pre_state: &State,
        observed: &BTreeSet<Effect>,
    ) -> Vec<Violation> {
        if self.permitted_behaviours.is_empty() {
            return Vec::new();
        }
        let permitted = self.permitted_behaviours.iter().any(|b| {
            let guard_ok = b.when.eval_bool(pre_state).unwrap_or(false);
            let effects_ok = b.effects.is_empty() || observed.is_subset(&b.effects);
            guard_ok && effects_ok
        });
        if permitted {
            Vec::new()
        } else {
            vec![Violation::new(
                ViolationKind::Permission,
                name,
                format!(
                    "transition `{name}` from this state with effects {observed:?} matches no permitted behaviour"
                ),
            )]
        }
    }

    /// Check an observed resource usage against a declared bound.
    pub fn check_resource_bound(
        &self,
        resource: &str,
        observed: u64,
        scope: BoundScope,
    ) -> Vec<Violation> {
        self.resource_bounds
            .iter()
            .filter(|b| b.resource == resource && b.scope == scope)
            .filter(|b| observed > b.max)
            .map(|b| {
                Violation::new(
                    ViolationKind::ResourceBound,
                    resource,
                    format!(
                        "resource `{resource}` used {observed}, bound is {} ({scope:?})",
                        b.max
                    ),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{Expr, Value};
    use crate::spec::{Named, PermittedBehaviour, Specification, Transition, ValueType};
    use std::collections::BTreeMap;
    use tpt_concord_core::{SpecificationID, Version};

    fn wal_spec() -> Specification {
        let mut spec = Specification::new(
            SpecificationID::new("wal-spec").unwrap(),
            Version::new("1.0.0").unwrap(),
            "Write-ahead log",
        );
        let mut vars = BTreeMap::new();
        vars.insert("committed_len".to_string(), ValueType::Int);
        vars.insert("log".to_string(), ValueType::Seq(Box::new(ValueType::Str)));
        spec.state_vars = vars;
        spec.invariants = vec![Named::new(
            "committed_within_log",
            Expr::le(Expr::var("committed_len"), Expr::len(Expr::var("log"))),
        )];
        let mut commit = Transition::new("commit");
        commit.post = vec![Named::new(
            "commit_advances",
            Expr::gt(Expr::var("committed_len"), Expr::int(-1)),
        )];
        spec.transitions = vec![commit];
        spec
    }

    fn state(committed: i128, log: Vec<Value>) -> State {
        let mut s = State::new();
        s.insert("committed_len".into(), Value::Int(committed));
        s.insert("log".into(), Value::Seq(log));
        s
    }

    #[test]
    fn schema_detects_missing_extra_and_mistyped() {
        let spec = wal_spec();
        assert!(spec.check_state_schema(&state(0, vec![])).is_empty());

        let mut bad = State::new();
        bad.insert("committed_len".into(), Value::Int(1));
        let vios = spec.check_state_schema(&bad);
        assert_eq!(vios.len(), 1); // missing `log`
        assert_eq!(vios[0].kind, ViolationKind::Schema);

        let mut extra = state(0, vec![]);
        extra.insert("junk".into(), Value::Int(0));
        assert_eq!(spec.check_state_schema(&extra).len(), 1);
    }

    #[test]
    fn invariants_pass_and_fail() {
        let spec = wal_spec();
        assert!(spec
            .check_invariants(&state(1, vec![Value::Str("a".into())]))
            .is_empty());
        let vios = spec.check_invariants(&state(2, vec![Value::Str("a".into())]));
        assert_eq!(vios.len(), 1);
        assert_eq!(vios[0].kind, ViolationKind::Invariant);
        assert_eq!(vios[0].name, "committed_within_log");
    }

    #[test]
    fn transition_pre_post_and_unknown() {
        let spec = wal_spec();
        assert!(spec
            .check_transition_pre("commit", &state(0, vec![]))
            .is_empty());
        assert!(spec
            .check_transition_post("commit", &state(0, vec![]))
            .is_empty());
        let vios = spec.check_transition_pre("frobnicate", &state(0, vec![]));
        assert_eq!(vios[0].kind, ViolationKind::Permission);
    }

    #[test]
    fn effects_must_be_declared() {
        let mut spec = wal_spec();
        let mut t = Transition::new("recover");
        t.effects.insert(Effect::new("wal.recover"));
        spec.transitions.push(t);

        let mut ok = BTreeSet::new();
        ok.insert(Effect::new("wal.recover"));
        assert!(spec.check_effects("recover", &ok).is_empty());

        let mut bad = ok;
        bad.insert(Effect::new("network.send"));
        let vios = spec.check_effects("recover", &bad);
        assert_eq!(vios[0].kind, ViolationKind::Effect);
        assert!(vios[0].detail.contains("network.send"));
    }

    #[test]
    fn permitted_behaviours_gate_transitions() {
        let mut spec = wal_spec();
        let mut effects = BTreeSet::new();
        effects.insert(Effect::new("wal.recover"));
        spec.permitted_behaviours = vec![PermittedBehaviour {
            name: "recover-when-crashed".into(),
            when: Expr::var("crashed"),
            effects,
        }];

        let mut state = State::new();
        state.insert("crashed".into(), Value::Bool(false));
        let mut observed = BTreeSet::new();
        observed.insert(Effect::new("wal.recover"));
        let vios = spec.check_permission("recover", &state, &observed);
        assert_eq!(vios[0].kind, ViolationKind::Permission);

        state.insert("crashed".into(), Value::Bool(true));
        assert!(spec
            .check_permission("recover", &state, &observed)
            .is_empty());
    }

    #[test]
    fn resource_bounds() {
        let mut spec = wal_spec();
        spec.resource_bounds.push(crate::spec::ResourceBound {
            resource: "steps".into(),
            max: 100,
            scope: BoundScope::PerRun,
        });
        assert!(spec
            .check_resource_bound("steps", 100, BoundScope::PerRun)
            .is_empty());
        let vios = spec.check_resource_bound("steps", 101, BoundScope::PerRun);
        assert_eq!(vios[0].kind, ViolationKind::ResourceBound);
        assert!(spec
            .check_resource_bound("steps", 101, BoundScope::PerOperation)
            .is_empty());
    }
}
