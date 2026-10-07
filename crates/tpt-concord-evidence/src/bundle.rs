// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! The evidence bundle format: manifest + content-addressed artifacts.

use crate::BundleError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use tpt_concord_core::{Assumption, EvidenceID, EvidenceKind};

/// Identity of the toolchain (compiler, prover, fuzzer, ...) that produced
/// evidence — recorded so results can be reproduced bit-for-bit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainIdentity {
    /// e.g. `rustc`, `lean`, `telos`.
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, String>,
}

impl ToolchainIdentity {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            commit: None,
            details: BTreeMap::new(),
        }
    }
}

/// One stored artifact with its content hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: EvidenceID,
    pub kind: EvidenceKind,
    /// Path relative to the bundle root, e.g. `artifacts/wal-trace.json`.
    pub path: String,
    /// SHA-256 hex of the artifact contents.
    pub sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// A stored evidence artifact's in-memory content (not serialized).
#[derive(Debug)]
struct Content {
    path: String,
    bytes: Vec<u8>,
}

/// The bundle manifest. Sections mirror spec §5 `tpt-concord-evidence`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceBundle {
    pub bundle_id: String,
    /// Unix timestamp of bundle creation.
    pub created_unix: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub toolchains: Vec<ToolchainIdentity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<Assumption>,
    /// Formal proof artifacts (Lean/Telos/Eidos/...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proofs: Vec<Artifact>,
    /// Model-checking results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_checks: Vec<Artifact>,
    /// Canonical traces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub traces: Vec<Artifact>,
    /// Deterministic simulation runs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub simulations: Vec<Artifact>,
    /// Fuzz campaign results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fuzz_campaigns: Vec<Artifact>,
    /// Property-testing campaign results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub property_campaigns: Vec<Artifact>,
    /// Test-suite results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test_results: Vec<Artifact>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl EvidenceBundle {
    pub fn new(bundle_id: impl Into<String>) -> Self {
        Self {
            bundle_id: bundle_id.into(),
            created_unix: now_unix(),
            toolchains: Vec::new(),
            assumptions: Vec::new(),
            proofs: Vec::new(),
            model_checks: Vec::new(),
            traces: Vec::new(),
            simulations: Vec::new(),
            fuzz_campaigns: Vec::new(),
            property_campaigns: Vec::new(),
            test_results: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    /// All artifacts across every section.
    pub fn artifacts(&self) -> impl Iterator<Item = &Artifact> {
        [
            &self.proofs,
            &self.model_checks,
            &self.traces,
            &self.simulations,
            &self.fuzz_campaigns,
            &self.property_campaigns,
            &self.test_results,
        ]
        .into_iter()
        .flatten()
    }

    /// Serialize the manifest to canonical JSON.
    pub fn to_json(&self) -> Result<String, BundleError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parse a manifest.
    pub fn from_json(json: &str) -> Result<Self, BundleError> {
        Ok(serde_json::from_str(json)?)
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Incremental builder that also holds artifact content in memory until the
/// bundle is written to disk.
#[derive(Debug)]
pub struct EvidenceBundleBuilder {
    bundle: EvidenceBundle,
    contents: Vec<Content>,
}

impl EvidenceBundleBuilder {
    pub fn new(bundle_id: impl Into<String>) -> Self {
        Self {
            bundle: EvidenceBundle::new(bundle_id),
            contents: Vec::new(),
        }
    }

    pub fn toolchain(mut self, t: ToolchainIdentity) -> Self {
        self.bundle.toolchains.push(t);
        self
    }

    pub fn assumption(mut self, a: Assumption) -> Self {
        self.bundle.assumptions.push(a);
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.bundle.metadata.insert(key.into(), value.into());
        self
    }

    /// Add an artifact with its content. `section` selects where it is
    /// recorded in the manifest.
    pub fn artifact(
        mut self,
        section: Section,
        id: impl Into<EvidenceID>,
        kind: EvidenceKind,
        filename: &str,
        description: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<Self, BundleError> {
        if filename.contains('\\') || filename.contains('/') {
            return Err(BundleError::InvalidArtifact(format!(
                "artifact filename must be a bare name, got {filename:?}"
            )));
        }
        let path = format!("artifacts/{filename}");
        let artifact = Artifact {
            id: id.into(),
            kind,
            path: path.clone(),
            sha256: sha256_hex(&bytes),
            description: description.into(),
        };
        match section {
            Section::Proofs => self.bundle.proofs.push(artifact),
            Section::ModelChecks => self.bundle.model_checks.push(artifact),
            Section::Traces => self.bundle.traces.push(artifact),
            Section::Simulations => self.bundle.simulations.push(artifact),
            Section::FuzzCampaigns => self.bundle.fuzz_campaigns.push(artifact),
            Section::PropertyCampaigns => self.bundle.property_campaigns.push(artifact),
            Section::TestResults => self.bundle.test_results.push(artifact),
        }
        self.contents.push(Content { path, bytes });
        Ok(self)
    }

    /// The manifest so far (without writing anything).
    pub fn manifest(&self) -> &EvidenceBundle {
        &self.bundle
    }

    /// Write `manifest.json` and all artifact files under `dir`.
    pub fn write(self, dir: impl AsRef<Path>) -> Result<EvidenceBundle, BundleError> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir.join("artifacts"))?;
        for content in &self.contents {
            fs::write(dir.join(&content.path), &content.bytes)?;
        }
        fs::write(dir.join("manifest.json"), self.bundle.to_json()?.as_bytes())?;
        Ok(self.bundle)
    }
}

/// Manifest section an artifact belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Proofs,
    ModelChecks,
    Traces,
    Simulations,
    FuzzCampaigns,
    PropertyCampaigns,
    TestResults,
}

/// Verification result for one artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactVerification {
    pub path: String,
    pub expected_sha256: String,
    pub actual_sha256: String,
    pub ok: bool,
}

/// Read a bundle directory, verify every artifact hash, and return the
/// manifest. Fails on the first mismatch or missing file.
pub fn read_and_verify(dir: impl AsRef<Path>) -> Result<EvidenceBundle, BundleError> {
    let dir = dir.as_ref();
    let manifest_bytes = fs::read_to_string(dir.join("manifest.json"))?;
    let bundle = EvidenceBundle::from_json(&manifest_bytes)?;
    verify(dir, &bundle)?
        .into_iter()
        .find(|v| !v.ok)
        .map_or(Ok(bundle), |v| {
            Err(BundleError::HashMismatch {
                path: v.path,
                expected: v.expected_sha256,
                actual: v.actual_sha256,
            })
        })
}

/// Verify all artifact hashes of a manifest against files in `dir`.
pub fn verify(
    dir: impl AsRef<Path>,
    bundle: &EvidenceBundle,
) -> Result<Vec<ArtifactVerification>, BundleError> {
    let dir = dir.as_ref();
    let mut out = Vec::new();
    for artifact in bundle.artifacts() {
        let path = dir.join(&artifact.path);
        let bytes = fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => BundleError::MissingArtifact {
                path: artifact.path.clone(),
            },
            _ => BundleError::Io(e),
        })?;
        let actual = sha256_hex(&bytes);
        out.push(ArtifactVerification {
            path: artifact.path.clone(),
            expected_sha256: artifact.sha256.clone(),
            ok: actual == artifact.sha256,
            actual_sha256: actual,
        });
    }
    Ok(out)
}

/// SHA-256 of bytes, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tpt_concord_core::{Assumption, AssumptionID};

    fn build() -> EvidenceBundleBuilder {
        EvidenceBundleBuilder::new("bundle-2026-10-05")
            .toolchain(ToolchainIdentity::new("rustc", "1.97.1"))
            .assumption(Assumption::new(
                AssumptionID::new("disk-prefix-survives").unwrap(),
                "prefix crash model assumption",
                None,
            ))
            .metadata("subject", "archon-wal")
    }

    #[test]
    fn manifest_roundtrip() {
        let built = build()
            .artifact(
                Section::Traces,
                EvidenceID::new("trace-1").unwrap(),
                EvidenceKind::ConformanceTrace,
                "trace.json",
                "conformance run",
                serde_json::to_vec(&json!({"events": []})).unwrap(),
            )
            .unwrap();
        let manifest = built.manifest().clone();
        let json = manifest.to_json().unwrap();
        let back = EvidenceBundle::from_json(&json).unwrap();
        assert_eq!(back, manifest);
        assert_eq!(back.traces.len(), 1);
        assert_eq!(back.traces[0].path, "artifacts/trace.json");
    }

    #[test]
    fn write_read_verify_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("concord-test-{}", std::process::id()));
        let bundle_dir = tmp.join("bundle");
        let _ = fs::remove_dir_all(&bundle_dir);

        let bundle = build()
            .artifact(
                Section::Proofs,
                EvidenceID::new("lean-1").unwrap(),
                EvidenceKind::LeanProof,
                "theorem.lean",
                "recovery theorem",
                b"theorem recovery_preserves_committed : True := trivial".to_vec(),
            )
            .unwrap()
            .artifact(
                Section::TestResults,
                EvidenceID::new("tests-1").unwrap(),
                EvidenceKind::TestRun,
                "results.json",
                "cargo test output",
                br#"{"passed": 18}"#.to_vec(),
            )
            .unwrap()
            .write(&bundle_dir)
            .unwrap();

        assert_eq!(bundle.proofs.len(), 1);
        assert_eq!(bundle.test_results.len(), 1);

        let read_back = read_and_verify(&bundle_dir).unwrap();
        assert_eq!(read_back, bundle);

        let report = verify(&bundle_dir, &bundle).unwrap();
        assert!(report.iter().all(|v| v.ok));

        // Tamper: rewrite an artifact in place; verification must fail.
        fs::write(
            bundle_dir.join("artifacts/results.json"),
            br#"{"passed": 19}"#,
        )
        .unwrap();
        assert!(matches!(
            read_and_verify(&bundle_dir),
            Err(BundleError::HashMismatch { path, .. }) if path == "artifacts/results.json"
        ));

        // Missing artifact is detected.
        fs::remove_file(bundle_dir.join("artifacts/results.json")).unwrap();
        assert!(matches!(
            read_and_verify(&bundle_dir),
            Err(BundleError::MissingArtifact { path }) if path == "artifacts/results.json"
        ));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_path_traversal_filenames() {
        let res = build().artifact(
            Section::Traces,
            EvidenceID::new("t").unwrap(),
            EvidenceKind::ConformanceTrace,
            "../escape.json",
            "bad",
            vec![],
        );
        assert!(matches!(res, Err(BundleError::InvalidArtifact(_))));
    }

    #[test]
    fn artifacts_iterates_all_sections() {
        let b = build()
            .artifact(
                Section::Proofs,
                EvidenceID::new("p").unwrap(),
                EvidenceKind::LeanProof,
                "p.txt",
                "",
                vec![1],
            )
            .unwrap()
            .artifact(
                Section::FuzzCampaigns,
                EvidenceID::new("f").unwrap(),
                EvidenceKind::FuzzCampaign,
                "f.txt",
                "",
                vec![2],
            )
            .unwrap();
        assert_eq!(b.manifest().artifacts().count(), 2);
    }
}
