// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `tpt-concord-evidence` — evidence bundles (spec §5).
//!
//! A bundle is the persisted form of everything a claim rests on: proof
//! artifacts, model-check results, traces, simulations, fuzz campaigns, test
//! results, toolchain identities and assumptions — each artifact
//! content-addressed with SHA-256 so any of it can be re-verified later.

pub mod bundle;

pub use bundle::{
    read_and_verify, sha256_hex, verify, Artifact, ArtifactVerification, EvidenceBundle,
    EvidenceBundleBuilder, Section, ToolchainIdentity,
};
pub use tpt_concord_core::EvidenceKind;

/// Errors from building, writing, reading or verifying evidence bundles.
#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("artifact `{path}` hash mismatch: expected {expected}, got {actual}")]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[error("artifact file `{path}` missing from bundle directory")]
    MissingArtifact { path: String },
    #[error("invalid artifact: {0}")]
    InvalidArtifact(String),
}
