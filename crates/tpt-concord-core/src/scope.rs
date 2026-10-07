// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Scopes: explicit boundaries within which claims are meant to hold.
//!
//! Example: the Archon WAL claim holds "under crash model X" — that crash
//! model is a scope. Claims that silently drop their scope are exactly the
//! over-claims Concord exists to prevent.

use crate::id::ScopeID;
use serde::{Deserialize, Serialize};

/// A named boundary (environment, crash model, deployment target, ...) that
/// qualifies where a claim or assumption applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub id: ScopeID,
    /// Human-readable description of the boundary.
    pub description: String,
}

impl Scope {
    pub fn new(id: impl Into<ScopeID>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
        }
    }
}
