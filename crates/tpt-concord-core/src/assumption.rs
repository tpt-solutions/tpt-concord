// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Assumptions: things accepted without independent proof, recorded so a
//! release can answer "what assumptions does this depend on?".
//!
//! An assumption is never silently converted into evidence. It stays an
//! assumption — a named, inspectable dependency of a claim.

use crate::id::{AssumptionID, ScopeID};
use serde::{Deserialize, Serialize};

/// An explicit precondition accepted without independent proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assumption {
    pub id: AssumptionID,
    /// What is being assumed, stated precisely.
    pub statement: String,
    /// Optionally, the scope in which the assumption holds.
    pub scope: Option<ScopeID>,
}

impl Assumption {
    pub fn new(
        id: impl Into<AssumptionID>,
        statement: impl Into<String>,
        scope: Option<ScopeID>,
    ) -> Self {
        Self {
            id: id.into(),
            statement: statement.into(),
            scope,
        }
    }
}
