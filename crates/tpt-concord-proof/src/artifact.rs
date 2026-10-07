// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Independently checkable proof artifacts and the registry that records
//! them.
//!
//! The core never trusts an adapter's word (spec §5): a proof claim is
//! recorded as the artifact itself — content-hashed — plus the adapter and
//! the exact re-check command. Anyone can later verify the hash and re-run
//! the command; the recorded [`ProofOutcome`] is an annotation, not the
//! source of truth.

use crate::adapter::{ProofOutcome, ProverAdapter};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tpt_concord_core::ObligationID;

/// A recorded proof artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofArtifact {
    /// The obligation this artifact discharges.
    pub obligation: ObligationID,
    /// Adapter that produced/interprets it (e.g. `lean4`).
    pub adapter: String,
    /// Path of the rendered source artifact (relative to an evidence bundle).
    pub source_path: String,
    /// SHA-256 of the source artifact at record time.
    pub source_sha256: String,
    /// The command that independently re-checks the artifact.
    pub recheck_command: String,
    /// The adapter's interpretation of the prover run — an annotation; the
    /// artifact + recheck command are the ground truth.
    pub recorded_outcome: ProofOutcome,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl ProofArtifact {
    /// Record an artifact for `obligation` rendered by `adapter`.
    ///
    /// `source` is the rendered artifact content; it is hashed here so later
    /// tampering is detectable via [`Self::verify_source`].
    pub fn record(
        obligation: &crate::adapter::ProofObligation,
        adapter: &impl ProverAdapter,
        source_path: impl Into<String>,
        source: &str,
        outcome: ProofOutcome,
    ) -> Self {
        let path: String = source_path.into();
        Self {
            obligation: obligation.id.clone(),
            adapter: adapter.name().to_string(),
            source_sha256: sha256_hex(source.as_bytes()),
            recheck_command: adapter.check_command(&path),
            source_path: path,
            recorded_outcome: outcome,
            metadata: BTreeMap::new(),
        }
    }

    /// Independently verify the stored artifact content against the recorded
    /// hash.
    pub fn verify_source(&self, source: &str) -> bool {
        sha256_hex(source.as_bytes()) == self.source_sha256
    }

    /// Whether this artifact claims a *complete* proof — i.e. the recorded
    /// outcome is `Proven`. Anything else (unvalidated, inconclusive,
    /// failed) is not. Gates should treat `Proven` here as an assertion to
    /// be re-verified, never as ground truth.
    pub fn claims_proven(&self) -> bool {
        matches!(self.recorded_outcome, ProofOutcome::Proven)
    }
}

/// Registry of proof artifacts, serializable into evidence bundles.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofRegistry {
    pub artifacts: Vec<ProofArtifact>,
}

impl ProofRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, artifact: ProofArtifact) {
        self.artifacts.push(artifact);
    }

    /// Artifacts recorded for one obligation.
    pub fn for_obligation(&self, obligation: &ObligationID) -> Vec<&ProofArtifact> {
        self.artifacts
            .iter()
            .filter(|a| &a.obligation == obligation)
            .collect()
    }

    /// True iff every artifact claiming `Proven` for the given obligations
    /// also passes its source-hash verification against `sources` (path ->
    /// content).
    pub fn proven_and_intact(&self, sources: &BTreeMap<String, String>) -> bool {
        self.artifacts.iter().all(|a| {
            !a.claims_proven()
                || sources
                    .get(&a.source_path)
                    .is_some_and(|src| a.verify_source(src))
        })
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{LeanAdapter, TelosAdapter};

    fn obligation() -> crate::adapter::ProofObligation {
        crate::adapter::ProofObligation::new(
            ObligationID::new("obligation.recovery").unwrap(),
            "recovery preserves the committed prefix",
        )
    }

    #[test]
    fn record_hashes_source_and_commands_recheck() {
        let ob = obligation();
        let source = LeanAdapter.render(&ob);
        let artifact = ProofArtifact::record(
            &ob,
            &LeanAdapter,
            "artifacts/recovery.lean",
            &source,
            LeanAdapter.parse_result("warning: declaration uses 'sorry'"),
        );
        assert_eq!(artifact.adapter, "lean4");
        assert_eq!(
            artifact.recheck_command,
            "lake env lean artifacts/recovery.lean"
        );
        assert!(artifact.verify_source(&source));
        assert!(!artifact.verify_source("-- tampered"));
        // A `sorry` artifact does not claim proven.
        assert!(!artifact.claims_proven());
    }

    #[test]
    fn registry_roundtrips_through_json() {
        let mut registry = ProofRegistry::new();
        let ob = obligation();
        let source = TelosAdapter.render(&ob);
        registry.record(ProofArtifact::record(
            &ob,
            &TelosAdapter,
            "artifacts/recovery.telos",
            &source,
            ProofOutcome::Proven,
        ));
        let json = registry.to_json().unwrap();
        let back = ProofRegistry::from_json(&json).unwrap();
        assert_eq!(back, registry);
        assert_eq!(back.for_obligation(&obligation().id).len(), 1);
    }

    #[test]
    fn proven_and_intact_detects_tampering() {
        let mut registry = ProofRegistry::new();
        let ob = obligation();
        let source = TelosAdapter.render(&ob);
        let path = "artifacts/recovery.telos".to_string();
        registry.record(ProofArtifact::record(
            &ob,
            &TelosAdapter,
            path.clone(),
            &source,
            ProofOutcome::Proven,
        ));

        let mut sources = BTreeMap::new();
        sources.insert(path.clone(), source.clone());
        assert!(registry.proven_and_intact(&sources));

        // Tampered source: the "proven" claim is no longer intact.
        sources.insert(path.clone(), "(claim (hacked))".to_string());
        assert!(!registry.proven_and_intact(&sources));

        // Missing source file: not intact.
        assert!(!registry.proven_and_intact(&BTreeMap::new()));

        // An inconclusive artifact doesn't claim proof, so it passes.
        let mut inconclusive = ProofRegistry::new();
        inconclusive.record(ProofArtifact::record(
            &ob,
            &LeanAdapter,
            path,
            "-- sorry",
            ProofOutcome::Inconclusive {
                detail: "sorry".into(),
            },
        ));
        assert!(inconclusive.proven_and_intact(&BTreeMap::new()));
    }
}
