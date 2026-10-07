// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Policy and evaluation for release decisions.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tpt_concord_core::{
    AssuranceGraph, AssuranceLevel, ClaimEvaluation, ClaimID, EvidenceStatus, ObligationID,
};

/// Minimum assurance demanded of one required claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimRequirement {
    /// Weakest acceptable assurance level for this claim.
    pub min_level: AssuranceLevel,
}

impl ClaimRequirement {
    pub fn at_least(min_level: AssuranceLevel) -> Self {
        Self { min_level }
    }
}

/// What the release gate demands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub name: String,
    /// Claims that must be established, with their minimum levels.
    pub required_claims: BTreeMap<ClaimID, ClaimRequirement>,
    /// Obligations that must be discharged by validated evidence regardless
    /// of which claim (if any) references them.
    pub required_obligations: BTreeSet<ObligationID>,
    /// If false, any claim in the closure of the required claims that rests
    /// on open assumptions causes rejection.
    pub allow_open_assumptions: bool,
}

impl Policy {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            required_claims: BTreeMap::new(),
            required_obligations: BTreeSet::new(),
            allow_open_assumptions: true,
        }
    }

    /// Require a claim at a minimum assurance level.
    pub fn require_claim(&mut self, claim: ClaimID, min_level: AssuranceLevel) -> &mut Self {
        self.required_claims
            .insert(claim, ClaimRequirement::at_least(min_level));
        self
    }

    /// Require an obligation to be discharged by validated evidence.
    pub fn require_obligation(&mut self, obligation: ObligationID) -> &mut Self {
        self.required_obligations.insert(obligation);
        self
    }
}

/// Why the gate rejected a release. Every reason is machine-readable and
/// names the offending object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum RejectionReason {
    GraphInvalid {
        detail: String,
    },
    UnknownClaim {
        claim: ClaimID,
    },
    UnknownObligation {
        obligation: ObligationID,
    },
    ClaimUnestablished {
        claim: ClaimID,
        #[serde(flatten)]
        evaluation: ClaimEvaluation,
    },
    LevelInsufficient {
        claim: ClaimID,
        required: AssuranceLevel,
        achieved: AssuranceLevel,
    },
    OpenAssumptions {
        claim: ClaimID,
        assumptions: Vec<tpt_concord_core::AssumptionID>,
    },
    UndischargedObligation {
        obligation: ObligationID,
    },
}

/// A gate approval, with what was accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateSummary {
    pub policy: String,
    pub established_claims: Vec<ClaimID>,
    /// Achieved level per required claim (policy hint, see
    /// `AssuranceLevel::strictness_rank`).
    pub achieved_levels: BTreeMap<ClaimID, AssuranceLevel>,
    /// Count of validated evidence objects supporting the decision.
    pub supporting_evidence: usize,
}

/// The gate's decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum GateDecision {
    Approved(GateSummary),
    Rejected { reasons: Vec<RejectionReason> },
}

impl GateDecision {
    pub fn is_approved(&self) -> bool {
        matches!(self, GateDecision::Approved(_))
    }
}

/// Evaluate `policy` against `graph`.
pub fn evaluate(graph: &AssuranceGraph, policy: &Policy) -> GateDecision {
    if let Err(e) = graph.validate() {
        return GateDecision::Rejected {
            reasons: vec![RejectionReason::GraphInvalid {
                detail: e.to_string(),
            }],
        };
    }

    let mut reasons = Vec::new();
    let mut established_claims = Vec::new();
    let mut achieved_levels = BTreeMap::new();
    let mut supporting_evidence = 0usize;

    for (claim, requirement) in &policy.required_claims {
        let eval = match graph.evaluate(claim) {
            Ok(eval) => eval,
            Err(_) => {
                reasons.push(RejectionReason::UnknownClaim {
                    claim: claim.clone(),
                });
                continue;
            }
        };
        if !eval.established {
            reasons.push(RejectionReason::ClaimUnestablished {
                claim: claim.clone(),
                evaluation: eval.clone(),
            });
            continue;
        }
        if !eval.supporting_evidence.is_empty() {
            supporting_evidence += eval.supporting_evidence.len();
        }
        if !eval.achieved_level.satisfies(requirement.min_level) {
            reasons.push(RejectionReason::LevelInsufficient {
                claim: claim.clone(),
                required: requirement.min_level,
                achieved: eval.achieved_level,
            });
            continue;
        }
        if !policy.allow_open_assumptions && !eval.open_assumptions.is_empty() {
            reasons.push(RejectionReason::OpenAssumptions {
                claim: claim.clone(),
                assumptions: eval.open_assumptions.clone(),
            });
            continue;
        }
        established_claims.push(claim.clone());
        achieved_levels.insert(claim.clone(), eval.achieved_level);
    }

    for obligation in &policy.required_obligations {
        if graph.obligation(obligation).is_none() {
            reasons.push(RejectionReason::UnknownObligation {
                obligation: obligation.clone(),
            });
            continue;
        }
        let discharged = graph.evidence_for(obligation).any(|e| {
            graph
                .evidence(e)
                .is_some_and(|r| r.status == EvidenceStatus::Validated)
        });
        if !discharged {
            reasons.push(RejectionReason::UndischargedObligation {
                obligation: obligation.clone(),
            });
        }
    }

    if reasons.is_empty() {
        GateDecision::Approved(GateSummary {
            policy: policy.name.clone(),
            established_claims,
            achieved_levels,
            supporting_evidence,
        })
    } else {
        GateDecision::Rejected { reasons }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_concord_core::{Claim, EvidenceID, EvidenceKind, EvidenceRecord, Obligation};

    fn cid(s: &str) -> ClaimID {
        ClaimID::new(s).unwrap()
    }
    fn oid(s: &str) -> ObligationID {
        ObligationID::new(s).unwrap()
    }
    fn eid(s: &str) -> EvidenceID {
        tpt_concord_core::EvidenceID::new(s).unwrap()
    }

    fn approved_graph() -> AssuranceGraph {
        let mut g = AssuranceGraph::new();
        g.add_claim(Claim::new(cid("wal.claim"), "recovery preserves committed"))
            .unwrap();
        g.add_obligation(Obligation::new(oid("wal.modelcheck"), "model check"))
            .unwrap();
        g.require_obligation(&cid("wal.claim"), &oid("wal.modelcheck"))
            .unwrap();
        let mut ev = EvidenceRecord::new(
            eid("ev.modelcheck"),
            EvidenceKind::ModelCheck,
            AssuranceLevel::ModelChecked,
            "exhaustive",
        );
        ev.validate();
        g.add_evidence(ev).unwrap();
        g.discharge(&oid("wal.modelcheck"), &eid("ev.modelcheck"))
            .unwrap();
        g
    }

    #[test]
    fn approves_established_claim_at_level() {
        let g = approved_graph();
        let mut policy = Policy::new("wal-release");
        policy.require_claim(cid("wal.claim"), AssuranceLevel::ModelChecked);
        let decision = evaluate(&g, &policy);
        let GateDecision::Approved(summary) = decision else {
            panic!("expected approval, got {decision:?}");
        };
        assert_eq!(summary.established_claims, vec![cid("wal.claim")]);
        assert_eq!(
            summary.achieved_levels[&cid("wal.claim")],
            AssuranceLevel::ModelChecked
        );
        assert_eq!(summary.supporting_evidence, 1);
    }

    #[test]
    fn rejects_unestablished_claim_with_reasons() {
        let g = approved_graph();
        let mut policy = Policy::new("wal-release");
        policy.require_claim(cid("wal.claim"), AssuranceLevel::FullyProven);
        let decision = evaluate(&g, &policy);
        let GateDecision::Rejected { reasons } = decision else {
            panic!("expected rejection");
        };
        // Level insufficient: model_checked < fully_proven.
        assert!(matches!(
            &reasons[0],
            RejectionReason::LevelInsufficient { required, achieved, .. }
                if *required == AssuranceLevel::FullyProven
                    && *achieved == AssuranceLevel::ModelChecked
        ));
    }

    #[test]
    fn rejects_unknown_claim() {
        let g = approved_graph();
        let mut policy = Policy::new("p");
        policy.require_claim(cid("nope"), AssuranceLevel::Specified);
        let GateDecision::Rejected { reasons } = evaluate(&g, &policy) else {
            panic!("expected rejection");
        };
        assert!(matches!(&reasons[0], RejectionReason::UnknownClaim { .. }));
    }

    #[test]
    fn rejects_undischarged_required_obligation() {
        let mut g = approved_graph();
        g.add_obligation(Obligation::new(oid("wal.fuzz"), "fuzz campaign"))
            .unwrap();
        let mut policy = Policy::new("p");
        policy.require_obligation(oid("wal.fuzz"));
        let GateDecision::Rejected { reasons } = evaluate(&g, &policy) else {
            panic!("expected rejection");
        };
        assert_eq!(
            reasons[0],
            RejectionReason::UndischargedObligation {
                obligation: oid("wal.fuzz")
            }
        );
    }

    #[test]
    fn open_assumptions_can_be_forbidden() {
        let aid = || tpt_concord_core::AssumptionID::new("disk-prefix").unwrap();
        // Graph without any assumption: approves with allow_open_assumptions=false.
        let g_clean = approved_graph();
        let mut policy = Policy::new("p");
        policy.allow_open_assumptions = false;
        policy.require_claim(cid("wal.claim"), AssuranceLevel::ModelChecked);
        assert!(evaluate(&g_clean, &policy).is_approved());

        // Same graph but the claim rests on an open assumption: rejected.
        let mut g_open = approved_graph();
        g_open
            .add_assumption(tpt_concord_core::Assumption::new(
                aid(),
                "disk prefix survives",
                None,
            ))
            .unwrap();
        let mut claim = g_open.claim(&cid("wal.claim")).unwrap().clone();
        claim.assumptions.push(aid());
        // Rebuild with the assumption defined and attached to the claim.
        let mut g_attached = AssuranceGraph::new();
        g_attached
            .add_assumption(tpt_concord_core::Assumption::new(
                aid(),
                "disk prefix survives",
                None,
            ))
            .unwrap();
        g_attached.add_claim(claim).unwrap();
        g_attached
            .add_obligation(Obligation::new(oid("wal.modelcheck"), "model check"))
            .unwrap();
        g_attached
            .require_obligation(&cid("wal.claim"), &oid("wal.modelcheck"))
            .unwrap();
        let mut ev = EvidenceRecord::new(
            eid("ev.modelcheck"),
            EvidenceKind::ModelCheck,
            AssuranceLevel::ModelChecked,
            "exhaustive",
        );
        ev.validate();
        g_attached.add_evidence(ev).unwrap();
        g_attached
            .discharge(&oid("wal.modelcheck"), &eid("ev.modelcheck"))
            .unwrap();

        let GateDecision::Rejected { reasons } = evaluate(&g_attached, &policy) else {
            panic!("expected rejection");
        };
        assert!(matches!(
            &reasons[0],
            RejectionReason::OpenAssumptions { .. }
        ));

        // Same graph approves when open assumptions are allowed.
        let mut permissive = Policy::new("p");
        permissive.require_claim(cid("wal.claim"), AssuranceLevel::ModelChecked);
        assert!(evaluate(&g_attached, &permissive).is_approved());
    }

    #[test]
    fn cycle_rejected_as_graph_invalid() {
        let mut g = AssuranceGraph::new();
        g.add_claim(Claim::new(cid("a"), "a")).unwrap();
        g.add_claim(Claim::new(cid("b"), "b")).unwrap();
        g.depend_on(&cid("a"), &cid("b")).unwrap();
        g.depend_on(&cid("b"), &cid("a")).unwrap();
        let policy = Policy::new("p");
        let GateDecision::Rejected { reasons } = evaluate(&g, &policy) else {
            panic!("expected rejection");
        };
        assert!(matches!(&reasons[0], RejectionReason::GraphInvalid { .. }));
    }

    #[test]
    fn decision_serializes() {
        let mut policy = Policy::new("p");
        policy.require_claim(cid("x"), AssuranceLevel::Specified);
        let g = AssuranceGraph::new();
        let json = serde_json::to_string(&evaluate(&g, &policy)).unwrap();
        assert!(json.contains("\"decision\":\"rejected\""));
    }
}
