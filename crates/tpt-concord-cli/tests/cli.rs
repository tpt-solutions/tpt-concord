// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! End-to-end CLI tests: build JSON objects via the library crates, invoke
//! the `concord` binary, and assert on exit codes and output.

use serde_json::json;
use std::process::Command;
use tpt_concord_core::{
    AssuranceGraph, AssuranceLevel, Claim, EvidenceKind, EvidenceRecord, Obligation,
};
use tpt_concord_evidence::{EvidenceBundleBuilder, Section};
use tpt_concord_release::Policy;
use tpt_concord_trace::TraceBuilder;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_concord")
}

fn run(args: &[&str]) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .output()
        .expect("binary runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("concord-cli-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &std::path::Path, value: &serde_json::Value) {
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

fn spec_json() -> serde_json::Value {
    json!({
        "id": "cli.spec",
        "version": "1.0.0",
        "name": "CLI smoke spec",
        "description": "",
        "state_vars": {"depth": "int"},
        "invariants": [
            {"name": "nonneg", "expr": {"ge": [{"var": "depth"}, {"const": {"int": 0}}]}}
        ]
    })
}

fn table_json() -> serde_json::Value {
    json!({
        "initial": "closed",
        "transitions": [
            {"from": "closed", "input": "open", "to": "open", "effects": ["door.open"]},
            {"from": "open", "input": "close", "to": "closed", "effects": ["door.close"]},
            {"from": "open", "input": "open", "to": "open"}
        ]
    })
}

fn trace_with(values: &[i64]) -> tpt_concord_trace::Trace {
    let mut b = TraceBuilder::new();
    for v in values {
        // A step payload the checker can consume.
        b.event(
            "step",
            json!({
                "name": "no-op",
                "before": {"x": {"int": v}},
                "after": {"x": {"int": v}},
                "effects": [],
                "resources": {},
            }),
        );
    }
    b.finish()
}

#[test]
fn spec_valid_and_invalid() {
    let dir = tmp_dir("spec");
    let path = dir.join("spec.json");
    write(&path, &spec_json());
    let (code, out) = run(&["spec", path.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(out.contains("\"valid\": true"));

    write(&path, &json!({"id": "", "version": "1", "name": "bad"}));
    let (code, _) = run(&["spec", path.to_str().unwrap()]);
    assert_eq!(code, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn model_run_prints_records() {
    let dir = tmp_dir("model");
    let model = dir.join("model.json");
    let inputs = dir.join("inputs.json");
    write(&model, &table_json());
    write(&inputs, &json!(["open", "close", "open"]));
    let (code, out) = run(&[
        "model",
        model.to_str().unwrap(),
        "--inputs",
        inputs.to_str().unwrap(),
    ]);
    assert_eq!(code, 0);
    assert!(out.contains("\"after\": \"open\""));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simulate_explores_and_fuzz_is_clean() {
    let dir = tmp_dir("sim");
    let model = dir.join("model.json");
    write(&model, &table_json());
    let (code, out) = run(&[
        "simulate",
        model.to_str().unwrap(),
        "--alphabet",
        "open,close",
        "--depth",
        "6",
    ]);
    assert_eq!(code, 0);
    assert!(out.contains("\"states_reached\": 2"));

    let (code, out) = run(&[
        "fuzz",
        model.to_str().unwrap(),
        "--alphabet",
        "open,close",
        "--seed",
        "5",
        "--cycles",
        "20",
    ]);
    assert_eq!(code, 0);
    assert!(out.contains("\"clean\": true"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simulate_finds_bad_state() {
    let dir = tmp_dir("bad");
    let model = dir.join("model.json");
    write(&model, &table_json());
    let (code, out) = run(&[
        "simulate",
        model.to_str().unwrap(),
        "--alphabet",
        "open,close",
        "--depth",
        "4",
        "--bad-state",
        "open",
    ]);
    assert_eq!(code, 1, "bad state found => exit 1");
    assert!(out.contains("\"witness_inputs\""));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn prove_renders_lean_artifact() {
    let dir = tmp_dir("prove");
    let out = dir.join("ob.lean");
    let (code, _) = run(&[
        "prove",
        "--adapter",
        "lean4",
        "--obligation",
        "obligation.cli",
        "--statement",
        "True",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0);
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("theorem obligation_cli : True := by"));
    // External adapter with template.
    let out2 = dir.join("ob.custom");
    let (code, _) = run(&[
        "prove",
        "--adapter",
        "hol:hol:Holmake {artifact}",
        "--obligation",
        "obligation.cli2",
        "--statement",
        "False",
        "--out",
        out2.to_str().unwrap(),
    ]);
    assert_eq!(code, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn diff_agrees_and_disagrees() {
    let dir = tmp_dir("diff");
    let a = trace_with(&[1, 2, 3]);
    let b = trace_with(&[1, 2, 3]);
    let c = trace_with(&[1, 9, 3]);
    let pa = dir.join("a.json");
    let pb = dir.join("b.json");
    let pc = dir.join("c.json");
    write(&pa, &serde_json::to_value(&a).unwrap());
    write(&pb, &serde_json::to_value(&b).unwrap());
    write(&pc, &serde_json::to_value(&c).unwrap());

    let (code, out) = run(&["diff", pa.to_str().unwrap(), pb.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(out.contains("\"agree\": true"));

    let (code, out) = run(&["diff", pa.to_str().unwrap(), pc.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(out.contains("\"agree\": false"));
    assert!(out.contains("value_mismatch"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn evidence_verify_roundtrip() {
    let dir = tmp_dir("ev");
    let bundle_dir = dir.join("bundle");
    EvidenceBundleBuilder::new("cli-bundle")
        .artifact(
            Section::TestResults,
            tpt_concord_core::EvidenceID::new("artifact.test").unwrap(),
            EvidenceKind::TestRun,
            "results.json",
            "",
            br#"{"ok": true}"#.to_vec(),
        )
        .unwrap()
        .write(&bundle_dir)
        .unwrap();

    let (code, out) = run(&["evidence", "verify", bundle_dir.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(out.contains("\"verified\": true"));

    // Tamper → verify fails with exit 1.
    std::fs::write(
        bundle_dir.join("artifacts/results.json"),
        br#"{"ok": false}"#,
    )
    .unwrap();
    let (code, _) = run(&["evidence", "verify", bundle_dir.to_str().unwrap()]);
    assert_eq!(code, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gate_approves_and_rejects() {
    let dir = tmp_dir("gate");

    let mut graph = AssuranceGraph::new();
    let claim = Claim::new(
        tpt_concord_core::ClaimID::new("claim.gate").unwrap(),
        "test claim",
    );
    graph.add_claim(claim).unwrap();
    graph
        .add_obligation(Obligation::new(
            tpt_concord_core::ObligationID::new("obligation.gate").unwrap(),
            "test obligation",
        ))
        .unwrap();
    graph
        .require_obligation(
            &tpt_concord_core::ClaimID::new("claim.gate").unwrap(),
            &tpt_concord_core::ObligationID::new("obligation.gate").unwrap(),
        )
        .unwrap();
    let mut ev = EvidenceRecord::new(
        tpt_concord_core::EvidenceID::new("evidence.gate").unwrap(),
        EvidenceKind::ModelCheck,
        AssuranceLevel::ModelChecked,
        "",
    );
    ev.validate();
    graph.add_evidence(ev).unwrap();
    graph
        .discharge(
            &tpt_concord_core::ObligationID::new("obligation.gate").unwrap(),
            &tpt_concord_core::EvidenceID::new("evidence.gate").unwrap(),
        )
        .unwrap();

    let mut policy = Policy::new("cli-policy");
    policy.require_claim(
        tpt_concord_core::ClaimID::new("claim.gate").unwrap(),
        AssuranceLevel::ModelChecked,
    );

    let pg = dir.join("graph.json");
    let pp = dir.join("policy.json");
    write(&pg, &serde_json::to_value(&graph).unwrap());
    write(&pp, &serde_json::to_value(&policy).unwrap());

    let (code, out) = run(&["gate", pg.to_str().unwrap(), pp.to_str().unwrap()]);
    assert_eq!(code, 0, "approved gate should exit 0: {out}");
    assert!(out.contains("\"approved\""));

    // Now demand full proof: rejection with exit 1 and reasons.
    let mut strict = Policy::new("cli-strict");
    strict.require_claim(
        tpt_concord_core::ClaimID::new("claim.gate").unwrap(),
        AssuranceLevel::FullyProven,
    );
    write(&pp, &serde_json::to_value(&strict).unwrap());
    let (code, out) = run(&["gate", pg.to_str().unwrap(), pp.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(out.contains("\"rejected\""));
    assert!(out.contains("level_insufficient"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_command_reports_conformance() {
    let dir = tmp_dir("check");
    // A spec requiring depth >= 0, and a trace that never violates it.
    let ps = dir.join("spec.json");
    write(&ps, &spec_json());

    let mut b = TraceBuilder::new();
    b.event(
        "step",
        json!({
            "name": "no-op",
            "before": {"depth": {"int": 0}},
            "after": {"depth": {"int": 1}},
            "effects": [],
            "resources": {},
        }),
    );
    let trace = b.finish();
    let pt = dir.join("trace.json");
    write(&pt, &serde_json::to_value(&trace).unwrap());

    let (code, out) = run(&["check", ps.to_str().unwrap(), pt.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"conforms\": true"));

    // Violating trace: depth goes negative.
    let mut b = TraceBuilder::new();
    b.event(
        "step",
        json!({
            "name": "no-op",
            "before": {"depth": {"int": 0}},
            "after": {"depth": {"int": -1}},
            "effects": [],
            "resources": {},
        }),
    );
    write(&pt, &serde_json::to_value(b.finish()).unwrap());
    let (code, out) = run(&["check", ps.to_str().unwrap(), pt.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(out.contains("\"conforms\": false"));
    assert!(out.contains("invariant"));
    let _ = std::fs::remove_dir_all(&dir);
}
