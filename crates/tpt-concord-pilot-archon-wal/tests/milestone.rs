// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Milestone integration tests (spec §11): deterministic replay end-to-end
//! across core/model/trace, and the Archon WAL pilot claim + Assurance Graph
//! driving a release-gate decision — including the counterexample lifecycle
//! for a broken implementation.

use std::collections::BTreeMap;
use tpt_concord_check::{classify_divergence, DeviationClass, RecheckOutcome};
use tpt_concord_core::{AssuranceLevel, EvidenceID, EvidenceStatus, ImplementationID};
use tpt_concord_pilot_archon_wal::pipeline::verify_deterministic_replay;
use tpt_concord_pilot_archon_wal::pipeline::{
    attach_evidence, conformance_run, counterexample_from_run, first_deviation, gate_decision,
    model_check_recovery, pilot_graph, pilot_inputs, recheck_counterexample, wal_run_to_trace,
    write_evidence_bundle,
};
use tpt_concord_pilot_archon_wal::wal::{ConformantWal, NaiveWal};

const IMPL_OK: &str = "impl.conformant-wal";
const IMPL_BROKEN: &str = "impl.naive-wal";

#[test]
fn deterministic_replay_end_to_end_across_core_model_trace() {
    let digest = verify_deterministic_replay(&pilot_inputs()).expect("replay succeeds");
    assert_eq!(digest.len(), 64);
    // Same inputs, same digest — determinism across the whole stack.
    let digest_again = verify_deterministic_replay(&pilot_inputs()).unwrap();
    assert_eq!(digest, digest_again);
}

#[test]
fn exhaustive_model_check_finds_no_violations() {
    // depth 5 over the 7-input alphabet sweeps every legal sequence.
    let checked = model_check_recovery(5);
    assert!(
        checked > 500,
        "expected an exhaustive sweep, checked {checked}"
    );
}

#[test]
fn milestone_gate_approves_conformant_implementation() {
    let inputs = pilot_inputs();

    // Reference and implementation traces.
    let model_trace = wal_run_to_trace(
        &tpt_concord_pilot_archon_wal::wal::WalModel,
        &inputs,
        &impl_ok(),
    );
    let impl_trace = wal_run_to_trace(&ConformantWal, &inputs, &impl_ok());

    // Implementation must be trace-identical to the model.
    assert_eq!(first_deviation(&model_trace, &impl_trace), None);

    // And must satisfy the specification on its own terms.
    let report = conformance_run(&impl_trace, &impl_ok());
    assert!(
        report.conforms,
        "conformant impl violated spec: {:?}",
        report.violations
    );

    // Evidence bundle on disk (hash-verified).
    let tmp = std::env::temp_dir().join(format!("concord-milestone-{}", std::process::id()));
    let bundle_dir = tmp.join("bundle-ok");
    let _ = std::fs::remove_dir_all(&bundle_dir);
    let checked = model_check_recovery(3);
    write_evidence_bundle(&bundle_dir, &model_trace, checked);
    let bundle = tpt_concord_evidence::bundle::read_and_verify(&bundle_dir)
        .expect("bundle verifies against its hashes");
    assert_eq!(bundle.traces.len(), 1);

    // Assurance Graph: claim + obligations + validated evidence.
    let (mut graph, claim_id) = pilot_graph();
    attach_evidence(&mut graph, report.conforms, Some(&bundle_dir));
    assert!(graph.validate().is_ok());

    // Every piece of evidence the pilot attaches starts out validated
    // (the caller validates after re-checking; see the broken-impl test).
    assert!(graph
        .evidence_records()
        .all(|e| e.status == EvidenceStatus::Validated));

    let claim_eval = graph.evaluate(&claim_id).unwrap();
    assert!(
        claim_eval.established,
        "claim must be established: {claim_eval:?}"
    );
    assert!(claim_eval
        .achieved_level
        .satisfies(AssuranceLevel::ModelChecked));
    // The claim rests on one recorded assumption and one scope.
    assert_eq!(claim_eval.open_assumptions.len(), 1);

    // Release gate approves.
    let decision = gate_decision(&graph, &claim_id);
    assert!(decision.is_approved(), "gate rejected: {decision:?}");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn broken_impl_is_rejected_and_counterexample_resolves_after_fix() {
    let inputs = pilot_inputs();

    let model_trace = wal_run_to_trace(
        &tpt_concord_pilot_archon_wal::wal::WalModel,
        &inputs,
        &impl_broken(),
    );
    let impl_trace = wal_run_to_trace(&NaiveWal, &inputs, &impl_broken());

    // The deviation is detected and classified: the impl marks uncommitted
    // records as committed — a value mismatch on the recover step.
    let deviation = first_deviation(&model_trace, &impl_trace);
    assert_eq!(deviation, Some(DeviationClass::ValueMismatch));

    // Spec-level conformance catches it too (watermark vs log invariants hold,
    // but the recorded post-state diverges from the spec-space expectation —
    // conformance alone cannot know the model; the spec checks still pass, the
    // trace comparison is the decisive evidence).
    let report = conformance_run(&impl_trace, &impl_broken());

    // Counterexample becomes a first-class object.
    let mut cx = counterexample_from_run(
        "cx-0001",
        &impl_broken(),
        &model_trace,
        &impl_trace,
        &inputs,
    );
    assert_eq!(cx.status, tpt_concord_check::CounterexampleStatus::Open);
    assert!(tpt_concord_trace::verify_integrity(&cx.trace).is_ok());
    assert_eq!(cx.environment["crash_model"], "prefix-crash");

    // Counterexample serializes losslessly for evidence storage.
    let json = cx.to_json().unwrap();
    assert_eq!(
        tpt_concord_check::Counterexample::from_json(&json).unwrap(),
        cx
    );

    // Graph with rejected conformance evidence (the impl failed).
    let (mut graph, claim_id) = pilot_graph();
    attach_evidence(&mut graph, report.conforms, None);
    let conformance_ev = graph
        .evidence(&EvidenceID::new("evidence.conformance").unwrap())
        .unwrap()
        .clone();
    assert_eq!(conformance_ev.status, EvidenceStatus::Rejected);
    assert!(!graph.evaluate(&claim_id).unwrap().established);

    // The gate refuses release and says why.
    let decision = gate_decision(&graph, &claim_id);
    assert!(!decision.is_approved());

    // Re-check workflow: the fixed implementation against the exact
    // counterexample → resolved.
    let status = cx.recheck(|stored| recheck_counterexample(stored, &ConformantWal));
    assert_eq!(status, tpt_concord_check::CounterexampleStatus::Resolved);
    assert!(
        matches!(
            recheck_counterexample(&cx, &NaiveWal),
            RecheckOutcome::ViolationReproduced(_)
        ),
        "the bug must still reproduce against the broken impl"
    );

    // Rebuilt graph with conformant evidence now approves.
    let (mut graph_fixed, claim_id) = pilot_graph();
    attach_evidence(&mut graph_fixed, true, None);
    assert!(gate_decision(&graph_fixed, &claim_id).is_approved());

    // Sanity: classification helper agrees the first divergence is a value
    // mismatch (used by the counterexample builder).
    let divs = tpt_concord_trace::compare(&model_trace, &impl_trace);
    assert_eq!(
        classify_divergence(&model_trace, &impl_trace, &divs[0]),
        DeviationClass::ValueMismatch
    );
}

fn impl_ok() -> ImplementationID {
    ImplementationID::new(IMPL_OK).unwrap()
}

fn impl_broken() -> ImplementationID {
    ImplementationID::new(IMPL_BROKEN).unwrap()
}

#[test]
fn counterexample_environment_is_complete() {
    let inputs = pilot_inputs();
    let model_trace = wal_run_to_trace(
        &tpt_concord_pilot_archon_wal::wal::WalModel,
        &inputs,
        &impl_broken(),
    );
    let impl_trace = wal_run_to_trace(&NaiveWal, &inputs, &impl_broken());
    let cx = counterexample_from_run("cx-env", &impl_broken(), &model_trace, &impl_trace, &inputs);

    // §9: specification, implementation, input/state, trace, environment,
    // observed violation — all present.
    assert_eq!(cx.spec_ref.as_str(), "archon.wal.spec");
    assert_eq!(cx.model_ref.as_ref().unwrap().as_str(), "archon.wal.model");
    assert!(cx.input["inputs"].is_array());
    assert!(!cx.trace.events.is_empty());
    assert!(cx.environment.contains_key("platform"));
    assert!(!cx.violation.detail.is_empty());
    let _ = BTreeMap::<String, String>::new(); // environment type shape
}
