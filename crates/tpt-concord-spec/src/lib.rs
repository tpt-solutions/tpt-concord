// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `tpt-concord-spec` — specification representation.
//!
//! Provides machine-readable specifications with preconditions,
//! postconditions, invariants, state transitions, effects, resource bounds
//! and permitted behaviours, over a small serializable predicate language.

pub mod checks;
pub mod expr;
pub mod spec;

pub use checks::{Violation, ViolationKind};
pub use expr::{EvalError, Expr, State, Value};
pub use spec::{
    BoundScope, Effect, Named, PermittedBehaviour, ResourceBound, Specification, Transition,
    ValueType,
};
