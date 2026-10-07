// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! The Assurance Graph (spec §3.4): Claims, Obligations and Evidence as a
//! directed graph answering "why is this claim trusted?" without collapsing
//! different evidence types into one vague `verified` label.
//!
//! Structure:
//! - a **claim** *requires* obligations,
//! - a **claim** may *depend on* other claims (composition),
//! - an **obligation** is *discharged by* evidence,
//! - claims may be qualified by scopes and rest on assumptions.
//!
//! The graph is append-only in structure; evidence status (`Unvalidated`,
//! `Validated`, `Rejected`) is the mutable part, reflecting independent
//! re-checking of artifacts.

use crate::assumption::Assumption;
use crate::error::Error;
use crate::id::{AssumptionID, ClaimID, EvidenceID, ObligationID, ScopeID};
use crate::level::AssuranceLevel;
use crate::scope::Scope;
use crate::version::Version;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A precise statement about a system whose acceptance is mediated by the graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub id: ClaimID,
    /// The precise statement being claimed.
    pub statement: String,
    /// Version of the claimed-about object (implementation/spec revision).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<Version>,
    /// Scopes under which the claim is meant to hold.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<ScopeID>,
    /// Assumptions the claim rests on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<AssumptionID>,
}

impl Claim {
    pub fn new(id: impl Into<ClaimID>, statement: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            statement: statement.into(),
            version: None,
            scopes: Vec::new(),
            assumptions: Vec::new(),
        }
    }
}

/// Something that must be established before a claim can be accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Obligation {
    pub id: ObligationID,
    /// What must be established, stated precisely.
    pub description: String,
}

impl Obligation {
    pub fn new(id: impl Into<ObligationID>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
        }
    }
}

/// The kind of artifact behind a piece of evidence.
///
/// Keeping kinds distinct is the point: a fuzz campaign must never be
/// presentable as a Lean proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// A Lean theorem/proof artifact.
    LeanProof,
    /// A Telos proof artifact.
    TelosProof,
    /// An Eidos proof artifact.
    EidosProof,
    /// A model-checking result.
    ModelCheck,
    /// A conformance run comparing implementation against model.
    ConformanceTrace,
    /// A deterministic simulation result.
    Simulation,
    /// A property-testing campaign result.
    PropertyTestCampaign,
    /// A fuzz campaign result.
    FuzzCampaign,
    /// A differential comparison result.
    DifferentialComparison,
    /// An ordinary test-suite run.
    TestRun,
    /// Anything else, named explicitly.
    Other(String),
}

impl std::fmt::Display for EvidenceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvidenceKind::LeanProof => f.write_str("lean_proof"),
            EvidenceKind::TelosProof => f.write_str("telos_proof"),
            EvidenceKind::EidosProof => f.write_str("eidos_proof"),
            EvidenceKind::ModelCheck => f.write_str("model_check"),
            EvidenceKind::ConformanceTrace => f.write_str("conformance_trace"),
            EvidenceKind::Simulation => f.write_str("simulation"),
            EvidenceKind::PropertyTestCampaign => f.write_str("property_test_campaign"),
            EvidenceKind::FuzzCampaign => f.write_str("fuzz_campaign"),
            EvidenceKind::DifferentialComparison => f.write_str("differential_comparison"),
            EvidenceKind::TestRun => f.write_str("test_run"),
            EvidenceKind::Other(s) => write!(f, "other({s})"),
        }
    }
}

/// Where the evidence artifact lives and how to detect tampering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Path or URI of the artifact (relative to an evidence bundle root).
    pub path: String,
    /// SHA-256 hex digest of the artifact contents.
    pub sha256: String,
}

/// Validation state of a piece of evidence.
///
/// The core never trusts an adapter's word: evidence starts `Unvalidated` and
/// becomes `Validated` only after the artifact has been independently checked
/// (re-run, hash-verified, replayed — whatever the kind demands).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Unvalidated,
    Validated,
    Rejected,
}

/// A machine-addressable object demonstrating an obligation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub id: EvidenceID,
    pub kind: EvidenceKind,
    /// What assurance level this evidence, if validated, supports.
    pub level: AssuranceLevel,
    pub description: String,
    /// Reference to the checkable artifact backing this record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactRef>,
    pub status: EvidenceStatus,
}

impl EvidenceRecord {
    pub fn new(
        id: impl Into<EvidenceID>,
        kind: EvidenceKind,
        level: AssuranceLevel,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            level,
            description: description.into(),
            artifact: None,
            status: EvidenceStatus::Unvalidated,
        }
    }

    /// Attach the independently checkable artifact.
    pub fn with_artifact(mut self, path: impl Into<String>, sha256: impl Into<String>) -> Self {
        self.artifact = Some(ArtifactRef {
            path: path.into(),
            sha256: sha256.into(),
        });
        self
    }

    /// Mark evidence as independently validated.
    pub fn validate(&mut self) {
        self.status = EvidenceStatus::Validated;
    }

    /// Mark evidence as rejected (failed re-check).
    pub fn reject(&mut self) {
        self.status = EvidenceStatus::Rejected;
    }
}

/// An obligation without acceptable backing evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnmetObligation {
    pub obligation: ObligationID,
    pub reason: UnmetReason,
}

/// Why an obligation is unmet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnmetReason {
    /// No evidence has been linked at all.
    NoEvidence,
    /// Evidence exists but has not been independently validated.
    EvidenceUnvalidated,
    /// Evidence was linked but failed re-checking.
    EvidenceRejected,
}

/// Result of evaluating whether a claim is established.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimEvaluation {
    pub claim: ClaimID,
    pub established: bool,
    pub unmet_obligations: Vec<UnmetObligation>,
    /// Direct dependencies that are themselves not established.
    pub unestablished_dependencies: Vec<ClaimID>,
    /// IDs of validated evidence supporting this claim (direct obligations
    /// and, transitively, dependency claims).
    pub supporting_evidence: Vec<EvidenceID>,
    /// Strongest assurance level among validated supporting evidence.
    /// Policy hint only — see `AssuranceLevel::strictness_rank`.
    pub achieved_level: AssuranceLevel,
    /// Assumptions this claim (or its dependencies) rests on.
    pub open_assumptions: Vec<AssumptionID>,
}

/// The directed assurance graph over claims, obligations and evidence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssuranceGraph {
    claims: BTreeMap<ClaimID, Claim>,
    obligations: BTreeMap<ObligationID, Obligation>,
    evidence: BTreeMap<EvidenceID, EvidenceRecord>,
    scopes: BTreeMap<ScopeID, Scope>,
    assumptions: BTreeMap<AssumptionID, Assumption>,
    /// claim -> obligations it requires
    claim_obligations: BTreeMap<ClaimID, BTreeSet<ObligationID>>,
    /// claim -> claims it depends on
    claim_dependencies: BTreeMap<ClaimID, BTreeSet<ClaimID>>,
    /// obligation -> evidence discharging it
    obligation_evidence: BTreeMap<ObligationID, BTreeSet<EvidenceID>>,
}

impl AssuranceGraph {
    pub fn new() -> Self {
        Self::default()
    }

    // ---- object registration -------------------------------------------------

    /// Register a claim.
    pub fn add_claim(&mut self, claim: Claim) -> Result<(), Error> {
        if self.claims.contains_key(&claim.id) {
            return Err(Error::Duplicate {
                kind: "claim",
                id: claim.id.to_string(),
            });
        }
        self.claims.insert(claim.id.clone(), claim);
        Ok(())
    }

    /// Register an obligation.
    pub fn add_obligation(&mut self, obligation: Obligation) -> Result<(), Error> {
        if self.obligations.contains_key(&obligation.id) {
            return Err(Error::Duplicate {
                kind: "obligation",
                id: obligation.id.to_string(),
            });
        }
        self.obligations.insert(obligation.id.clone(), obligation);
        Ok(())
    }

    /// Register an evidence record (starts `Unvalidated`).
    pub fn add_evidence(&mut self, evidence: EvidenceRecord) -> Result<(), Error> {
        if self.evidence.contains_key(&evidence.id) {
            return Err(Error::Duplicate {
                kind: "evidence",
                id: evidence.id.to_string(),
            });
        }
        self.evidence.insert(evidence.id.clone(), evidence);
        Ok(())
    }

    /// Register a scope.
    pub fn add_scope(&mut self, scope: Scope) -> Result<(), Error> {
        if self.scopes.contains_key(&scope.id) {
            return Err(Error::Duplicate {
                kind: "scope",
                id: scope.id.to_string(),
            });
        }
        self.scopes.insert(scope.id.clone(), scope);
        Ok(())
    }

    /// Register an assumption.
    pub fn add_assumption(&mut self, assumption: Assumption) -> Result<(), Error> {
        if self.assumptions.contains_key(&assumption.id) {
            return Err(Error::Duplicate {
                kind: "assumption",
                id: assumption.id.to_string(),
            });
        }
        self.assumptions.insert(assumption.id.clone(), assumption);
        Ok(())
    }

    // ---- edges ---------------------------------------------------------------

    /// Record that `claim` requires `obligation` to be established.
    pub fn require_obligation(
        &mut self,
        claim: &ClaimID,
        obligation: &ObligationID,
    ) -> Result<(), Error> {
        if !self.claims.contains_key(claim) {
            return Err(Error::Unknown {
                kind: "claim",
                id: claim.to_string(),
            });
        }
        if !self.obligations.contains_key(obligation) {
            return Err(Error::Unknown {
                kind: "obligation",
                id: obligation.to_string(),
            });
        }
        self.claim_obligations
            .entry(claim.clone())
            .or_default()
            .insert(obligation.clone());
        Ok(())
    }

    /// Record that `claim` depends on `dependency` being established first.
    pub fn depend_on(&mut self, claim: &ClaimID, dependency: &ClaimID) -> Result<(), Error> {
        if !self.claims.contains_key(claim) {
            return Err(Error::Unknown {
                kind: "claim",
                id: claim.to_string(),
            });
        }
        if !self.claims.contains_key(dependency) {
            return Err(Error::Unknown {
                kind: "claim",
                id: dependency.to_string(),
            });
        }
        if claim == dependency {
            return Err(Error::SelfDependency(claim.clone()));
        }
        self.claim_dependencies
            .entry(claim.clone())
            .or_default()
            .insert(dependency.clone());
        Ok(())
    }

    /// Record that `evidence` discharges `obligation`.
    pub fn discharge(
        &mut self,
        obligation: &ObligationID,
        evidence: &EvidenceID,
    ) -> Result<(), Error> {
        if !self.obligations.contains_key(obligation) {
            return Err(Error::Unknown {
                kind: "obligation",
                id: obligation.to_string(),
            });
        }
        if !self.evidence.contains_key(evidence) {
            return Err(Error::Unknown {
                kind: "evidence",
                id: evidence.to_string(),
            });
        }
        self.obligation_evidence
            .entry(obligation.clone())
            .or_default()
            .insert(evidence.clone());
        Ok(())
    }

    // ---- accessors -----------------------------------------------------------

    pub fn claim(&self, id: &ClaimID) -> Option<&Claim> {
        self.claims.get(id)
    }

    pub fn obligation(&self, id: &ObligationID) -> Option<&Obligation> {
        self.obligations.get(id)
    }

    pub fn evidence(&self, id: &EvidenceID) -> Option<&EvidenceRecord> {
        self.evidence.get(id)
    }

    /// Mutable evidence access, for recording validation/rejection outcomes.
    pub fn evidence_mut(&mut self, id: &EvidenceID) -> Option<&mut EvidenceRecord> {
        self.evidence.get_mut(id)
    }

    pub fn claims(&self) -> impl Iterator<Item = &Claim> {
        self.claims.values()
    }

    pub fn obligations(&self) -> impl Iterator<Item = &Obligation> {
        self.obligations.values()
    }

    pub fn evidence_records(&self) -> impl Iterator<Item = &EvidenceRecord> {
        self.evidence.values()
    }

    pub fn scope(&self, id: &ScopeID) -> Option<&Scope> {
        self.scopes.get(id)
    }

    pub fn assumption(&self, id: &AssumptionID) -> Option<&Assumption> {
        self.assumptions.get(id)
    }

    pub fn obligations_of(&self, claim: &ClaimID) -> impl Iterator<Item = &ObligationID> {
        self.claim_obligations.get(claim).into_iter().flatten()
    }

    pub fn dependencies_of(&self, claim: &ClaimID) -> impl Iterator<Item = &ClaimID> {
        self.claim_dependencies.get(claim).into_iter().flatten()
    }

    pub fn evidence_for(&self, obligation: &ObligationID) -> impl Iterator<Item = &EvidenceID> {
        self.obligation_evidence
            .get(obligation)
            .into_iter()
            .flatten()
    }

    // ---- validation ----------------------------------------------------------

    /// Check structural integrity: all referenced scopes/assumptions exist,
    /// and the claim dependency relation is acyclic.
    pub fn validate(&self) -> Result<(), Error> {
        for claim in self.claims.values() {
            for scope in &claim.scopes {
                if !self.scopes.contains_key(scope) {
                    return Err(Error::UnknownScope(scope.clone()));
                }
            }
            for assumption in &claim.assumptions {
                if !self.assumptions.contains_key(assumption) {
                    return Err(Error::UnknownAssumption(assumption.clone()));
                }
            }
        }
        // Iterative DFS cycle detection over claim dependencies.
        const WHITE: u8 = 0;
        const GRAY: u8 = 1;
        let mut color: BTreeMap<&ClaimID, u8> = self.claims.keys().map(|k| (k, WHITE)).collect();
        for root in self.claims.keys() {
            let mut stack: Vec<(&ClaimID, Vec<&ClaimID>)> =
                vec![(root, self.direct_deps(root).collect())];
            color.insert(root, GRAY);
            while let Some((node, deps)) = stack.pop() {
                match deps.split_first() {
                    None => {
                        color.insert(node, 2);
                    }
                    Some((&first, rest)) => {
                        stack.push((node, rest.to_vec()));
                        match color.get(first).copied() {
                            Some(GRAY) => return Err(Error::Cycle(first.clone())),
                            Some(2) => {}
                            _ => {
                                color.insert(first, GRAY);
                                stack.push((first, self.direct_deps(first).collect()));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn direct_deps(&self, claim: &ClaimID) -> impl Iterator<Item = &ClaimID> {
        self.claim_dependencies.get(claim).into_iter().flatten()
    }

    /// Collect validated evidence reachable from a claim: evidence discharging
    /// its direct obligations, plus validated evidence of established
    /// dependencies. Cycle-safe.
    fn collect_supporting_evidence(&self, claim: &ClaimID) -> Vec<EvidenceRecord> {
        let mut supporting = Vec::new();
        let mut visited: BTreeSet<ClaimID> = BTreeSet::new();
        self.collect_supporting_evidence_rec(claim, &mut visited, &mut supporting);
        supporting
    }

    fn collect_supporting_evidence_rec(
        &self,
        claim: &ClaimID,
        visited: &mut BTreeSet<ClaimID>,
        out: &mut Vec<EvidenceRecord>,
    ) {
        if !visited.insert(claim.clone()) {
            return;
        }
        for obligation in self.obligations_of(claim) {
            for evidence_id in self.evidence_for(obligation) {
                if let Some(record) = self.evidence.get(evidence_id) {
                    if record.status == EvidenceStatus::Validated {
                        out.push(record.clone());
                    }
                }
            }
        }
        for dep in self.dependencies_of(claim) {
            self.collect_supporting_evidence_rec(dep, visited, out);
        }
    }

    fn assumptions_of(&self, claim: &ClaimID) -> Vec<AssumptionID> {
        let mut out: Vec<AssumptionID> = Vec::new();
        let mut visited: BTreeSet<ClaimID> = BTreeSet::new();
        self.assumptions_rec(claim, &mut visited, &mut out);
        out
    }

    fn assumptions_rec(
        &self,
        claim: &ClaimID,
        visited: &mut BTreeSet<ClaimID>,
        out: &mut Vec<AssumptionID>,
    ) {
        if !visited.insert(claim.clone()) {
            return;
        }
        if let Some(c) = self.claims.get(claim) {
            for a in &c.assumptions {
                if !out.contains(a) {
                    out.push(a.clone());
                }
            }
            for dep in self.dependencies_of(claim) {
                self.assumptions_rec(dep, visited, out);
            }
        }
    }

    /// Evaluate whether a claim is established: every required obligation is
    /// discharged by validated evidence, and every dependency claim is itself
    /// established.
    pub fn evaluate(&self, claim: &ClaimID) -> Result<ClaimEvaluation, Error> {
        let Some(_) = self.claims.get(claim) else {
            return Err(Error::Unknown {
                kind: "claim",
                id: claim.to_string(),
            });
        };

        let mut unmet_obligations = Vec::new();
        let mut established = true;

        for obligation in self.obligations_of(claim) {
            let evidence_ids: Vec<&EvidenceID> = self.evidence_for(obligation).collect();
            if evidence_ids.is_empty() {
                established = false;
                unmet_obligations.push(UnmetObligation {
                    obligation: obligation.clone(),
                    reason: UnmetReason::NoEvidence,
                });
                continue;
            }
            let any_valid = evidence_ids.iter().any(|e| {
                self.evidence
                    .get(*e)
                    .is_some_and(|r| r.status == EvidenceStatus::Validated)
            });
            if !any_valid {
                established = false;
                let all_rejected = evidence_ids.iter().all(|e| {
                    self.evidence
                        .get(*e)
                        .is_some_and(|r| r.status == EvidenceStatus::Rejected)
                });
                unmet_obligations.push(UnmetObligation {
                    obligation: obligation.clone(),
                    reason: if all_rejected {
                        UnmetReason::EvidenceRejected
                    } else {
                        UnmetReason::EvidenceUnvalidated
                    },
                });
            }
        }

        let mut unestablished_dependencies = Vec::new();
        for dep in self.dependencies_of(claim) {
            let dep_eval = self.evaluate(dep)?;
            if !dep_eval.established {
                established = false;
                unestablished_dependencies.push(dep.clone());
            }
        }

        let supporting = self.collect_supporting_evidence(claim);
        let supporting_evidence = supporting.iter().map(|e| e.id.clone()).collect();
        let achieved_level = supporting
            .iter()
            .map(|e| e.level)
            .max_by_key(AssuranceLevel::strictness_rank)
            .unwrap_or(AssuranceLevel::Specified);

        Ok(ClaimEvaluation {
            claim: claim.clone(),
            established,
            unmet_obligations,
            unestablished_dependencies,
            supporting_evidence,
            achieved_level,
            open_assumptions: self.assumptions_of(claim),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{ClaimID, EvidenceID, ObligationID};

    fn cid(s: &str) -> ClaimID {
        ClaimID::new(s).unwrap()
    }
    fn oid(s: &str) -> ObligationID {
        ObligationID::new(s).unwrap()
    }
    fn eid(s: &str) -> EvidenceID {
        EvidenceID::new(s).unwrap()
    }

    #[test]
    fn established_claim_with_validated_evidence() {
        let mut g = AssuranceGraph::new();
        g.add_claim(Claim::new(cid("c1"), "Recovery preserves committed txs"))
            .unwrap();
        g.add_obligation(Obligation::new(oid("o1"), "model check passes"))
            .unwrap();
        g.require_obligation(&cid("c1"), &oid("o1")).unwrap();

        let mut ev = EvidenceRecord::new(
            eid("e1"),
            EvidenceKind::ModelCheck,
            AssuranceLevel::ModelChecked,
            "exhaustive small-space check",
        );
        assert_eq!(ev.status, EvidenceStatus::Unvalidated);
        ev.validate();
        g.add_evidence(ev).unwrap();
        g.discharge(&oid("o1"), &eid("e1")).unwrap();

        let eval = g.evaluate(&cid("c1")).unwrap();
        assert!(eval.established);
        assert!(eval.unmet_obligations.is_empty());
        assert_eq!(eval.achieved_level, AssuranceLevel::ModelChecked);
        assert_eq!(eval.supporting_evidence, vec![eid("e1")]);
    }

    #[test]
    fn unvalidated_evidence_leaves_claim_unestablished() {
        let mut g = AssuranceGraph::new();
        g.add_claim(Claim::new(cid("c1"), "stmt")).unwrap();
        g.add_obligation(Obligation::new(oid("o1"), "desc"))
            .unwrap();
        g.require_obligation(&cid("c1"), &oid("o1")).unwrap();
        g.add_evidence(EvidenceRecord::new(
            eid("e1"),
            EvidenceKind::FuzzCampaign,
            AssuranceLevel::FuzzTested,
            "campaign",
        ))
        .unwrap();
        g.discharge(&oid("o1"), &eid("e1")).unwrap();

        let eval = g.evaluate(&cid("c1")).unwrap();
        assert!(!eval.established);
        assert_eq!(
            eval.unmet_obligations,
            vec![UnmetObligation {
                obligation: oid("o1"),
                reason: UnmetReason::EvidenceUnvalidated,
            }]
        );
    }

    #[test]
    fn rejected_evidence_reports_rejection() {
        let mut g = AssuranceGraph::new();
        g.add_claim(Claim::new(cid("c1"), "stmt")).unwrap();
        g.add_obligation(Obligation::new(oid("o1"), "desc"))
            .unwrap();
        g.require_obligation(&cid("c1"), &oid("o1")).unwrap();
        let mut ev = EvidenceRecord::new(
            eid("e1"),
            EvidenceKind::ConformanceTrace,
            AssuranceLevel::ModelChecked,
            "run",
        );
        ev.reject();
        g.add_evidence(ev).unwrap();
        g.discharge(&oid("o1"), &eid("e1")).unwrap();

        let eval = g.evaluate(&cid("c1")).unwrap();
        assert_eq!(
            eval.unmet_obligations[0].reason,
            UnmetReason::EvidenceRejected
        );
    }

    #[test]
    fn missing_evidence_reports_no_evidence() {
        let mut g = AssuranceGraph::new();
        g.add_claim(Claim::new(cid("c1"), "stmt")).unwrap();
        g.add_obligation(Obligation::new(oid("o1"), "desc"))
            .unwrap();
        g.require_obligation(&cid("c1"), &oid("o1")).unwrap();
        let eval = g.evaluate(&cid("c1")).unwrap();
        assert_eq!(eval.unmet_obligations[0].reason, UnmetReason::NoEvidence);
    }

    #[test]
    fn dependency_chain_composes() {
        let mut g = AssuranceGraph::new();
        g.add_claim(Claim::new(cid("base"), "base claim")).unwrap();
        g.add_claim(Claim::new(cid("top"), "top claim")).unwrap();
        g.add_obligation(Obligation::new(oid("ob"), "ob desc"))
            .unwrap();
        g.depend_on(&cid("top"), &cid("base")).unwrap();
        g.require_obligation(&cid("base"), &oid("ob")).unwrap();

        let mut ev = EvidenceRecord::new(
            eid("ev"),
            EvidenceKind::LeanProof,
            AssuranceLevel::FullyProven,
            "theorem",
        );
        ev.validate();
        g.add_evidence(ev).unwrap();
        g.discharge(&oid("ob"), &eid("ev")).unwrap();

        let eval = g.evaluate(&cid("top")).unwrap();
        assert!(eval.established, "top should inherit base's establishment");
        assert_eq!(eval.achieved_level, AssuranceLevel::FullyProven);
    }

    #[test]
    fn unestablished_dependency_blocks_claim() {
        let mut g = AssuranceGraph::new();
        g.add_claim(Claim::new(cid("base"), "base")).unwrap();
        g.add_claim(Claim::new(cid("top"), "top")).unwrap();
        g.depend_on(&cid("top"), &cid("base")).unwrap();
        g.add_obligation(Obligation::new(oid("ob"), "ob")).unwrap();
        g.require_obligation(&cid("base"), &oid("ob")).unwrap();

        let eval = g.evaluate(&cid("top")).unwrap();
        assert!(!eval.established);
        assert_eq!(eval.unestablished_dependencies, vec![cid("base")]);
    }

    #[test]
    fn cycle_is_rejected_by_validate() {
        let mut g = AssuranceGraph::new();
        g.add_claim(Claim::new(cid("a"), "a")).unwrap();
        g.add_claim(Claim::new(cid("b"), "b")).unwrap();
        g.add_claim(Claim::new(cid("c"), "c")).unwrap();
        g.depend_on(&cid("a"), &cid("b")).unwrap();
        g.depend_on(&cid("b"), &cid("c")).unwrap();
        g.depend_on(&cid("c"), &cid("a")).unwrap();
        assert!(matches!(g.validate(), Err(Error::Cycle(_))));
    }

    #[test]
    fn duplicates_are_rejected() {
        let mut g = AssuranceGraph::new();
        g.add_claim(Claim::new(cid("x"), "one")).unwrap();
        assert!(matches!(
            g.add_claim(Claim::new(cid("x"), "two")),
            Err(Error::Duplicate { kind: "claim", .. })
        ));
    }

    #[test]
    fn dangling_links_are_rejected() {
        let mut g = AssuranceGraph::new();
        assert!(matches!(
            g.require_obligation(&cid("nope"), &oid("alsono")),
            Err(Error::Unknown { kind: "claim", .. })
        ));
    }

    #[test]
    fn self_dependency_is_rejected() {
        let mut g = AssuranceGraph::new();
        g.add_claim(Claim::new(cid("a"), "a")).unwrap();
        assert!(matches!(
            g.depend_on(&cid("a"), &cid("a")),
            Err(Error::SelfDependency(_))
        ));
    }

    #[test]
    fn open_assumptions_are_reported_through_dependencies() {
        let mut g = AssuranceGraph::new();
        g.add_assumption(Assumption::new(
            crate::id::AssumptionID::new("disk-prefix-survives").unwrap(),
            "surviving disk prefix length >= committed watermark after crash",
            None,
        ))
        .unwrap();
        g.add_claim(Claim::new(cid("base"), "base")).unwrap();
        g.add_claim(Claim::new(cid("top"), "top")).unwrap();
        g.claims
            .get_mut(&cid("base"))
            .unwrap()
            .assumptions
            .push(crate::id::AssumptionID::new("disk-prefix-survives").unwrap());
        g.depend_on(&cid("top"), &cid("base")).unwrap();

        let eval = g.evaluate(&cid("top")).unwrap();
        assert_eq!(
            eval.open_assumptions,
            vec![crate::id::AssumptionID::new("disk-prefix-survives").unwrap()]
        );
    }

    #[test]
    fn graph_serializes() {
        let mut g = AssuranceGraph::new();
        g.add_claim(Claim::new(cid("c1"), "statement")).unwrap();
        let json = serde_json::to_string(&g).unwrap();
        let back: AssuranceGraph = serde_json::from_str(&json).unwrap();
        assert!(back.claim(&cid("c1")).is_some());
    }
}
