// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Command implementations. Every command reads/writes machine-readable
//! JSON and returns a process exit code.

use std::path::Path;
use tpt_concord_check::{check_trace, classify_divergence};
use tpt_concord_core::AssuranceGraph;
use tpt_concord_diff::DiffSide;
use tpt_concord_evidence::{read_and_verify, EvidenceBundle};
use tpt_concord_model::{run, TransitionTable};
use tpt_concord_proof::{
    EidosAdapter, ExternalAdapter, LeanAdapter, ProofObligation, ProverAdapter, TelosAdapter,
};
use tpt_concord_release::{evaluate, Policy};
use tpt_concord_sim::explore;
use tpt_concord_spec::Specification;
use tpt_concord_trace::Trace;

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("cannot parse {} as JSON: {e}", path.display()))
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    let out = serde_json::to_string_pretty(value).map_err(|e| format!("cannot serialize: {e}"))?;
    println!("{out}");
    Ok(())
}

/// `concord spec <file>` — validate a specification.
pub fn spec(file: &Path) -> i32 {
    match read_json::<Specification>(file) {
        Ok(spec) => {
            let summary = serde_json::json!({
                "id": spec.id,
                "version": spec.version,
                "name": spec.name,
                "invariants": spec.invariants.len(),
                "preconditions": spec.preconditions.len(),
                "postconditions": spec.postconditions.len(),
                "transitions": spec.transitions.len(),
                "resource_bounds": spec.resource_bounds.len(),
                "permitted_behaviours": spec.permitted_behaviours.len(),
                "valid": true,
            });
            if print_json(&summary).is_err() {
                2
            } else {
                0
            }
        }
        Err(e) => {
            eprintln!("spec invalid: {e}");
            2
        }
    }
}

/// `concord model <file> --inputs <file>` — run a table model.
pub fn model(file: &Path, inputs: &Path) -> i32 {
    let table: TransitionTable<String, String> = match read_json(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("model: {e}");
            return 2;
        }
    };
    let inputs: Vec<String> = match read_json(inputs) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("inputs: {e}");
            return 2;
        }
    };
    match run(&table, &inputs) {
        Ok(records) => {
            let out: Vec<_> = records
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "step": r.step,
                        "input": r.input,
                        "before": r.before,
                        "after": r.after,
                        "effects": r.effects,
                    })
                })
                .collect();
            if print_json(&out).is_err() {
                2
            } else {
                0
            }
        }
        Err(e) => {
            eprintln!("model run failed: {e}");
            1
        }
    }
}

/// `concord check <spec> <trace>` — conformance-check a trace.
pub fn check(spec: &Path, trace: &Path) -> i32 {
    let (spec, trace): (Specification, Trace) = match (read_json(spec), read_json(trace)) {
        (Ok(s), Ok(t)) => (s, t),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("check: {e}");
            return 2;
        }
    };
    match check_trace(&spec, &trace) {
        Ok(report) => {
            let json = serde_json::json!({
                "checked_steps": report.checked_steps,
                "conforms": report.conforms,
                "violations": report.violations,
            });
            let printed = print_json(&json);
            if report.conforms {
                if printed.is_ok() {
                    0
                } else {
                    2
                }
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("check: {e}");
            2
        }
    }
}

/// `concord prove --adapter .. --obligation .. --statement .. --out ..`
pub fn prove(adapter: &str, obligation: &str, statement: &str, out: &Path) -> i32 {
    let ob = match make_obligation(obligation, statement) {
        Ok(ob) => ob,
        Err(e) => {
            eprintln!("prove: {e}");
            return 2;
        }
    };
    let rendered = if let Some((name, ext, template)) = adapter
        .split_once(':')
        .and_then(|(n, rest)| rest.split_once(':').map(|(e, t)| (n, e, t)))
    {
        ExternalAdapter {
            name: name.to_string(),
            extension: ext.to_string(),
            check_command_template: template.to_string(),
        }
        .render(&ob)
    } else {
        match adapter {
            "lean4" | "lean" => LeanAdapter.render(&ob),
            "telos" => TelosAdapter.render(&ob),
            "eidos" => EidosAdapter.render(&ob),
            other => {
                eprintln!(
                    "prove: unknown adapter {other:?} (lean4, telos, eidos, or name:ext:template)"
                );
                return 2;
            }
        }
    };
    match std::fs::write(out, rendered) {
        Ok(()) => {
            println!(
                "rendered {} obligation {obligation} -> {}",
                adapter,
                out.display()
            );
            0
        }
        Err(e) => {
            eprintln!("prove: cannot write {}: {e}", out.display());
            2
        }
    }
}

/// `concord simulate <file> --alphabet .. [--depth ..] [--bad-state ..]`
pub fn simulate(file: &Path, alphabet: &str, depth: usize, bad_state: Option<&str>) -> i32 {
    let table: TransitionTable<String, String> = match read_json(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("simulate: {e}");
            return 2;
        }
    };
    let alphabet: Vec<String> = alphabet.split(',').map(|s| s.trim().to_string()).collect();
    let report = match explore(&table, &alphabet, depth, |s: &String| {
        bad_state.is_some_and(|bad| s.contains(bad))
    }) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("simulate: {e}");
            return 1;
        }
    };
    let json = serde_json::json!({
        "states_reached": report.states_reached,
        "transitions_explored": report.transitions_explored,
        "max_depth_reached": report.max_depth_reached,
        "bad_state": report.bad_state.as_ref().map(|(s, path)| serde_json::json!({
            "state": s,
            "witness_inputs": path,
        })),
    });
    let ok = print_json(&json).is_ok();
    if !ok {
        2
    } else if report.found_bad_state() {
        1
    } else {
        0
    }
}

/// `concord fuzz <file> --alphabet .. --seed .. --cycles .. --seq-len ..`
pub fn fuzz(file: &Path, alphabet: &str, seed: u64, cycles: usize, seq_len: usize) -> i32 {
    let table: TransitionTable<String, String> = match read_json(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("fuzz: {e}");
            return 2;
        }
    };
    let alphabet: Vec<String> = alphabet.split(',').map(|s| s.trim().to_string()).collect();
    // Invariant under fuzz: every legal generated sequence must execute
    // without an unknown-transition error, and every reached state must be
    // re-serializable (machine-readability itself is the invariant here).
    let report = tpt_concord_fuzz::fuzz_stateful(
        &table,
        &alphabet,
        seed,
        cycles,
        seq_len,
        |seq: &[String]| {
            // Re-execute: legal sequences generated from the abstract table
            // must all be executable by the deterministic machine.
            if run(&table, seq).is_ok() {
                Ok(())
            } else {
                Err("generated sequence failed to execute".to_string())
            }
        },
        None,
    );
    let json = serde_json::json!({
        "cycles": report.cycles,
        "clean": report.is_clean(),
        "failures": report.failures,
        "note": "a clean campaign is fuzz_tested evidence, never proof",
    });
    let ok = print_json(&json).is_ok();
    if !ok {
        2
    } else if report.is_clean() {
        0
    } else {
        1
    }
}

/// `concord diff <left> <right>` — compare two traces.
pub fn diff(left: &Path, right: &Path) -> i32 {
    let (left_trace, right_trace): (Trace, Trace) = match (read_json(left), read_json(right)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("diff: {e}");
            return 2;
        }
    };
    let report = tpt_concord_diff::diff(
        (
            &left_trace,
            DiffSide::Implementation {
                implementation: tpt_concord_core::ImplementationID::new("left").unwrap(),
                version: None,
            },
        ),
        (
            &right_trace,
            DiffSide::Implementation {
                implementation: tpt_concord_core::ImplementationID::new("right").unwrap(),
                version: None,
            },
        ),
    );
    let json = serde_json::json!({
        "agree": report.agree(),
        "verdict": report.verdict(),
        "first_deviation_class": report.divergences.first().map(|d| {
            classify_divergence(&left_trace, &right_trace, d)
        }),
        "divergences": report.divergences,
    });
    let ok = print_json(&json).is_ok();
    if !ok {
        2
    } else if report.agree() {
        0
    } else {
        1
    }
}

/// `concord evidence verify <dir>` / `evidence show <dir>`.
pub fn evidence_verify(dir: &Path) -> i32 {
    match read_and_verify(dir) {
        Ok(bundle) => {
            let json = serde_json::json!({
                "bundle_id": bundle.bundle_id,
                "verified": true,
                "artifacts": bundle.artifacts().count(),
            });
            let ok = print_json(&json).is_ok();
            if ok {
                0
            } else {
                2
            }
        }
        Err(e) => {
            eprintln!("evidence: {e}");
            1
        }
    }
}

pub fn evidence_show(dir: &Path) -> i32 {
    match std::fs::read_to_string(dir.join("manifest.json")) {
        Ok(text) => match serde_json::from_str::<EvidenceBundle>(&text) {
            Ok(bundle) => {
                let ok = print_json(&bundle).is_ok();
                if ok {
                    0
                } else {
                    2
                }
            }
            Err(e) => {
                eprintln!("evidence: bad manifest: {e}");
                2
            }
        },
        Err(e) => {
            eprintln!("evidence: {e}");
            2
        }
    }
}

/// `concord gate <graph> <policy>` — evaluate the release gate.
pub fn gate(graph: &Path, policy: &Path) -> i32 {
    let (graph, policy): (AssuranceGraph, Policy) = match (read_json(graph), read_json(policy)) {
        (Ok(g), Ok(p)) => (g, p),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("gate: {e}");
            return 2;
        }
    };
    let decision = evaluate(&graph, &policy);
    let approved = decision.is_approved();
    let ok = print_json(&decision).is_ok();
    if !ok {
        2
    } else if approved {
        0
    } else {
        1
    }
}

fn make_obligation(obligation: &str, statement: &str) -> Result<ProofObligation, String> {
    use tpt_concord_core::ObligationID;
    let id = ObligationID::new(obligation).map_err(|e| e.to_string())?;
    Ok(ProofObligation::new(id, statement))
}
