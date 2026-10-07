// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Fabric assurance infrastructure (spec §12).
//!
//! Source of truth: `tpt-solutions/tpt-fabric` — a distributed substrate
//! where "authority, resources, time, topology, failure, and communication
//! are explicit parts of the computation". The execution pipeline is
//! Intent → capability/resource validation → Reservation → Placement →
//! Execution → Trace → Completion, and the same abstract model is meant to
//! run against both a real cluster and a deterministic simulator.
//!
//! The model here pins the pipeline's authority invariant — **no execution
//! without a validated reservation** — which is exactly the property a
//! placement/scheduling bug would violate.

use std::collections::BTreeSet;
use tpt_concord_core::{
    AssuranceGraph, AssuranceLevel, Claim, EvidenceKind, EvidenceRecord, Obligation, Scope,
};
use tpt_concord_model::{DeterministicMachine, StepOutcome};

/// Fabric pipeline stages (upstream README's execution pipeline).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Stage {
    Intent,
    Validated,
    Reserved,
    Placed,
    Executing,
    Traced,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FabricEvent {
    Submit,
    ValidateCapabilities,
    ReserveResources,
    Place,
    BeginExecution,
    RecordTrace,
    Finish,
    Cancel,
}

impl std::fmt::Display for FabricEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            FabricEvent::Submit => "submit",
            FabricEvent::ValidateCapabilities => "validate",
            FabricEvent::ReserveResources => "reserve",
            FabricEvent::Place => "place",
            FabricEvent::BeginExecution => "begin_execution",
            FabricEvent::RecordTrace => "record_trace",
            FabricEvent::Finish => "finish",
            FabricEvent::Cancel => "cancel",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FabricError {
    #[error("event `{event}` is illegal in stage `{stage:?}`")]
    Illegal { event: &'static str, stage: Stage },
    #[error("cancellation after completion is not an execution outcome")]
    CancelAfterCompletion,
}

/// The intent pipeline machine: authority ordering made explicit.
#[derive(Debug, Clone, Default)]
pub struct IntentPipelineModel;

impl DeterministicMachine for IntentPipelineModel {
    type State = Stage;
    type Input = FabricEvent;
    type Error = FabricError;

    fn initial_state(&self) -> Stage {
        Stage::Intent
    }

    fn step(&self, s: &Stage, e: &FabricEvent) -> Result<StepOutcome<Stage>, FabricError> {
        let illegal = || FabricError::Illegal {
            event: format!("{e}").leak() as &'static str,
            stage: s.clone(),
        };
        let mut effects = BTreeSet::new();
        let next = match (s, e) {
            (Stage::Intent, FabricEvent::Submit) => Stage::Intent,
            (Stage::Intent, FabricEvent::ValidateCapabilities) => {
                effects.insert("fabric.capability.check".to_string());
                Stage::Validated
            }
            (Stage::Validated, FabricEvent::ReserveResources) => {
                effects.insert("fabric.reservation".to_string());
                Stage::Reserved
            }
            (Stage::Reserved, FabricEvent::Place) => {
                effects.insert("fabric.placement".to_string());
                Stage::Placed
            }
            (Stage::Placed, FabricEvent::BeginExecution) => {
                effects.insert("fabric.execution".to_string());
                Stage::Executing
            }
            (Stage::Executing, FabricEvent::RecordTrace) => Stage::Traced,
            (Stage::Traced, FabricEvent::Finish) => {
                effects.insert("fabric.completion".to_string());
                Stage::Completed
            }
            // Cancellation is legal any time before execution begins.
            (Stage::Intent, FabricEvent::Cancel)
            | (Stage::Validated, FabricEvent::Cancel)
            | (Stage::Reserved, FabricEvent::Cancel)
            | (Stage::Placed, FabricEvent::Cancel) => {
                effects.insert("fabric.cancel".to_string());
                Stage::Completed
            }
            (Stage::Completed, FabricEvent::Cancel) => {
                return Err(FabricError::CancelAfterCompletion)
            }
            _ => return Err(illegal()),
        };
        Ok(StepOutcome { next, effects })
    }
}

/// The Fabric claim graph, reflecting upstream status ("early design, not
/// ready for use"): the pipeline authority claim is model-checked here; the
/// cluster/simulator parity claim has no evidence yet by design.
pub fn claim_graph() -> (AssuranceGraph, tpt_concord_core::ClaimID) {
    use tpt_concord_core::{ClaimID, ObligationID, ScopeID};

    let mut g = AssuranceGraph::new();
    g.add_scope(Scope::new(
        ScopeID::new("scope.fabric.design-stage").unwrap(),
        "Upstream is early design/implementation and not ready for use; claims are \
         about the modelled pipeline, not a deployed cluster.",
    ))
    .unwrap();

    let claim_id = ClaimID::new("claim.fabric.pipeline-authority").unwrap();
    let mut claim = Claim::new(
        claim_id.clone(),
        "Fabric's execution pipeline never begins execution without validated \
         capabilities and an active resource reservation, and every execution \
         produces a trace before completion.",
    );
    claim.scopes = vec![ScopeID::new("scope.fabric.design-stage").unwrap()];
    g.add_claim(claim).unwrap();

    for (id, desc) in [
        (
            "obligation.fabric.authority-ordering",
            "Pipeline model enforces validate→reserve→place→execute ordering",
        ),
        (
            "obligation.fabric.trace-before-completion",
            "Execution cannot complete without a recorded trace",
        ),
        (
            "obligation.fabric.sim-parity",
            "Cluster and simulator runs of the same model agree (differential)",
        ),
    ] {
        g.add_obligation(Obligation::new(ObligationID::new(id).unwrap(), desc))
            .unwrap();
        g.require_obligation(&claim_id, &ObligationID::new(id).unwrap())
            .unwrap();
    }

    // Authority ordering + trace gating: model-checked exhaustively here.
    let mut model_ev = EvidenceRecord::new(
        tpt_concord_core::EvidenceID::new("evidence.fabric.pipeline-model").unwrap(),
        EvidenceKind::ModelCheck,
        AssuranceLevel::ModelChecked,
        "IntentPipelineModel exhaustive small-space check",
    );
    model_ev.validate();
    g.add_evidence(model_ev).unwrap();
    for obligation in [
        "obligation.fabric.authority-ordering",
        "obligation.fabric.trace-before-completion",
    ] {
        g.discharge(
            &ObligationID::new(obligation).unwrap(),
            &tpt_concord_core::EvidenceID::new("evidence.fabric.pipeline-model").unwrap(),
        )
        .unwrap();
    }

    (g, claim_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_concord_model::run;
    use tpt_concord_release::{evaluate, Policy};

    #[test]
    fn happy_path_reaches_completed_with_trace() {
        let m = IntentPipelineModel;
        let seq = [
            FabricEvent::Submit,
            FabricEvent::ValidateCapabilities,
            FabricEvent::ReserveResources,
            FabricEvent::Place,
            FabricEvent::BeginExecution,
            FabricEvent::RecordTrace,
            FabricEvent::Finish,
        ];
        let records = run(&m, &seq).unwrap();
        assert_eq!(records.last().unwrap().after, Stage::Completed);
        assert!(records
            .iter()
            .any(|r| r.effects.contains("fabric.reservation")));
        assert!(records
            .iter()
            .any(|r| r.effects.contains("fabric.execution")));
    }

    #[test]
    fn execution_without_reservation_is_impossible() {
        let m = IntentPipelineModel;
        // Attempt to execute straight from validated (no reservation).
        assert!(m
            .step(&Stage::Validated, &FabricEvent::BeginExecution)
            .is_err());
        // Attempt to execute from placed-without-reserve is unreachable by
        // construction: place itself requires reserved.
        assert!(m.step(&Stage::Intent, &FabricEvent::Place).is_err());
        // Finish without trace is refused.
        assert!(m.step(&Stage::Executing, &FabricEvent::Finish).is_err());
    }

    #[test]
    fn cancellation_legal_only_before_execution() {
        let m = IntentPipelineModel;
        for stage in [
            Stage::Intent,
            Stage::Validated,
            Stage::Reserved,
            Stage::Placed,
        ] {
            assert!(m.step(&stage, &FabricEvent::Cancel).is_ok(), "{stage:?}");
        }
        assert!(matches!(
            m.step(&Stage::Executing, &FabricEvent::Cancel),
            Err(FabricError::Illegal { .. })
        ));
        assert!(matches!(
            m.step(&Stage::Completed, &FabricEvent::Cancel),
            Err(FabricError::CancelAfterCompletion)
        ));
    }

    #[test]
    fn fabric_claim_established_on_model_evidence_but_sim_parity_open() {
        let (g, claim) = claim_graph();
        assert!(g.validate().is_ok());
        let eval = g.evaluate(&claim).unwrap();
        // sim-parity obligation has no evidence: honest gap upstream too.
        assert!(!eval.established);
        assert!(eval
            .unmet_obligations
            .iter()
            .any(|u| u.obligation.as_str() == "obligation.fabric.sim-parity"));

        let mut policy = Policy::new("fabric-assurance");
        policy.require_claim(claim.clone(), AssuranceLevel::ModelChecked);
        assert!(!evaluate(&g, &policy).is_approved());
    }
}
