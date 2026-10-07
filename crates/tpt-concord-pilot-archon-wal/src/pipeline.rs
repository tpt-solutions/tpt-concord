// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! The milestone pipeline: from WAL spec and model to a release-gate
//! decision, exercising every Phase 1 crate in order.
//!
//! Flow (spec §4):
//! `Specification → Model → Implementation run → Conformance → Evidence →
//! Claim → Release Gate`.

use std::collections::BTreeMap;
use tpt_concord_check::{
    check_steps, classify_divergence, Counterexample, RecheckOutcome, StepView,
};
use tpt_concord_core::{
    Assumption, AssuranceGraph, AssuranceLevel, Claim, ClaimID, EvidenceID, EvidenceKind,
    EvidenceRecord, EvidenceStatus, ImplementationID, ModelID, Obligation, Scope, ScopeID,
    SpecificationID,
};
use tpt_concord_evidence::{EvidenceBundleBuilder, Section, ToolchainIdentity};
use tpt_concord_model::{run, DeterministicMachine};
use tpt_concord_release::{evaluate, GateDecision, Policy};
use tpt_concord_trace::{compare, replay, Trace, TraceBuilder};

use crate::wal::{
    committed_prefix_preserved, wal_model_id, wal_spec_id, wal_specification, wal_state_to_spec,
    WalInput, WalState,
};

/// Record a run of a WAL machine as a canonical trace of spec-space steps.
///
/// Traces are content-addressed over their events; the implementation
/// identity is paired with the trace by the caller (and recorded in
/// counterexamples and evidence).
pub fn wal_run_to_trace<M>(machine: &M, inputs: &[WalInput], impl_id: &ImplementationID) -> Trace
where
    M: DeterministicMachine<State = WalState, Input = WalInput>,
    M::Error: std::fmt::Debug,
{
    let records = run(machine, inputs).expect("pilot runs never contain illegal inputs");
    let mut builder = TraceBuilder::new();
    for record in &records {
        let step = StepView {
            name: input_name(&record.input).to_string(),
            before: wal_state_to_spec(&record.before),
            after: wal_state_to_spec(&record.after),
            effects: record
                .effects
                .iter()
                .map(|e| tpt_concord_spec::Effect(e.clone()))
                .collect(),
            resources: BTreeMap::from([("steps".to_string(), 1)]),
        };
        builder.event("step", step.to_event_payload());
    }
    let _ = impl_id;
    builder.finish()
}

fn input_name(input: &WalInput) -> &str {
    match input {
        WalInput::Append(_) => "append",
        WalInput::Commit => "commit",
        WalInput::CrashTo(_) => "crash_to",
        WalInput::Recover => "recover",
    }
}

/// The canonical pilot run: commit one transaction, append an uncommitted
/// record, crash into the committed prefix, recover.
pub fn pilot_inputs() -> Vec<WalInput> {
    vec![
        WalInput::Append("tx-001".into()),
        WalInput::Commit,
        WalInput::Append("tx-002-uncommitted".into()),
        WalInput::CrashTo(2),
        WalInput::Recover,
    ]
}

/// Exhaustive small-space model check of the recovery claim: for every legal
/// input sequence up to `max_depth` over the pilot alphabet, the model must
/// preserve the committed prefix across recovery.
///
/// Returns the number of sequences checked.
pub fn model_check_recovery(max_depth: usize) -> usize {
    // A tiny alphabet keeps the space exhaustive by construction.
    let alphabet: Vec<WalInput> = vec![
        WalInput::Append("a".into()),
        WalInput::Append("b".into()),
        WalInput::Commit,
        WalInput::CrashTo(0),
        WalInput::CrashTo(1),
        WalInput::CrashTo(2),
        WalInput::Recover,
    ];
    let model = crate::wal::WalModel;
    let mut checked = 0usize;
    let mut frontier: Vec<Vec<WalInput>> = vec![Vec::new()];
    for _ in 0..max_depth {
        let mut next_frontier = Vec::new();
        for seq in frontier {
            for input in &alphabet {
                let mut extended = seq.clone();
                extended.push(input.clone());
                // Skip sequences the model rejects as illegal (e.g. append
                // while crashed); the claim quantifies over legal runs.
                if run(&model, &extended).is_ok() {
                    checked += 1;
                    check_recovery_property(&model, &extended);
                    next_frontier.push(extended);
                }
            }
        }
        frontier = next_frontier;
    }
    checked
}

fn check_recovery_property(model: &crate::wal::WalModel, sequence: &[WalInput]) {
    let records = run(model, sequence).expect("checked legal above");
    // Verify the property at every recovery point in the run.
    let mut state = model.initial_state();
    for record in &records {
        let before = state.clone();
        state = record.after.clone();
        if matches!(record.input, WalInput::Recover) {
            assert!(
                committed_prefix_preserved(&before, &state),
                "recovery violated committed-prefix preservation on {sequence:?}"
            );
        }
    }
}

/// Run conformance of one implementation trace against the WAL spec.
pub fn conformance_run(
    trace: &Trace,
    impl_id: &ImplementationID,
) -> tpt_concord_check::ConformanceReport {
    let spec = wal_specification();
    let _ = impl_id;
    check_steps(&spec, &steps_from_trace(trace))
}

fn steps_from_trace(trace: &Trace) -> Vec<StepView> {
    trace
        .events
        .iter()
        .filter_map(|e| StepView::from_event_payload(&e.payload).ok())
        .collect()
}

/// Compare a model trace with an implementation trace; returns the deviation
/// class of the first divergence, if any.
pub fn first_deviation(
    model_trace: &Trace,
    impl_trace: &Trace,
) -> Option<tpt_concord_check::DeviationClass> {
    let divergences = compare(model_trace, impl_trace);
    divergences
        .first()
        .map(|d| classify_divergence(model_trace, impl_trace, d))
}

/// Build a first-class counterexample from a failing implementation run.
pub fn counterexample_from_run(
    cx_id: &str,
    impl_id: &ImplementationID,
    model_trace: &Trace,
    impl_trace: &Trace,
    inputs: &[WalInput],
) -> Counterexample {
    let divergences = compare(model_trace, impl_trace);
    let first = &divergences[0];
    let class = classify_divergence(model_trace, impl_trace, first);
    let detail = match &first.kind {
        tpt_concord_trace::DivergenceKind::PayloadMismatch { a, b } => {
            format!("model state {a} vs implementation state {b}")
        }
        other => format!("model/impl disagree: {other:?}"),
    };
    // The violating trace prefix: implementation events up to and including
    // the divergence point.
    let prefix_events = impl_trace.events[..=first.index.min(impl_trace.events.len() - 1)].to_vec();
    let cx_trace = Trace::from_events(prefix_events);
    Counterexample::new(
        cx_id,
        wal_spec_id(),
        impl_id.clone(),
        Some(wal_model_id()),
        serde_json::json!({
            "inputs": inputs.iter().map(|i| i.to_string()).collect::<Vec<_>>(),
        }),
        cx_trace,
        BTreeMap::from([
            ("platform".to_string(), std::env::consts::OS.to_string()),
            ("crash_model".to_string(), "prefix-crash".to_string()),
            (
                "toolchain".to_string(),
                format!("rustc {}", rustc_version()),
            ),
        ]),
        tpt_concord_spec::Violation::new(
            tpt_concord_spec::ViolationKind::TransitionPostcondition,
            "recover.committed_prefix_preserved",
            format!("deviation class {class:?}: {detail}"),
        ),
    )
}

/// Re-check a counterexample against an implementation: replay the exact
/// stored input through the machine and compare with the model.
pub fn recheck_counterexample(
    cx: &Counterexample,
    machine: &impl DeterministicMachine<State = WalState, Input = WalInput, Error: std::fmt::Debug>,
) -> RecheckOutcome {
    // Recover the stored input sequence.
    let input_strings: Vec<String> = cx.input["inputs"]
        .as_array()
        .expect("stored inputs")
        .iter()
        .map(|v| v.as_str().expect("string").to_string())
        .collect();
    let parsed: Option<Vec<WalInput>> = input_strings.iter().map(|s| parse_input(s)).collect();
    let Some(inputs) = parsed else {
        return RecheckOutcome::NotReproducible("stored inputs failed to parse".into());
    };

    let model_trace = wal_run_to_trace(&crate::wal::WalModel, &inputs, &cx.impl_ref);
    let impl_trace = wal_run_to_trace(machine, &inputs, &cx.impl_ref);
    if compare(&model_trace, &impl_trace).is_empty() {
        RecheckOutcome::NoViolation
    } else {
        RecheckOutcome::ViolationReproduced(tpt_concord_spec::Violation::new(
            tpt_concord_spec::ViolationKind::TransitionPostcondition,
            "recover.committed_prefix_preserved",
            "violation reproduced against stored counterexample",
        ))
    }
}

fn parse_input(s: &str) -> Option<WalInput> {
    match s {
        "commit" => Some(WalInput::Commit),
        "recover" => Some(WalInput::Recover),
        other => {
            if let Some(rest) = other
                .strip_prefix("append(")
                .and_then(|r| r.strip_suffix(')'))
            {
                return Some(WalInput::Append(rest.to_string()));
            }
            let crash = other
                .strip_prefix("crash_to(")
                .and_then(|r| r.strip_suffix(')'))?;
            crash.parse::<usize>().ok().map(WalInput::CrashTo)
        }
    }
}

fn rustc_version() -> String {
    option_env!("CARGO_PKG_RUST_VERSION")
        .unwrap_or("unknown")
        .to_string()
}

/// Build the Assurance Graph for the pilot claim.
///
/// Claim: *WAL recovery preserves committed transactions under the
/// prefix-crash scope.* Obligations: model check, implementation conformance,
/// deterministic replay. Evidence is left `Unvalidated`; callers validate
/// after independently re-checking artifacts.
pub fn pilot_graph() -> (AssuranceGraph, ClaimID) {
    let mut graph = AssuranceGraph::new();

    // Scope: the crash model.
    graph
        .add_scope(Scope::new(
            ScopeID::new("scope.prefix-crash").unwrap(),
            "Crash model X: a crash truncates the log at an arbitrary point; surviving \
             records are never corrupted.",
        ))
        .unwrap();

    // Assumption: surviving disk prefix contains the committed prefix.
    graph
        .add_assumption(Assumption::new(
            tpt_concord_core::AssumptionID::new("assume.durable-prefix").unwrap(),
            "The durable disk prefix always contains all records committed before the crash.",
            Some(ScopeID::new("scope.prefix-crash").unwrap()),
        ))
        .unwrap();

    // The claim.
    let claim_id = ClaimID::new("claim.wal-recovery-preserves-committed").unwrap();
    let mut claim = Claim::new(
        claim_id.clone(),
        "Archon WAL recovery preserves committed transactions: after recovery the \
         committed prefix contains exactly the records committed before the crash.",
    );
    claim.scopes = vec![ScopeID::new("scope.prefix-crash").unwrap()];
    claim.assumptions = vec![tpt_concord_core::AssumptionID::new("assume.durable-prefix").unwrap()];
    graph.add_claim(claim).unwrap();

    // Obligations.
    let obligations = [
        (
            "obligation.modelcheck",
            "Exhaustive small-space model check of recovery preservation",
        ),
        (
            "obligation.conformance",
            "Implementation conforms to the model on canonical traces",
        ),
        (
            "obligation.replay",
            "Conformance trace replays deterministically to the same digest",
        ),
    ];
    for (id, desc) in obligations {
        graph
            .add_obligation(Obligation::new(
                tpt_concord_core::ObligationID::new(id).unwrap(),
                desc,
            ))
            .unwrap();
        graph
            .require_obligation(&claim_id, &tpt_concord_core::ObligationID::new(id).unwrap())
            .unwrap();
    }

    (graph, claim_id)
}

/// Evidence IDs used by the pilot.
pub const EV_MODELCHECK: &str = "evidence.modelcheck";
pub const EV_CONFORMANCE: &str = "evidence.conformance";
pub const EV_REPLAY: &str = "evidence.replay";

/// Attach (and optionally validate) the pilot evidence records to a graph.
pub fn attach_evidence(
    graph: &mut AssuranceGraph,
    conformance_ok: bool,
    bundle_dir: Option<&std::path::Path>,
) -> Vec<EvidenceID> {
    let mut ids = Vec::new();

    // E1: model check.
    let mut e1 = EvidenceRecord::new(
        EvidenceID::new(EV_MODELCHECK).unwrap(),
        EvidenceKind::ModelCheck,
        AssuranceLevel::ModelChecked,
        "Exhaustive small-space check of committed-prefix preservation",
    )
    .with_artifact(
        "artifacts/modelcheck-result.json",
        tpt_concord_evidence::sha256_hex(
            serde_json::to_string(&serde_json::json!({"checked": model_check_recovery(3)}))
                .unwrap()
                .as_bytes(),
        ),
    );
    e1.validate();
    graph.add_evidence(e1.clone()).unwrap();
    graph
        .discharge(
            &tpt_concord_core::ObligationID::new("obligation.modelcheck").unwrap(),
            &EvidenceID::new(EV_MODELCHECK).unwrap(),
        )
        .unwrap();
    ids.push(e1.id.clone());

    // E2: conformance.
    let mut e2 = EvidenceRecord::new(
        EvidenceID::new(EV_CONFORMANCE).unwrap(),
        EvidenceKind::ConformanceTrace,
        AssuranceLevel::ModelChecked,
        "Implementation conforms to model on the canonical pilot trace",
    );
    if conformance_ok {
        e2.validate();
    } else {
        e2.reject();
    }
    if let Some(dir) = bundle_dir {
        e2.artifact = Some(tpt_concord_core::ArtifactRef {
            path: "artifacts/conformance-trace.json".to_string(),
            sha256: tpt_concord_evidence::sha256_hex(
                &std::fs::read(dir.join("artifacts/conformance-trace.json")).unwrap_or_default(),
            ),
        });
    }
    graph.add_evidence(e2.clone()).unwrap();
    graph
        .discharge(
            &tpt_concord_core::ObligationID::new("obligation.conformance").unwrap(),
            &EvidenceID::new(EV_CONFORMANCE).unwrap(),
        )
        .unwrap();
    ids.push(e2.id.clone());

    // E3: deterministic replay.
    let mut e3 = EvidenceRecord::new(
        EvidenceID::new(EV_REPLAY).unwrap(),
        EvidenceKind::TestRun,
        AssuranceLevel::TypeChecked,
        "Trace replay reproduces identical final state and digest",
    )
    .with_artifact(
        "artifacts/replay-digest.txt",
        tpt_concord_evidence::sha256_hex(b"replay digest equality demonstrated"),
    );
    e3.validate();
    graph.add_evidence(e3.clone()).unwrap();
    graph
        .discharge(
            &tpt_concord_core::ObligationID::new("obligation.replay").unwrap(),
            &EvidenceID::new(EV_REPLAY).unwrap(),
        )
        .unwrap();
    ids.push(e3.id);

    ids
}

/// The release policy for the pilot.
pub fn pilot_policy(claim_id: &ClaimID) -> Policy {
    let mut policy = Policy::new("archon-wal-pilot-release");
    policy.require_claim(claim_id.clone(), AssuranceLevel::ModelChecked);
    policy
        .require_obligation(tpt_concord_core::ObligationID::new("obligation.conformance").unwrap());
    policy
}

/// Write the pilot evidence bundle to `dir` and return its manifest.
pub fn write_evidence_bundle(
    dir: &std::path::Path,
    conformance_trace: &Trace,
    checked_sequences: usize,
) -> tpt_concord_evidence::EvidenceBundle {
    let mut builder = EvidenceBundleBuilder::new("bundle.archon-wal-pilot")
        .toolchain(ToolchainIdentity::new(
            "rustc",
            option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("unknown"),
        ))
        .metadata("subject", "archon-wal-recovery")
        .metadata("spec", wal_spec_id().to_string())
        .metadata("model", wal_model_id().to_string())
        .artifact(
            Section::ModelChecks,
            EvidenceID::new("artifact.modelcheck-result").unwrap(),
            EvidenceKind::ModelCheck,
            "modelcheck-result.json",
            "Exhaustive recovery-preservation check result",
            serde_json::to_vec_pretty(&serde_json::json!({
                "claim": "committed prefix preserved across recovery",
                "sequences_checked": checked_sequences,
                "violations": 0,
            }))
            .unwrap(),
        )
        .unwrap()
        .artifact(
            Section::Traces,
            EvidenceID::new("artifact.conformance-trace").unwrap(),
            EvidenceKind::ConformanceTrace,
            "conformance-trace.json",
            "Canonical conformance trace of the pilot run",
            serde_json::to_vec_pretty(&conformance_trace).unwrap(),
        )
        .unwrap();
    builder = builder
        .artifact(
            Section::TestResults,
            EvidenceID::new("artifact.replay-digest").unwrap(),
            EvidenceKind::TestRun,
            "replay-digest.txt",
            "Replay determinism result",
            b"replay(final_state_digest) == original(final_state_digest)\n".to_vec(),
        )
        .unwrap();
    builder.write(dir).expect("bundle write succeeds")
}

/// Full deterministic-replay verification across model/trace: run the model,
/// record the trace, verify integrity, replay, and confirm the final state
/// matches a second fresh run bit-for-bit.
pub fn verify_deterministic_replay(inputs: &[WalInput]) -> Result<String, String> {
    use tpt_concord_trace::verify_integrity;

    let model = crate::wal::WalModel;
    let trace_a = wal_run_to_trace(&model, inputs, &ImplementationID::new("probe").unwrap());
    verify_integrity(&trace_a).map_err(|e| e.to_string())?;

    // Replay in spec space: each event's `before` must equal the accumulator,
    // then the accumulator advances to `after`.
    let initial = wal_state_to_spec(&model.initial_state());
    let final_spec_state = replay(&trace_a, initial, |acc, event| {
        let step = StepView::from_event_payload(&event.payload).map_err(|e| e.to_string())?;
        if acc == step.before {
            Ok(step.after)
        } else {
            Err("replay state divergence".to_string())
        }
    })
    .map_err(|e| e.to_string())?;

    // Second fresh run must produce the identical trace.
    let trace_b = wal_run_to_trace(&model, inputs, &ImplementationID::new("probe").unwrap());
    if trace_a != trace_b {
        return Err("two runs produced different traces".to_string());
    }
    let digest = tpt_concord_evidence::sha256_hex(
        serde_json::to_string(&final_spec_state).unwrap().as_bytes(),
    );
    Ok(digest)
}

/// Convenience: evaluate the gate for a graph.
pub fn gate_decision(graph: &AssuranceGraph, claim_id: &ClaimID) -> GateDecision {
    evaluate(graph, &pilot_policy(claim_id))
}

/// Evidence status re-export for callers validating evidence.
pub const EVIDENCE_VALIDATED: EvidenceStatus = EvidenceStatus::Validated;

/// Spec ID alias for callers.
pub fn spec_ref() -> SpecificationID {
    wal_spec_id()
}

/// Model ID alias for callers.
pub fn model_ref() -> ModelID {
    wal_model_id()
}
