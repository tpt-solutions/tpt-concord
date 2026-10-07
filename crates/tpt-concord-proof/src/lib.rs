// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `tpt-concord-proof` — proof-obligation abstraction (spec §5).
//!
//! Adapters for Lean, Telos, Eidos and external proof systems share one
//! contract: they **render** an obligation into a source artifact and
//! **interpret** prover output — but the core never takes an adapter's word
//! for anything. What gets recorded is the artifact itself, its content
//! hash, and the exact command an independent verifier can re-run
//! ([`ProofArtifact`]). A proof claim is only as good as the re-checkable
//! artifact behind it.
//!
//! Paralleling spec §7: an AI (or anyone) may *propose* proof scripts; the
//! pipeline treats them as unvalidated until the independent check passes.

pub mod adapter;
pub mod artifact;

pub use adapter::{
    check_output, EidosAdapter, ExternalAdapter, LeanAdapter, ProofObligation, ProofOutcome,
    ProverAdapter, TelosAdapter,
};
pub use artifact::{ProofArtifact, ProofRegistry};
