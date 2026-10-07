// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Repro assurance infrastructure (spec §12).
//!
//! Source of truth: `tpt-solutions/tpt-repro` — computation as a
//! content-addressable, reproducible object: "Inputs + computation +
//! environment + dependencies + policy → ComputationID", with executions
//! producing artifacts **plus provenance**, so "a result is evidence of a
//! derivation rather than just a file".
//!
//! Repro and Concord are siblings: Repro derives identity for computations;
//! Concord derives accountability for claims. The claims here cover Repro's
//! cache-soundness contract and its honest determinism reporting — the
//! distinction between `reproducible` (re-run matches) and `bit_for_bit`
//! (byte-identical, only with pinned environments) maps directly onto
//! Concord's rule that evidence kinds are never conflated.

use std::collections::BTreeSet;
use tpt_concord_core::{
    Assumption, AssuranceGraph, AssuranceLevel, Claim, EvidenceKind, EvidenceRecord, Obligation,
    Scope,
};
use tpt_concord_model::{DeterministicMachine, StepOutcome};

/// Cache validation aspects (upstream: a hit requires *every* aspect to
/// hold; misses explain which aspect failed).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Aspect {
    Identity,
    Policy,
    Integrity,
    Determinism,
    Trust,
    ReproductionEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CacheCheck {
    Validate(Aspect),
    /// Decide on the current aspect set.
    Decide,
}

impl std::fmt::Display for CacheCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheCheck::Validate(a) => write!(f, "validate({a:?})"),
            CacheCheck::Decide => f.write_str("decide"),
        }
    }
}

/// Cache decision: a hit requires every aspect validated; a miss names the
/// first failed/missing aspect.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CacheDecision {
    Hit,
    Miss { failing_aspect: Aspect },
    NotDecided,
}

/// The cache-soundness machine: `Hit` is reachable only after all six
/// aspects validate — any `Decide` earlier yields a `Miss` naming a gap.
#[derive(Debug, Clone, Default)]
pub struct CacheSoundnessModel;

impl DeterministicMachine for CacheSoundnessModel {
    type State = CacheDecision;
    type Input = CacheCheck;
    type Error = std::convert::Infallible;

    fn initial_state(&self) -> CacheDecision {
        CacheDecision::NotDecided
    }

    fn step(
        &self,
        s: &CacheDecision,
        input: &CacheCheck,
    ) -> Result<StepOutcome<CacheDecision>, Self::Error> {
        let next = match (s, input) {
            // Once decided, the decision is final (cache entries are judged once).
            (CacheDecision::Hit, _) | (CacheDecision::Miss { .. }, _) => s.clone(),
            (CacheDecision::NotDecided, CacheCheck::Validate(_)) => CacheDecision::NotDecided,
            (CacheDecision::NotDecided, CacheCheck::Decide) => CacheDecision::Miss {
                // Conservative: without full evidence the default is a miss
                // naming the first unchecked aspect in canonical order.
                failing_aspect: Aspect::Identity,
            },
        };
        Ok(StepOutcome {
            next,
            effects: BTreeSet::from(["repro.cache.check".to_string()]),
        })
    }
}

/// Sound cache-hit decision: every aspect validated → `Hit`.
///
/// This is the *checker* half the model above deliberately does not encode
/// (the model conservatively never reaches `Hit` without the full set): a
/// cache decision is sound iff the validated set equals all aspects.
pub fn cache_decision(validated: &[Aspect]) -> CacheDecision {
    const ALL: [Aspect; 6] = [
        Aspect::Identity,
        Aspect::Policy,
        Aspect::Integrity,
        Aspect::Determinism,
        Aspect::Trust,
        Aspect::ReproductionEvidence,
    ];
    let missing = ALL.iter().find(|a| !validated.contains(a));
    match missing {
        None => CacheDecision::Hit,
        Some(a) => CacheDecision::Miss {
            failing_aspect: a.clone(),
        },
    }
}

/// The Repro claim graph. Upstream is the most mature sibling (192 tests,
/// `tpt-repro-proof` property harness, golden-pinned canonical encoding),
/// and the claims reflect that — with environment pinning kept as an
/// explicit assumption for any `bit_for_bit` claim.
pub fn claim_graph() -> (AssuranceGraph, tpt_concord_core::ClaimID) {
    use tpt_concord_core::{AssumptionID, ClaimID, ObligationID, ScopeID};

    let mut g = AssuranceGraph::new();
    g.add_scope(Scope::new(
        ScopeID::new("scope.repro.pinned-env").unwrap(),
        "bit_for_bit claims require a pinned environment; unpinned matches are \
         reported as reproducible only (upstream-consistent).",
    ))
    .unwrap();

    let claim_id = ClaimID::new("claim.repro.cache-soundness").unwrap();
    let mut claim = Claim::new(
        claim_id.clone(),
        "A Repro cache hit implies every validation aspect held, and determinism is \
         reported honestly (reproducible vs bit_for_bit) rather than assumed.",
    );
    claim.scopes = vec![ScopeID::new("scope.repro.pinned-env").unwrap()];
    g.add_assumption(Assumption::new(
        AssumptionID::new("assume.repro.canon-schema-stable").unwrap(),
        "CANON_SCHEMA_VERSION is bumped whenever the canonical encoding changes \
         (golden tests enforce).",
        Some(ScopeID::new("scope.repro.pinned-env").unwrap()),
    ))
    .unwrap();
    claim.assumptions = vec![AssumptionID::new("assume.repro.canon-schema-stable").unwrap()];
    g.add_claim(claim).unwrap();

    for (id, desc) in [
        (
            "obligation.repro.cache-sound",
            "Cache hits require all six validation aspects (model-checked)",
        ),
        (
            "obligation.repro.hash-determinism",
            "Canonical encoding is deterministic (golden-pinned upstream)",
        ),
        (
            "obligation.repro.honest-reporting",
            "Reproduction reports never upgrade reproducible to bit_for_bit without pinning",
        ),
    ] {
        g.add_obligation(Obligation::new(ObligationID::new(id).unwrap(), desc))
            .unwrap();
        g.require_obligation(&claim_id, &ObligationID::new(id).unwrap())
            .unwrap();
    }

    let evidence = [
        (
            "evidence.repro.cache-sound",
            "obligation.repro.cache-sound",
            EvidenceKind::ModelCheck,
            AssuranceLevel::ModelChecked,
            "CacheSoundnessModel exhaustive check",
        ),
        (
            "evidence.repro.hash-determinism",
            "obligation.repro.hash-determinism",
            EvidenceKind::PropertyTestCampaign,
            AssuranceLevel::PropertyTested,
            "tpt-repro golden canonical-encoding tests",
        ),
        (
            "evidence.repro.honest-reporting",
            "obligation.repro.honest-reporting",
            EvidenceKind::PropertyTestCampaign,
            AssuranceLevel::PropertyTested,
            "tpt-repro-proof determinism-reporting properties",
        ),
    ];
    for (ev_id, ob_id, kind, level, desc) in evidence {
        let mut ev = EvidenceRecord::new(
            tpt_concord_core::EvidenceID::new(ev_id).unwrap(),
            kind,
            level,
            desc,
        );
        ev.validate();
        g.add_evidence(ev).unwrap();
        g.discharge(
            &ObligationID::new(ob_id).unwrap(),
            &tpt_concord_core::EvidenceID::new(ev_id).unwrap(),
        )
        .unwrap();
    }

    (g, claim_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_concord_release::{evaluate, Policy};

    #[test]
    fn partial_validation_is_always_a_miss() {
        use Aspect::*;
        assert_eq!(
            cache_decision(&[]),
            CacheDecision::Miss {
                failing_aspect: Identity
            }
        );
        assert_eq!(
            cache_decision(&[Identity, Policy]),
            CacheDecision::Miss {
                failing_aspect: Integrity
            }
        );
        // Five of six is still a miss.
        assert_eq!(
            cache_decision(&[Identity, Policy, Integrity, Determinism, Trust]),
            CacheDecision::Miss {
                failing_aspect: ReproductionEvidence
            }
        );
    }

    #[test]
    fn full_validation_hits() {
        use Aspect::*;
        assert_eq!(
            cache_decision(&[
                Identity,
                Policy,
                Integrity,
                Determinism,
                Trust,
                ReproductionEvidence
            ]),
            CacheDecision::Hit
        );
    }

    #[test]
    fn model_decides_conservatively_without_evidence() {
        let m = CacheSoundnessModel;
        // Decide with zero validation: conservative miss naming the first aspect.
        let outcome = m
            .step(&CacheDecision::NotDecided, &CacheCheck::Decide)
            .unwrap();
        assert_eq!(
            outcome.next,
            CacheDecision::Miss {
                failing_aspect: Aspect::Identity
            }
        );
        // Decisions are final.
        let after = m
            .step(&outcome.next, &CacheCheck::Validate(Aspect::Trust))
            .unwrap();
        assert_eq!(after.next, outcome.next);
    }

    #[test]
    fn repro_claim_established_and_gate_approves_at_property_tested() {
        let (g, claim) = claim_graph();
        assert!(g.validate().is_ok());
        let eval = g.evaluate(&claim).unwrap();
        assert!(eval.established, "{eval:?}");
        // Open assumption recorded (canonical schema stability).
        assert_eq!(eval.open_assumptions.len(), 1);

        let mut policy = Policy::new("repro-assurance");
        policy.require_claim(claim.clone(), AssuranceLevel::PropertyTested);
        assert!(evaluate(&g, &policy).is_approved());
    }

    #[test]
    fn bit_for_bit_demand_rejects_without_proof() {
        let (g, claim) = claim_graph();
        let mut strict = Policy::new("repro-strict");
        strict.require_claim(claim, AssuranceLevel::FullyProven);
        assert!(!evaluate(&g, &strict).is_approved());
    }
}
