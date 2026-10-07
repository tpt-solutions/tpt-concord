// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Specification representation (spec §5, `tpt-concord-spec`).
//!
//! A [`Specification`] is a machine-readable object with preconditions,
//! postconditions, invariants, state transitions, effects, resource bounds
//! and permitted behaviours.

use crate::expr::{Expr, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use tpt_concord_core::{SpecificationID, Version};

/// Expected type of a state variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Int,
    Bool,
    Str,
    Seq(Box<ValueType>),
}

impl ValueType {
    /// Whether `value` conforms to this declared type.
    pub fn matches(&self, value: &Value) -> bool {
        match (self, value) {
            (ValueType::Int, Value::Int(_))
            | (ValueType::Bool, Value::Bool(_))
            | (ValueType::Str, Value::Str(_)) => true,
            (ValueType::Seq(inner), Value::Seq(items)) => items.iter().all(|v| inner.matches(v)),
            _ => false,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ValueType::Int => "int",
            ValueType::Bool => "bool",
            ValueType::Str => "str",
            ValueType::Seq(_) => "seq",
        }
    }
}

/// A side-effect label a transition may produce (e.g. `io`, `network`, `alloc`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Effect(pub String);

impl Effect {
    pub fn new(s: impl Into<String>) -> Self {
        Effect(s.into())
    }
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A named predicate (e.g. an invariant with a name for error reporting).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Named {
    pub name: String,
    pub expr: Expr,
}

impl Named {
    pub fn new(name: impl Into<String>, expr: Expr) -> Self {
        Self {
            name: name.into(),
            expr,
        }
    }
}

/// A named state transition with its own pre/postconditions and permitted effects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub name: String,
    /// Preconditions evaluated on the state before the transition.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pre: Vec<Named>,
    /// Postconditions evaluated on the state after the transition.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub post: Vec<Named>,
    /// Effects this transition may produce.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub effects: BTreeSet<Effect>,
}

impl Transition {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            pre: Vec::new(),
            post: Vec::new(),
            effects: BTreeSet::new(),
        }
    }
}

/// Scope of a resource bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundScope {
    /// Maximum per single operation/transition.
    PerOperation,
    /// Maximum across a whole run.
    PerRun,
}

/// A declared resource bound (e.g. memory, steps, io ops).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceBound {
    /// Name of the measured resource (e.g. `steps`, `allocations`).
    pub resource: String,
    pub max: u64,
    pub scope: BoundScope,
}

/// A behaviour the specification permits under a guarding condition.
///
/// An observed transition is permitted if at least one behaviour's `when`
/// holds in the pre-state and the observed effects are a subset of the
/// behaviour's allowed effects. An empty `effects` set means "effect-unconstrained".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermittedBehaviour {
    pub name: String,
    /// Guard evaluated on the pre-state.
    pub when: Expr,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub effects: BTreeSet<Effect>,
}

/// A complete machine-readable specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Specification {
    pub id: SpecificationID,
    pub version: Version,
    pub name: String,
    pub description: String,
    /// Declared state variables and their expected types.
    #[serde(default)]
    pub state_vars: std::collections::BTreeMap<String, ValueType>,
    /// Global preconditions: must hold before any operation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preconditions: Vec<Named>,
    /// Global postconditions: must hold after any operation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub postconditions: Vec<Named>,
    /// Invariants: must hold in every reachable state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<Named>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<Transition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_bounds: Vec<ResourceBound>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permitted_behaviours: Vec<PermittedBehaviour>,
}

impl Specification {
    pub fn new(
        id: impl Into<SpecificationID>,
        version: impl Into<Version>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            name: name.into(),
            description: String::new(),
            state_vars: Default::default(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
            invariants: Vec::new(),
            transitions: Vec::new(),
            resource_bounds: Vec::new(),
            permitted_behaviours: Vec::new(),
        }
    }

    /// Look up a transition by name.
    pub fn transition(&self, name: &str) -> Option<&Transition> {
        self.transitions.iter().find(|t| t.name == name)
    }
}
