// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `concord init` — scaffold a runnable starter project.
//!
//! Every file is built from the library types (never hand-written JSON), so
//! the starter project is valid by construction and doubles as living
//! documentation of each object's schema. The scaffold is deliberately
//! *honest*: it ships one claim that is established at `model_checked` and a
//! second claim with no evidence, so the very first `concord report` shows
//! the difference between "what we know" and "what we don't".

use serde::Serialize;
use std::path::Path;
use tpt_concord_check::StepView;
use tpt_concord_core::{
    Assumption, AssuranceGraph, AssuranceLevel, Claim, EvidenceID, EvidenceKind, EvidenceRecord,
    Obligation, Scope,
};
use tpt_concord_evidence::{EvidenceBundleBuilder, Section, ToolchainIdentity};
use tpt_concord_model::TransitionTable;
use tpt_concord_release::Policy;
use tpt_concord_spec::{Expr, Named, Specification, Value, ValueType};
use tpt_concord_trace::{Trace, TraceBuilder};

const CLAIM_SAFE: &str = "claim.door.never-open-while-locked";
const CLAIM_LIVE: &str = "claim.door.never-deadlocks";
const OB_SAFE: &str = "obligation.door.state-space-explored";
const OB_LIVE: &str = "obligation.door.liveness-proof";

fn id<T: TryFrom<String, Error = tpt_concord_core::Error>>(s: &str) -> T {
    T::try_from(s.to_string()).expect("scaffold ids are valid")
}

fn table(unlock_to: &str) -> TransitionTable<String, String> {
    let mut t = TransitionTable::new("closed".to_string());
    let s = |x: &str| x.to_string();
    t.transition(s("closed"), s("open"), s("open"));
    t.transition(s("open"), s("close"), s("closed"));
    t.transition(s("closed"), s("lock"), s("locked"));
    t.transition(s("locked"), s("unlock"), s(unlock_to));
    t
}

fn spec() -> Specification {
    let mut spec = Specification::new(
        id::<tpt_concord_core::SpecificationID>("spec.door"),
        tpt_concord_core::Version::new("1.0.0").unwrap(),
        "Door controller",
    );
    spec.description = "A door is never open and locked at the same time.".into();
    spec.state_vars.insert("open".into(), ValueType::Bool);
    spec.state_vars.insert("locked".into(), ValueType::Bool);
    spec.invariants.push(Named::new(
        "never-open-while-locked",
        Expr::Not(Box::new(Expr::and(Expr::var("open"), Expr::var("locked")))),
    ));
    spec
}

fn step(name: &str, before: (bool, bool), after: (bool, bool)) -> StepView {
    let state = |(open, locked): (bool, bool)| {
        [
            ("open".to_string(), Value::Bool(open)),
            ("locked".to_string(), Value::Bool(locked)),
        ]
        .into_iter()
        .collect()
    };
    StepView::new(name, state(before), state(after))
}

fn trace(steps: &[StepView]) -> Trace {
    let mut b = TraceBuilder::new();
    for s in steps {
        b.event("step", s.to_event_payload());
    }
    b.finish()
}

fn good_trace() -> Trace {
    trace(&[
        step("open", (false, false), (true, false)),
        step("close", (true, false), (false, false)),
        step("lock", (false, false), (false, true)),
    ])
}

/// The classic bug: the controller locks the bolt while the door is open.
fn bad_trace() -> Trace {
    trace(&[
        step("open", (false, false), (true, false)),
        step("lock", (true, false), (true, true)),
    ])
}

fn pretty<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}

/// Build the starter assurance graph and its evidence bundle.
fn graph_and_bundle(bundle_dir: &Path) -> Result<AssuranceGraph, String> {
    // Real evidence: explore the model's state space and record the result.
    let model = table("closed");
    let alphabet: Vec<String> = ["open", "close", "lock", "unlock"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let report = tpt_concord_sim::explore(&model, &alphabet, 6, |s: &String| s == "open+locked")
        .map_err(|e| e.to_string())?;
    let artifact = serde_json::json!({
        "states_reached": report.states_reached,
        "transitions_explored": report.transitions_explored,
        "max_depth_reached": report.max_depth_reached,
        "bad_state": "open+locked",
        "bad_state_found": report.found_bad_state(),
    });
    let artifact_bytes = pretty(&artifact)?.into_bytes();

    let bundle = EvidenceBundleBuilder::new("door-starter")
        .toolchain(ToolchainIdentity::new("concord", env!("CARGO_PKG_VERSION")))
        .artifact(
            Section::Simulations,
            id::<EvidenceID>("evidence.door.exploration"),
            EvidenceKind::Simulation,
            "exploration.json",
            "Exhaustive exploration of the door model to depth 6",
            artifact_bytes,
        )
        .map_err(|e| e.to_string())?
        .write(bundle_dir)
        .map_err(|e| e.to_string())?;
    let stored = bundle
        .simulations
        .first()
        .ok_or("bundle lost its artifact")?;

    let mut g = AssuranceGraph::new();
    let e = |r: Result<(), tpt_concord_core::Error>| r.map_err(|e| e.to_string());

    e(g.add_scope(Scope::new(
        id::<tpt_concord_core::ScopeID>("scope.door.single-controller"),
        "One controller instance; no concurrent actuators",
    )))?;
    e(g.add_assumption(Assumption::new(
        id::<tpt_concord_core::AssumptionID>("assumption.door.sensor-truthful"),
        "The door-position sensor reports the true physical state",
        Some(id("scope.door.single-controller")),
    )))?;

    let mut safe = Claim::new(
        id::<tpt_concord_core::ClaimID>(CLAIM_SAFE),
        "The door controller never reaches a state where the door is open and locked",
    );
    safe.scopes.push(id("scope.door.single-controller"));
    safe.assumptions.push(id("assumption.door.sensor-truthful"));
    e(g.add_claim(safe))?;
    e(g.add_obligation(Obligation::new(
        id::<tpt_concord_core::ObligationID>(OB_SAFE),
        "Every reachable state within depth 6 satisfies the invariant",
    )))?;
    e(g.require_obligation(&id(CLAIM_SAFE), &id(OB_SAFE)))?;

    let mut ev = EvidenceRecord::new(
        id::<EvidenceID>("evidence.door.exploration"),
        EvidenceKind::Simulation,
        AssuranceLevel::ModelChecked,
        "Bounded exhaustive exploration (depth 6): no bad state reachable",
    )
    .with_artifact(&stored.path, &stored.sha256);
    ev.validate();
    e(g.add_evidence(ev))?;
    e(g.discharge(&id(OB_SAFE), &id("evidence.door.exploration")))?;

    // A claim nobody has evidence for — deliberately left unproven.
    e(g.add_claim(Claim::new(
        id::<tpt_concord_core::ClaimID>(CLAIM_LIVE),
        "The door controller can always return to the closed state",
    )))?;
    e(g.add_obligation(Obligation::new(
        id::<tpt_concord_core::ObligationID>(OB_LIVE),
        "A proof that `closed` is reachable from every reachable state",
    )))?;
    e(g.require_obligation(&id(CLAIM_LIVE), &id(OB_LIVE)))?;
    Ok(g)
}

fn policy() -> Policy {
    let mut p = Policy::new("door-release");
    p.require_claim(id(CLAIM_SAFE), AssuranceLevel::ModelChecked);
    p
}

const README: &str = "\
Concord starter project
=======================

Everything here is plain JSON. Try these, in order:

  concord spec spec.json                         # validate the specification
  concord check spec.json trace-good.json        # conforms      -> exit 0
  concord check spec.json trace-bad.json         # violation     -> exit 1
  concord simulate model.json --alphabet open,close,lock,unlock --depth 6
  concord fuzz model.json --alphabet open,close,lock,unlock --impl impl-buggy.json
                                                 # finds the bug -> exit 1
  concord report graph.json --policy policy.json --bundle evidence
                                                 # what we know / don't know
  concord gate graph.json policy.json --bundle evidence
                                                 # release decision -> exit 0

Note the report: `claim.door.never-deadlocks` has no evidence, so Concord
says so. Add evidence for it (or drop the claim) and re-run.

Run `concord evidence verify evidence` after editing anything in evidence/:
the gate re-hashes artifacts and rejects tampered or missing ones.
";

/// `concord init [dir] [--force]`.
pub fn init(dir: &Path, force: bool) -> i32 {
    match write_project(dir, force) {
        Ok(files) => {
            println!("created Concord starter project in {}:", dir.display());
            for f in files {
                println!("  {f}");
            }
            println!("\nnext: cd {} && cat README.txt", dir.display());
            0
        }
        Err(e) => {
            eprintln!("init: {e}");
            2
        }
    }
}

fn write_project(dir: &Path, force: bool) -> Result<Vec<String>, String> {
    let files: Vec<(&str, String)> = vec![
        ("spec.json", pretty(&spec())?),
        ("model.json", pretty(&table("closed"))?),
        ("impl-buggy.json", pretty(&table("open"))?),
        ("trace-good.json", pretty(&good_trace())?),
        ("trace-bad.json", pretty(&bad_trace())?),
        ("policy.json", pretty(&policy())?),
        ("README.txt", README.to_string()),
    ];
    if !force {
        let clash: Vec<&str> = files
            .iter()
            .map(|(n, _)| *n)
            .chain(["graph.json", "evidence"])
            .filter(|n| dir.join(n).exists())
            .collect();
        if !clash.is_empty() {
            return Err(format!(
                "refusing to overwrite {} (use --force)",
                clash.join(", ")
            ));
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let bundle_dir = dir.join("evidence");
    if bundle_dir.exists() {
        std::fs::remove_dir_all(&bundle_dir).map_err(|e| e.to_string())?;
    }
    let graph = graph_and_bundle(&bundle_dir)?;

    let mut written = Vec::new();
    for (name, content) in files.into_iter().chain([("graph.json", pretty(&graph)?)]) {
        std::fs::write(dir.join(name), content).map_err(|e| format!("cannot write {name}: {e}"))?;
        written.push(name.to_string());
    }
    written.push("evidence/".to_string());
    Ok(written)
}
