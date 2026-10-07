// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Error type shared across `tpt-concord-core`.

use crate::id::{AssumptionID, ClaimID, EvidenceID, ObligationID, ScopeID};
use thiserror::Error as ThisError;

/// Errors from building and validating assurance structures.
#[derive(Debug, ThisError)]
pub enum Error {
    #[error("invalid identifier {0:?}: {1}")]
    InvalidId(String, String),

    #[error("duplicate {kind} `{id}`")]
    Duplicate { kind: &'static str, id: String },

    #[error("unknown {kind} `{id}`")]
    Unknown { kind: &'static str, id: String },

    #[error("claim `{0}` cannot depend on itself")]
    SelfDependency(ClaimID),

    #[error("assurance graph contains a claim dependency cycle involving `{0}`")]
    Cycle(ClaimID),

    #[error("scope `{0}` is not defined in this graph")]
    UnknownScope(ScopeID),

    #[error("assumption `{0}` is not defined in this graph")]
    UnknownAssumption(AssumptionID),

    #[error("obligation `{0}` is not required by any claim")]
    OrphanObligation(ObligationID),

    #[error("evidence `{0}` does not discharge any obligation")]
    OrphanEvidence(EvidenceID),
}
