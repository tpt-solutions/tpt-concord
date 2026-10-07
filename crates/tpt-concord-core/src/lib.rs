// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Core types for TPT Concord: identifiers, assurance levels, assumptions,
//! scopes, versions, and the Assurance Graph.
//!
//! Everything Concord builds on starts here: first-class machine-readable
//! objects for *what we claim*, *why we claim it*, *under which assumptions*,
//! and *what remains unproven*.

pub mod assumption;
pub mod error;
pub mod graph;
pub mod id;
pub mod level;
pub mod scope;
pub mod version;

pub use assumption::Assumption;
pub use error::Error;
pub use graph::{
    ArtifactRef, AssuranceGraph, Claim, ClaimEvaluation, EvidenceKind, EvidenceRecord,
    EvidenceStatus, Obligation, UnmetObligation, UnmetReason,
};
pub use id::{
    AssumptionID, ClaimID, EvidenceID, ImplementationID, ModelID, ObligationID, ScopeID,
    SpecificationID, TraceID,
};
pub use level::{AssuranceLevel, EvidenceCategory};
pub use scope::Scope;
pub use version::Version;
