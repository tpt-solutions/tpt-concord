// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `concord report` — answer the spec §13 questions in plain markdown.
//!
//! For every claim: what is claimed, under which scope and assumptions, what
//! evidence backs it, and — grouped by evidence family — what is *proven*,
//! what is *model-checked*, what is only *tested*, and what is *unknown*.
//! Evidence families are never merged: a fuzz campaign is listed under
//! "tested", never under "proven".

use std::fmt::Write as _;
use tpt_concord_core::{
    AssuranceGraph, EvidenceCategory, EvidenceStatus, ObligationID, UnmetReason,
};
use tpt_concord_release::{GateDecision, Policy, RejectionReason};

fn family(c: EvidenceCategory) -> &'static str {
    match c {
        EvidenceCategory::FormalProof => "proven",
        EvidenceCategory::ModelChecking => "model-checked",
        EvidenceCategory::Empirical => "tested",
        EvidenceCategory::StaticAnalysis => "statically constrained",
        EvidenceCategory::Specification => "specified only",
    }
}

/// Render the report. `decision` is appended when a policy was supplied.
pub fn render(
    graph: &AssuranceGraph,
    policy: Option<&Policy>,
    decision: Option<&GateDecision>,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Assurance report\n");
    if let Some(p) = policy {
        let _ = writeln!(out, "Policy: **{}**\n", p.name);
    }

    for claim in graph.claims() {
        let _ = writeln!(out, "## {}\n", claim.id);
        let _ = writeln!(out, "**What do we claim?** {}\n", claim.statement);

        let Ok(eval) = graph.evaluate(&claim.id) else {
            let _ = writeln!(out, "_could not evaluate this claim_\n");
            continue;
        };

        let status = if eval.established {
            format!("established at `{}`", eval.achieved_level)
        } else {
            "**not established**".to_string()
        };
        let _ = writeln!(out, "**Status:** {status}\n");

        // Scope and assumptions.
        let _ = writeln!(out, "**Under what assumptions?**");
        let scopes: Vec<_> = claim
            .scopes
            .iter()
            .map(|s| match graph.scope(s) {
                Some(sc) => format!("scope `{s}` — {}", sc.description),
                None => format!("scope `{s}` (undefined)"),
            })
            .collect();
        let assumptions: Vec<_> = eval
            .open_assumptions
            .iter()
            .map(|a| match graph.assumption(a) {
                Some(asm) => format!("assumption `{a}` — {}", asm.statement),
                None => format!("assumption `{a}` (undefined)"),
            })
            .collect();
        if scopes.is_empty() && assumptions.is_empty() {
            let _ = writeln!(
                out,
                "- none recorded (a claim with no scope is an over-claim risk)"
            );
        }
        for line in scopes.iter().chain(assumptions.iter()) {
            let _ = writeln!(out, "- {line}");
        }
        let _ = writeln!(out);

        // Evidence per obligation, grouped by family.
        let _ = writeln!(out, "**Why do we believe it? What evidence exists?**");
        let obligations: Vec<&ObligationID> = graph.obligations_of(&claim.id).collect();
        if obligations.is_empty() {
            let _ = writeln!(
                out,
                "- no obligations declared: nothing is required, so nothing is shown"
            );
        }
        let mut unknown = Vec::new();
        for ob in obligations {
            let desc = graph
                .obligation(ob)
                .map(|o| o.description.as_str())
                .unwrap_or("");
            let _ = writeln!(out, "- `{ob}` — {desc}");
            let mut any = false;
            for ev_id in graph.evidence_for(ob) {
                let Some(ev) = graph.evidence(ev_id) else {
                    continue;
                };
                any = true;
                let state = match ev.status {
                    EvidenceStatus::Validated => family(ev.level.category()),
                    EvidenceStatus::Unvalidated => "UNVALIDATED (does not count)",
                    EvidenceStatus::Rejected => "REJECTED (failed re-check)",
                };
                let _ = writeln!(
                    out,
                    "  - [{state}] `{}` ({}, level `{}`): {}",
                    ev.id, ev.kind, ev.level, ev.description
                );
            }
            if !any {
                let _ = writeln!(out, "  - _no evidence linked_");
            }
        }
        let _ = writeln!(out);

        let _ = writeln!(out, "**What remains unknown or unproven?**");
        for u in &eval.unmet_obligations {
            let why = match u.reason {
                UnmetReason::NoEvidence => "no evidence",
                UnmetReason::EvidenceUnvalidated => "evidence not independently validated",
                UnmetReason::EvidenceRejected => "evidence failed re-checking",
            };
            unknown.push(format!("`{}` — {why}", u.obligation));
        }
        for dep in &eval.unestablished_dependencies {
            unknown.push(format!("depends on unestablished claim `{dep}`"));
        }
        if unknown.is_empty() {
            let _ = writeln!(
                out,
                "- every declared obligation has validated evidence; anything *not declared* as an obligation is still unknown"
            );
        }
        for u in unknown {
            let _ = writeln!(out, "- {u}");
        }
        let _ = writeln!(out);
    }

    if let Some(decision) = decision {
        let _ = writeln!(out, "## Release decision\n");
        match decision {
            GateDecision::Approved(s) => {
                let _ = writeln!(
                    out,
                    "**APPROVED** — {} required claim(s) established, {} validated evidence object(s).",
                    s.established_claims.len(),
                    s.supporting_evidence
                );
            }
            GateDecision::Rejected { reasons } => {
                let _ = writeln!(out, "**REJECTED**\n");
                for r in reasons {
                    let _ = writeln!(out, "- {}", describe(r));
                }
            }
        }
    }
    out
}

fn describe(r: &RejectionReason) -> String {
    match r {
        RejectionReason::GraphInvalid { detail } => format!("assurance graph is invalid: {detail}"),
        RejectionReason::UnknownClaim { claim } => {
            format!("required claim `{claim}` does not exist")
        }
        RejectionReason::UnknownObligation { obligation } => {
            format!("required obligation `{obligation}` does not exist")
        }
        RejectionReason::ClaimUnestablished { claim, .. } => {
            format!("claim `{claim}` is not established")
        }
        RejectionReason::LevelInsufficient {
            claim,
            required,
            achieved,
        } => format!("claim `{claim}` needs `{required}` but only reaches `{achieved}`"),
        RejectionReason::OpenAssumptions { claim, assumptions } => format!(
            "claim `{claim}` rests on open assumptions: {}",
            assumptions
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        RejectionReason::UndischargedObligation { obligation } => {
            format!("required obligation `{obligation}` has no validated evidence")
        }
    }
}
