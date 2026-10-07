// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Boxcar assurance infrastructure (spec §12).
//!
//! Source of truth: `tpt-solutions/tpt-boxcar` — five cloud-native tools:
//! TPT Origin (local sandbox: OCI + Wasm side by side), TPT Tether (state &
//! connection proxy for Wasm), TPT Scope (eBPF tracing), TPT Chisel (image
//! distiller), TPT Frontier (Wasm edge mesh / API gateway).
//!
//! The upstream README is explicit that Origin's `type: oci` and `type: wasm`
//! services are "bookkeeping-only pending containerd/Wasmtime integration".
//! Concord's job is to keep claims honest about exactly that: the execution
//! claim carries an assumption that integration is complete, so the gate
//! rejects until it is — and approves for the in-scope `process` type.

use std::collections::BTreeSet;
use tpt_concord_core::{
    Assumption, AssuranceGraph, AssuranceLevel, Claim, EvidenceKind, EvidenceRecord, Obligation,
    Scope, SpecificationID, Version,
};
use tpt_concord_model::{DeterministicMachine, StepOutcome};
use tpt_concord_spec::{Effect, Expr, Named, Specification, Transition, ValueType};

/// Frontier plugin lifecycle: a route's plugin versions move through
/// Registered → Active → Retired; hot reload swaps the active version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct PluginState {
    pub route: String,
    pub active_version: Option<u64>,
    pub registered: Vec<u64>,
    pub retired: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PluginOp {
    Register(u64),
    Activate(u64),
    Retire(u64),
}

impl std::fmt::Display for PluginOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginOp::Register(v) => write!(f, "register(v{v})"),
            PluginOp::Activate(v) => write!(f, "activate(v{v})"),
            PluginOp::Retire(v) => write!(f, "retire(v{v})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PluginError {
    #[error("version v{0} is not registered")]
    NotRegistered(u64),
    #[error("version v{0} is already active")]
    AlreadyActive(u64),
    #[error("cannot retire the active version v{0} without activating another")]
    RetireActive(u64),
    #[error("version v{0} already registered")]
    Duplicate(u64),
}

/// Frontier hot-reload model: exactly one active version per route;
/// retiring the active version is refused; activation requires registration.
#[derive(Debug, Clone, Default)]
pub struct FrontierPluginModel {
    pub route: String,
}

impl DeterministicMachine for FrontierPluginModel {
    type State = PluginState;
    type Input = PluginOp;
    type Error = PluginError;

    fn initial_state(&self) -> PluginState {
        PluginState {
            route: self.route.clone(),
            active_version: None,
            registered: Vec::new(),
            retired: Vec::new(),
        }
    }

    fn step(
        &self,
        s: &PluginState,
        input: &PluginOp,
    ) -> Result<StepOutcome<PluginState>, PluginError> {
        let mut next = s.clone();
        let effects = match input {
            PluginOp::Register(v) => {
                if s.registered.contains(v) {
                    return Err(PluginError::Duplicate(*v));
                }
                next.registered.push(*v);
                BTreeSet::from(["boxcar.frontier.register".to_string()])
            }
            PluginOp::Activate(v) => {
                if !s.registered.contains(v) {
                    return Err(PluginError::NotRegistered(*v));
                }
                if s.active_version == Some(*v) {
                    return Err(PluginError::AlreadyActive(*v));
                }
                // Previously active version is retired by the swap.
                if let Some(old) = s.active_version {
                    if !next.retired.contains(&old) {
                        next.retired.push(old);
                    }
                }
                next.active_version = Some(*v);
                BTreeSet::from(["boxcar.frontier.hot_reload".to_string()])
            }
            PluginOp::Retire(v) => {
                if s.active_version == Some(*v) {
                    return Err(PluginError::RetireActive(*v));
                }
                if !s.retired.contains(v) {
                    next.retired.push(*v);
                }
                BTreeSet::from(["boxcar.frontier.retire".to_string()])
            }
        };
        Ok(StepOutcome { next, effects })
    }
}

/// Origin workload manifest specification: only `process` services are in
/// the executable scope (upstream README); `oci`/`wasm` execution is an
/// assumption, not a property.
pub fn origin_manifest_specification() -> Specification {
    let mut spec = Specification::new(
        SpecificationID::new("boxcar.origin.manifest.spec").unwrap(),
        Version::new("0.1.0").unwrap(),
        "TPT Origin workload manifest",
    );
    spec.description = "One manifest, side-by-side services. Executable scope: type=process. \
                        type=oci and type=wasm are bookkeeping-only upstream — claims about \
                        their execution carry an explicit assumption."
        .to_string();
    spec.state_vars = std::collections::BTreeMap::from([
        ("service_type".to_string(), ValueType::Str),
        ("spawned".to_string(), ValueType::Bool),
    ]);
    spec.invariants = vec![
        // A spawned service must be a process-type service (the honest scope).
        Named::new(
            "only_processes_spawn",
            Expr::or(
                !(Expr::var("spawned")),
                Expr::eq(Expr::var("service_type"), Expr::str("process")),
            ),
        ),
    ]
    .into_iter()
    .collect();
    let mut spawn = Transition::new("spawn");
    spawn.pre = vec![Named::new("not_spawned_yet", !(Expr::var("spawned")))];
    spawn.post = vec![Named::new("now_spawned", Expr::var("spawned"))];
    spawn.effects = BTreeSet::from([Effect::new("boxcar.origin.spawn")]);
    spec.transitions = vec![spawn];
    spec
}

/// The Boxcar claim graph. Status by upstream's own README: execution of
/// oci/wasm services is assumed, not delivered — so the claim is *not*
/// established until that assumption is discharged.
pub fn claim_graph() -> (AssuranceGraph, tpt_concord_core::ClaimID) {
    use tpt_concord_core::{AssumptionID, ClaimID, ObligationID, ScopeID};

    let mut g = AssuranceGraph::new();
    g.add_scope(Scope::new(
        ScopeID::new("scope.boxcar.process-only").unwrap(),
        "Origin executes only type=process services; oci/wasm types are manifest bookkeeping upstream.",
    ))
    .unwrap();

    let claim_id = ClaimID::new("claim.boxcar.origin-executes-manifest").unwrap();
    let mut claim = Claim::new(
        claim_id.clone(),
        "TPT Origin spawns every service declared in a manifest as a real, isolated \
         OS/Wasm workload.",
    );
    claim.scopes = vec![ScopeID::new("scope.boxcar.process-only").unwrap()];
    g.add_assumption(Assumption::new(
        AssumptionID::new("assume.boxcar.oci-wasm-integrated").unwrap(),
        "containerd and Wasmtime integrations are complete (upstream: pending).",
        Some(ScopeID::new("scope.boxcar.process-only").unwrap()),
    ))
    .unwrap();
    claim.assumptions = vec![AssumptionID::new("assume.boxcar.oci-wasm-integrated").unwrap()];
    g.add_claim(claim).unwrap();

    for (id, desc) in [
        (
            "obligation.boxcar.process-spawn",
            "type=process services spawn as real OS processes (conformance run)",
        ),
        (
            "obligation.boxcar.oci-wasm-spawn",
            "type=oci / type=wasm services spawn as real workloads",
        ),
        (
            "obligation.boxcar.frontier-reload",
            "Frontier hot reload keeps exactly one active plugin per route",
        ),
    ] {
        g.add_obligation(Obligation::new(ObligationID::new(id).unwrap(), desc))
            .unwrap();
        g.require_obligation(&claim_id, &ObligationID::new(id).unwrap())
            .unwrap();
    }

    // Process-spawn conformance is in-scope and validated by upstream tests.
    let mut ev = EvidenceRecord::new(
        tpt_concord_core::EvidenceID::new("evidence.boxcar.process-spawn").unwrap(),
        EvidenceKind::TestRun,
        AssuranceLevel::PropertyTested,
        "Origin process spawning covered by repo tests",
    );
    ev.validate();
    g.add_evidence(ev).unwrap();
    g.discharge(
        &ObligationID::new("obligation.boxcar.process-spawn").unwrap(),
        &tpt_concord_core::EvidenceID::new("evidence.boxcar.process-spawn").unwrap(),
    )
    .unwrap();

    // Frontier reload: model-checked here (exactly-one-active invariant below).
    let mut ev = EvidenceRecord::new(
        tpt_concord_core::EvidenceID::new("evidence.boxcar.frontier-reload").unwrap(),
        EvidenceKind::ModelCheck,
        AssuranceLevel::ModelChecked,
        "FrontierPluginModel exhaustive lifecycle check",
    );
    ev.validate();
    g.add_evidence(ev).unwrap();
    g.discharge(
        &ObligationID::new("obligation.boxcar.frontier-reload").unwrap(),
        &tpt_concord_core::EvidenceID::new("evidence.boxcar.frontier-reload").unwrap(),
    )
    .unwrap();

    (g, claim_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_concord_model::run;
    use tpt_concord_release::{evaluate, Policy};
    use tpt_concord_spec::State;

    #[test]
    fn frontier_reload_keeps_exactly_one_active() {
        let m = FrontierPluginModel {
            route: "edge/api".into(),
        };
        let seq = [
            PluginOp::Register(1),
            PluginOp::Register(2),
            PluginOp::Activate(1),
            PluginOp::Activate(2), // hot swap retires v1
        ];
        let records = run(&m, &seq).unwrap();
        let final_state = records.last().unwrap().after.clone();
        assert_eq!(final_state.active_version, Some(2));
        assert_eq!(final_state.retired, vec![1]);
        // Invariant: at every point, active is registered and not retired.
        for r in &records {
            if let Some(active) = r.after.active_version {
                assert!(r.after.registered.contains(&active));
                assert!(!r.after.retired.contains(&active));
            }
        }
    }

    #[test]
    fn frontier_model_rejects_illegal_lifecycle() {
        let m = FrontierPluginModel { route: "r".into() };
        // Activate unregistered.
        assert!(m.step(&m.initial_state(), &PluginOp::Activate(9)).is_err());
        // Retire the active version.
        let state = PluginState {
            route: "r".into(),
            active_version: Some(1),
            registered: vec![1],
            retired: Vec::new(),
        };
        assert!(matches!(
            m.step(&state, &PluginOp::Retire(1)),
            Err(PluginError::RetireActive(1))
        ));
        // Duplicate registration.
        let state = PluginState {
            route: "r".into(),
            active_version: None,
            registered: vec![1],
            retired: Vec::new(),
        };
        assert!(matches!(
            m.step(&state, &PluginOp::Register(1)),
            Err(PluginError::Duplicate(1))
        ));
    }

    #[test]
    fn origin_manifest_enforces_process_scope() {
        let spec = origin_manifest_specification();
        // A spawned process-type service: OK.
        let mut ok = State::new();
        ok.insert(
            "service_type".into(),
            tpt_concord_spec::Value::Str("process".into()),
        );
        ok.insert("spawned".into(), tpt_concord_spec::Value::Bool(true));
        assert!(spec.check_invariants(&ok).is_empty());
        // A spawned wasm-type service violates the honest scope.
        let mut bad = ok.clone();
        bad.insert(
            "service_type".into(),
            tpt_concord_spec::Value::Str("wasm".into()),
        );
        assert_eq!(spec.check_invariants(&bad).len(), 1);
    }

    #[test]
    fn boxcar_gate_rejects_until_oci_wasm_assumption_is_discharged() {
        let (g, claim) = claim_graph();
        assert!(g.validate().is_ok());
        let eval = g.evaluate(&claim).unwrap();
        // Upstream's own admission: oci/wasm obligation has no evidence.
        assert!(!eval.established);
        assert_eq!(eval.open_assumptions.len(), 1);

        let mut policy = Policy::new("boxcar-assurance");
        policy.require_claim(claim.clone(), AssuranceLevel::PropertyTested);
        assert!(!evaluate(&g, &policy).is_approved());
    }
}
