// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! The AI acceptance pipeline (spec §7): AI is an untrusted proposer, and
//! every artifact it generates must walk the ordered acceptance path before
//! anything is trusted.
//!
//! ```text
//! AI proposal
//!     ↓  (compiler / type system)
//! specification obligations
//!     ↓  (core: AssuranceGraph)
//! formal proof / model checking
//!     ↓  (proof adapters, model exploration)
//! implementation conformance
//!     ↓  (check crate)
//! property testing → fuzzing
//!     ↓  (property, fuzz crates)
//! evidence validation
//!     ↓  (evidence bundle re-verification)
//! release gate
//! ```
//!
//! [`AcceptanceTrace`] records each stage's outcome as a first-class,
//! serializable object, so a rejected proposal carries its exact rejection
//! point — and nothing past that point is claimed.

use serde::{Deserialize, Serialize};

/// The ordered pipeline stages (spec §7), each wired to the Concord
/// capability that enforces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// The AI-generated artifact as received. Trusted for nothing.
    AiProposal,
    /// The artifact must compile / type-check (`cargo check`, etc.).
    CompilerTypeSystem,
    /// Obligations derived from the specification are registered in the
    /// AssuranceGraph (`tpt-concord-core`).
    SpecificationObligations,
    /// Formal proof or model checking discharges proof obligations
    /// (`tpt-concord-proof`, `tpt-concord-sim` exploration, model checks).
    FormalProofOrModelChecking,
    /// The implementation conforms to the executable model on canonical
    /// traces (`tpt-concord-check`).
    ImplementationConformance,
    /// Property-based testing derived from the specification
    /// (`tpt-concord-property`).
    PropertyTesting,
    /// Specification-driven fuzzing (`tpt-concord-fuzz`).
    Fuzzing,
    /// Evidence artifacts are re-verified (hashes, replays, re-runs)
    /// (`tpt-concord-evidence`).
    EvidenceValidation,
    /// The release gate evaluates policy over validated evidence
    /// (`tpt-concord-release`).
    ReleaseGate,
}

impl Stage {
    /// All stages, in acceptance order.
    pub const ORDERED: [Stage; 9] = [
        Stage::AiProposal,
        Stage::CompilerTypeSystem,
        Stage::SpecificationObligations,
        Stage::FormalProofOrModelChecking,
        Stage::ImplementationConformance,
        Stage::PropertyTesting,
        Stage::Fuzzing,
        Stage::EvidenceValidation,
        Stage::ReleaseGate,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Stage::AiProposal => "ai_proposal",
            Stage::CompilerTypeSystem => "compiler_type_system",
            Stage::SpecificationObligations => "specification_obligations",
            Stage::FormalProofOrModelChecking => "formal_proof_or_model_checking",
            Stage::ImplementationConformance => "implementation_conformance",
            Stage::PropertyTesting => "property_testing",
            Stage::Fuzzing => "fuzzing",
            Stage::EvidenceValidation => "evidence_validation",
            Stage::ReleaseGate => "release_gate",
        }
    }
}

/// Outcome of one stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum StageOutcome {
    Passed,
    Failed { reason: String },
}

/// A recorded stage result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageReport {
    pub stage: Stage,
    #[serde(flatten)]
    pub outcome: StageOutcome,
}

/// The ordered trace of an artifact walking the pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceTrace {
    pub stages: Vec<StageReport>,
}

impl AcceptanceTrace {
    /// Begin a trace (the proposal stage is recorded as passed — the
    /// artifact exists; it is trusted for nothing).
    pub fn begin() -> Self {
        Self {
            stages: vec![StageReport {
                stage: Stage::AiProposal,
                outcome: StageOutcome::Passed,
            }],
        }
    }

    /// Record a passing stage; stages must be appended in order.
    pub fn pass(&mut self, stage: Stage) -> &mut Self {
        debug_assert_eq!(
            self.stages.last().map(|r| stage_index(r.stage)),
            Some(stage_index(stage).saturating_sub(1)),
            "stages must be recorded in pipeline order"
        );
        self.stages.push(StageReport {
            stage,
            outcome: StageOutcome::Passed,
        });
        self
    }

    /// Record a failure and stop: nothing past this stage is claimed.
    pub fn fail(&mut self, stage: Stage, reason: impl Into<String>) -> &mut Self {
        self.stages.push(StageReport {
            stage,
            outcome: StageOutcome::Failed {
                reason: reason.into(),
            },
        });
        self
    }

    /// The whole pipeline passed.
    pub fn accepted(&self) -> bool {
        self.stages.len() == Stage::ORDERED.len()
            && self
                .stages
                .iter()
                .all(|r| r.outcome == StageOutcome::Passed)
    }

    /// The first failed stage, if any.
    pub fn rejected_at(&self) -> Option<(Stage, &str)> {
        self.stages.iter().find_map(|r| match &r.outcome {
            StageOutcome::Failed { reason } => Some((r.stage, reason.as_str())),
            _ => None,
        })
    }
}

fn stage_index(stage: Stage) -> usize {
    Stage::ORDERED
        .iter()
        .position(|s| *s == stage)
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_pass_is_accepted() {
        let mut t = AcceptanceTrace::begin();
        for stage in Stage::ORDERED.iter().skip(1) {
            t.pass(*stage);
        }
        assert!(t.accepted());
        assert!(t.rejected_at().is_none());
    }

    #[test]
    fn failure_records_rejection_point_and_nothing_past_it() {
        let mut t = AcceptanceTrace::begin();
        t.pass(Stage::CompilerTypeSystem);
        t.pass(Stage::SpecificationObligations);
        t.fail(
            Stage::FormalProofOrModelChecking,
            "obligation X has no proof artifact",
        );
        assert!(!t.accepted());
        let (stage, reason) = t.rejected_at().unwrap();
        assert_eq!(stage, Stage::FormalProofOrModelChecking);
        assert!(reason.contains("no proof artifact"));
        // The trace ends at the failure: fuzzing was never claimed.
        assert_eq!(t.stages.len(), 4);
        assert_ne!(t.stages.last().unwrap().stage, Stage::ReleaseGate);
    }

    #[test]
    fn acceptance_trace_serializes() {
        let mut t = AcceptanceTrace::begin();
        t.fail(Stage::ImplementationConformance, "trace diverges at step 3");
        let json = serde_json::to_string(&t).unwrap();
        let back: AcceptanceTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
        assert!(json.contains("implementation_conformance"));
    }

    #[test]
    fn stages_are_complete_and_ordered() {
        // §7's path, in order, ending at the gate.
        assert_eq!(Stage::ORDERED.first(), Some(&Stage::AiProposal));
        assert_eq!(Stage::ORDERED.last(), Some(&Stage::ReleaseGate));
        assert_eq!(Stage::ORDERED.len(), 9);
    }
}
