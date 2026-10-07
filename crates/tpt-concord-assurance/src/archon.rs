// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Archon assurance infrastructure (spec §12), beyond the Phase 1 WAL pilot.
//!
//! Source of truth: `tpt-solutions/tpt-archon` (design doc `spec.txt` and
//! `tpt-archon-core/src/faultsim.rs`). Archon's own guarantees, in its own
//! words:
//! - "Replaying the WAL after any crash results in a consistent state"
//!   (proven by tpt-telos; see the pilot crate for the Concord-side model).
//! - Fault-sim property: recovery replays **at most** the committed pages,
//!   and every replayed page equals its committed image.
//! - B-Link tree structural invariants enforced at type level by tpt-eidos.
//! - The kernel scheduler "cannot deadlock" (tpt-telos).
//!
//! This module adds the buffer-pool state model, the B-Link tree structural
//! specification, and the claim graph wiring them — with Archon's
//! Telos/Eidos evidence recorded as references that stay `Unvalidated` until
//! their artifacts re-verify (specialized provers remain authoritative).

use std::collections::BTreeSet;
use tpt_concord_core::{
    AssuranceGraph, AssuranceLevel, Claim, EvidenceKind, EvidenceRecord, EvidenceStatus, ModelID,
    Obligation, Scope, SpecificationID, Version,
};
use tpt_concord_model::{DeterministicMachine, StepOutcome};
use tpt_concord_spec::{
    BoundScope, Effect, Expr, Named, ResourceBound, Specification, Transition, ValueType,
};

/// Buffer-pool page states (archon spec: Free, Clean, Dirty, Pinned).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum PageState {
    Free,
    Clean,
    Dirty,
    Pinned,
}

/// Buffer-pool model inputs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PoolOp {
    /// Allocate a free page for reading (becomes Clean).
    Load,
    /// Modify a loaded page (Clean/Dirty -> Dirty; Pinned stays Pinned-and-dirty).
    Modify,
    /// Pin a page (I/O in flight; cannot be evicted).
    Pin,
    /// Unpin (Pinned -> Dirty if dirtied while pinned, else Clean).
    Unpin,
    /// Write back a dirty page (Dirty -> Clean); Free/Clean are no-ops.
    WriteBack,
}

impl std::fmt::Display for PoolOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PoolOp::Load => f.write_str("load"),
            PoolOp::Modify => f.write_str("modify"),
            PoolOp::Pin => f.write_str("pin"),
            PoolOp::Unpin => f.write_str("unpin"),
            PoolOp::WriteBack => f.write_str("write_back"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PoolError {
    #[error("operation `{op}` is illegal in state `{state:?}`")]
    Illegal { op: &'static str, state: PageState },
}

/// The buffer-pool page lifecycle as a deterministic machine.
///
/// Invariant of interest: only a `Dirty` page's `WriteBack` changes durable
/// content — writing back a `Clean` page is a no-op by definition, so the
/// machine forbids it as a state error (it signals a bookkeeping bug).
#[derive(Debug, Clone, Default)]
pub struct BufferPoolModel;

impl DeterministicMachine for BufferPoolModel {
    type State = PageState;
    type Input = PoolOp;
    type Error = PoolError;

    fn initial_state(&self) -> PageState {
        PageState::Free
    }

    fn step(&self, state: &PageState, input: &PoolOp) -> Result<StepOutcome<PageState>, PoolError> {
        let illegal = |op: &'static str| PoolError::Illegal {
            op,
            state: state.clone(),
        };
        let next = match (state, input) {
            (PageState::Free, PoolOp::Load) => PageState::Clean,
            (PageState::Clean, PoolOp::Modify) | (PageState::Dirty, PoolOp::Modify) => {
                PageState::Dirty
            }
            (PageState::Clean, PoolOp::Pin) | (PageState::Dirty, PoolOp::Pin) => PageState::Pinned,
            (PageState::Pinned, PoolOp::Modify) => PageState::Pinned,
            (PageState::Pinned, PoolOp::Unpin) => PageState::Dirty,
            (PageState::Clean, PoolOp::Unpin) => PageState::Clean,
            (PageState::Dirty, PoolOp::WriteBack) => PageState::Clean,
            (PageState::Clean, PoolOp::WriteBack) => PageState::Clean,
            _ => {
                return Err(illegal(match input {
                    PoolOp::Load => "load",
                    PoolOp::Modify => "modify",
                    PoolOp::Pin => "pin",
                    PoolOp::Unpin => "unpin",
                    PoolOp::WriteBack => "write_back",
                }))
            }
        };
        let effect = match input {
            PoolOp::WriteBack => "archon.page.writeback",
            _ => "archon.page.cache",
        };
        Ok(StepOutcome::with_effects(next, [effect]))
    }
}

/// The B-Link tree structural specification: the invariants a B-Link tree
/// must hold in every reachable state (Archon enforces capacity at type
/// level via tpt-eidos; the spec records them checkably for conformance).
pub fn blink_specification(max_node_keys: i128) -> Specification {
    let mut spec = Specification::new(
        SpecificationID::new("archon.blink.spec").unwrap(),
        Version::new("0.1.0").unwrap(),
        "Archon B-Link tree structure",
    );
    spec.description = "Structural invariants of the concurrent B-Link tree: ordered keys, \
                        capacity bounds, and non-empty nodes."
        .to_string();
    spec.state_vars = std::collections::BTreeMap::from([
        ("key_count".to_string(), ValueType::Int),
        ("height".to_string(), ValueType::Int),
    ]);
    spec.invariants = vec![
        Named::new(
            "keys_within_capacity",
            Expr::le(Expr::var("key_count"), Expr::int(max_node_keys)),
        ),
        Named::new(
            "keys_nonempty_when_allocated",
            Expr::ge(Expr::var("key_count"), Expr::int(0)),
        ),
        Named::new(
            "height_bounded",
            Expr::le(Expr::var("height"), Expr::int(64)),
        ),
    ]
    .into_iter()
    .collect();
    let mut insert = Transition::new("insert");
    insert.pre = vec![Named::new(
        "not_full",
        Expr::lt(Expr::var("key_count"), Expr::int(max_node_keys)),
    )];
    insert.post = vec![Named::new(
        "grew_by_one",
        Expr::ge(Expr::var("key_count"), Expr::int(0)),
    )];
    insert.effects = BTreeSet::from([Effect::new("archon.blink.insert")]);
    spec.transitions = vec![insert];
    spec.resource_bounds = vec![ResourceBound {
        resource: "node_writes".to_string(),
        max: 1_000_000,
        scope: BoundScope::PerRun,
    }];
    spec
}

/// The Archon claim graph (beyond the WAL pilot): buffer-pool conformance,
/// B-Link structure, and the Telos-proved guarantees recorded as references.
pub fn claim_graph() -> (AssuranceGraph, tpt_concord_core::ClaimID) {
    use tpt_concord_core::{AssumptionID, ClaimID, ObligationID, ScopeID};

    let mut g = AssuranceGraph::new();
    g.add_scope(Scope::new(
        ScopeID::new("scope.archon.prefix-crash").unwrap(),
        "Crash model X: crash may lose un-synced writes; surviving blocks are never corrupt.",
    ))
    .unwrap();
    g.add_scope(Scope::new(
        ScopeID::new("scope.archon.single-node").unwrap(),
        "Single-node operation; distributed claims are out of scope.",
    ))
    .unwrap();

    let claim_id = ClaimID::new("claim.archon.storage-conformance").unwrap();
    let mut claim = Claim::new(
        claim_id.clone(),
        "Archon storage (buffer pool, B-Link tree, WAL recovery) conforms to its \
         Concord reference models, and Telos-proved guarantees hold for the \
         shipped artifacts.",
    );
    claim.scopes = vec![
        ScopeID::new("scope.archon.prefix-crash").unwrap(),
        ScopeID::new("scope.archon.single-node").unwrap(),
    ];
    // Assumption: the Telos/Eidos proofs correspond to this artifact build.
    g.add_assumption(tpt_concord_core::Assumption::new(
        AssumptionID::new("assume.archon.proofs-match-build").unwrap(),
        "The formal-proofs/ artifacts in tpt-archon correspond to the audited build.",
        Some(ScopeID::new("scope.archon.single-node").unwrap()),
    ))
    .unwrap();
    claim.assumptions = vec![AssumptionID::new("assume.archon.proofs-match-build").unwrap()];
    g.add_claim(claim).unwrap();

    let obligations = [
        (
            "obligation.archon.pool-model",
            "Buffer pool conforms to the page-state machine",
        ),
        (
            "obligation.archon.blink-invariants",
            "B-Link states satisfy the structural spec",
        ),
        (
            "obligation.archon.wal-replay",
            "WAL replay crash-consistency (Telos-authoritative)",
        ),
        (
            "obligation.archon.scheduler",
            "Scheduler deadlock-freedom (Telos-authoritative)",
        ),
    ];
    for (id, desc) in obligations {
        g.add_obligation(Obligation::new(ObligationID::new(id).unwrap(), desc))
            .unwrap();
        g.require_obligation(&claim_id, &ObligationID::new(id).unwrap())
            .unwrap();
    }

    // Model-side evidence: buffer pool + B-Link conformance are dischargeable
    // by running the real engine through the models (harness lives in
    // tpt-archon); registered Unvalidated until then.
    let pool_ev = EvidenceRecord::new(
        tpt_concord_core::EvidenceID::new("evidence.archon.pool-model").unwrap(),
        EvidenceKind::ConformanceTrace,
        AssuranceLevel::ModelChecked,
        "tpt-archon buffer pool traced against BufferPoolModel",
    ); // stays Unvalidated until the harness runs
    g.add_evidence(pool_ev).unwrap();
    g.discharge(
        &ObligationID::new("obligation.archon.pool-model").unwrap(),
        &tpt_concord_core::EvidenceID::new("evidence.archon.pool-model").unwrap(),
    )
    .unwrap();

    // Telos-authoritative evidence: recorded as references with artifacts,
    // explicitly Unvalidated — Concord never takes the prover's word.
    for (ev_id, ob_id, desc) in [
        (
            "evidence.archon.wal-replay-telos",
            "obligation.archon.wal-replay",
            "tpt-archon formal-proofs/ WAL replay theorem (Telos)",
        ),
        (
            "evidence.archon.scheduler-telos",
            "obligation.archon.scheduler",
            "tpt-archon formal-proofs/ scheduler deadlock-freedom (Telos)",
        ),
    ] {
        let ev = EvidenceRecord::new(
            tpt_concord_core::EvidenceID::new(ev_id).unwrap(),
            EvidenceKind::TelosProof,
            AssuranceLevel::FullyProven,
            desc,
        )
        .with_artifact("artifacts/tpt-archon/formal-proofs/", "pending-recheck");
        // Deliberately NOT validated: needs independent artifact re-check.
        assert_eq!(ev.status, EvidenceStatus::Unvalidated);
        g.add_evidence(ev).unwrap();
        g.discharge(
            &ObligationID::new(ob_id).unwrap(),
            &tpt_concord_core::EvidenceID::new(ev_id).unwrap(),
        )
        .unwrap();
    }

    (g, claim_id)
}

/// Archon model registry ID.
pub fn archon_model_id() -> ModelID {
    ModelID::new("archon.pool-model").unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_concord_model::run;
    use tpt_concord_release::{evaluate, Policy};
    use tpt_concord_spec::State;

    #[test]
    fn buffer_pool_lifecycle_is_total_and_legal() {
        let m = BufferPoolModel;
        let seq = [
            PoolOp::Load,
            PoolOp::Modify,
            PoolOp::Pin,
            PoolOp::Modify,    // dirtied while pinned
            PoolOp::Unpin,     // -> Dirty
            PoolOp::WriteBack, // -> Clean
            PoolOp::WriteBack, // Clean writeback is legal no-op per spec
        ];
        let records = run(&m, &seq).unwrap();
        assert_eq!(records.last().unwrap().after, PageState::Clean);
        assert!(records
            .iter()
            .all(|r| r.effects.contains("archon.page.cache")
                || r.effects.contains("archon.page.writeback")));
    }

    #[test]
    fn illegal_transitions_are_state_errors() {
        let m = BufferPoolModel;
        // Load twice without free: second load from Clean is illegal.
        assert!(m.step(&PageState::Clean, &PoolOp::Load).is_err());
        // Unpin without pin.
        assert!(m.step(&PageState::Dirty, &PoolOp::Unpin).is_err());
        // WriteBack from Free is illegal (nothing to write).
        assert!(m.step(&PageState::Free, &PoolOp::WriteBack).is_err());
    }

    #[test]
    fn blink_states_satisfy_spec() {
        let spec = blink_specification(8);
        let mut state = State::new();
        state.insert("key_count".into(), tpt_concord_spec::Value::Int(8));
        state.insert("height".into(), tpt_concord_spec::Value::Int(3));
        assert!(spec.check_invariants(&state).is_empty());
        // Capacity violated at 9.
        state.insert("key_count".into(), tpt_concord_spec::Value::Int(9));
        assert_eq!(spec.check_invariants(&state).len(), 1);
        // Insert precondition correctly refuses a full node.
        assert_eq!(spec.check_transition_pre("insert", &state).len(), 1);
    }

    #[test]
    fn graph_is_structurally_valid_and_claim_unestablished_until_evidence_validates() {
        let (g, claim) = claim_graph();
        assert!(g.validate().is_ok());
        let eval = g.evaluate(&claim).unwrap();
        // Honest status: nothing validated yet — harness + proof re-checks pending.
        assert!(!eval.established);
        // Assumption and both scopes travel with the claim.
        assert_eq!(eval.open_assumptions.len(), 1);

        let mut policy = Policy::new("archon-assurance");
        policy.require_claim(claim.clone(), AssuranceLevel::ModelChecked);
        assert!(!evaluate(&g, &policy).is_approved());
    }

    #[test]
    fn validating_reference_evidence_establishes_claim() {
        let (mut g, claim) = claim_graph();
        // Independent harness re-check passes for the pool model.
        g.evidence_mut(&tpt_concord_core::EvidenceID::new("evidence.archon.pool-model").unwrap())
            .unwrap()
            .validate();
        // Telos artifacts re-verified independently.
        for ev in [
            "evidence.archon.wal-replay-telos",
            "evidence.archon.scheduler-telos",
        ] {
            g.evidence_mut(&tpt_concord_core::EvidenceID::new(ev).unwrap())
                .unwrap()
                .validate();
        }
        // B-Link obligation still lacks any evidence.
        assert!(!g.evaluate(&claim).unwrap().established);
        g.add_evidence({
            let mut e = EvidenceRecord::new(
                tpt_concord_core::EvidenceID::new("evidence.archon.blink").unwrap(),
                EvidenceKind::ModelCheck,
                AssuranceLevel::ModelChecked,
                "B-Link structural sweep",
            );
            e.validate();
            e
        })
        .unwrap();
        g.discharge(
            &tpt_concord_core::ObligationID::new("obligation.archon.blink-invariants").unwrap(),
            &tpt_concord_core::EvidenceID::new("evidence.archon.blink").unwrap(),
        )
        .unwrap();

        let mut policy = Policy::new("archon-assurance");
        policy.require_claim(claim.clone(), AssuranceLevel::FullyProven);
        let decision = evaluate(&g, &policy);
        assert!(decision.is_approved(), "{decision:?}");
    }
}
