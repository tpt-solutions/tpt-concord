// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Prover adapters: Lean, Telos, Eidos, and a configurable external one.

use serde::{Deserialize, Serialize};
use tpt_concord_core::ObligationID;

/// A proof obligation: something a proof system must establish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofObligation {
    pub id: ObligationID,
    /// The precise statement to prove.
    pub statement: String,
    /// Free-form context: definitions the statement relies on, assumptions
    /// in scope, pointers to the formalization.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub context: std::collections::BTreeMap<String, String>,
}

impl ProofObligation {
    pub fn new(id: impl Into<ObligationID>, statement: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            statement: statement.into(),
            context: Default::default(),
        }
    }
}

/// Outcome of interpreting prover output.
///
/// Note what is *missing*: there is no way for output to claim "proven"
/// without the artifact being re-checkable — that guarantee lives in
/// [`crate::ProofArtifact`], not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum ProofOutcome {
    /// The prover reports the obligation discharged.
    Proven,
    /// The prover reports a definite failure.
    NotProven { detail: String },
    /// The artifact is incomplete — e.g. a `sorry`, `admit`, or axiom
    /// placeholder. Never treat as proven.
    Inconclusive { detail: String },
}

/// A proof-system adapter.
pub trait ProverAdapter {
    /// Adapter name (recorded in artifacts).
    fn name(&self) -> &str;
    /// File extension for rendered artifacts (e.g. `lean`).
    fn file_extension(&self) -> &str;
    /// Render an obligation into the source artifact for the prover.
    fn render(&self, obligation: &ProofObligation) -> String;
    /// The command an independent verifier runs to re-check the artifact —
    /// recorded verbatim in the proof artifact.
    fn check_command(&self, artifact_path: &str) -> String;
    /// Interpret prover output.
    fn parse_result(&self, output: &str) -> ProofOutcome;
}

/// Lean 4 adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct LeanAdapter;

impl ProverAdapter for LeanAdapter {
    fn name(&self) -> &str {
        "lean4"
    }

    fn file_extension(&self) -> &str {
        "lean"
    }

    fn render(&self, obligation: &ProofObligation) -> String {
        let name = sanitize(obligation.id.as_str());
        let mut out = String::new();
        out.push_str(&format!("-- obligation: {}\n", obligation.id));
        for (k, v) in &obligation.context {
            out.push_str(&format!("-- context {k}: {v}\n"));
        }
        out.push_str(&format!(
            "theorem {name} : {statement} := by\n  sorry\n",
            statement = obligation.statement
        ));
        out
    }

    fn check_command(&self, artifact_path: &str) -> String {
        format!("lake env lean {artifact_path}")
    }

    fn parse_result(&self, output: &str) -> ProofOutcome {
        // A `sorry` is *not* a proof: Lean emits a warning we must respect.
        if output.contains("declaration uses 'sorry'") || output.contains("sorry") {
            return ProofOutcome::Inconclusive {
                detail: "artifact contains `sorry` (unproved placeholder)".to_string(),
            };
        }
        for line in output.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("error") || line.contains("error:") {
                return ProofOutcome::NotProven {
                    detail: line.trim().to_string(),
                };
            }
        }
        ProofOutcome::Proven
    }
}

/// Telos adapter (TPT formal proof system).
#[derive(Debug, Clone, Copy, Default)]
pub struct TelosAdapter;

impl ProverAdapter for TelosAdapter {
    fn name(&self) -> &str {
        "telos"
    }

    fn file_extension(&self) -> &str {
        "telos"
    }

    fn render(&self, obligation: &ProofObligation) -> String {
        let mut out = String::new();
        out.push_str(&format!("(claim (id \"{}\")\n", obligation.id));
        out.push_str(&format!(
            "       (statement \"{}\"))\n",
            obligation.statement.replace('"', "\\\"")
        ));
        for (k, v) in &obligation.context {
            out.push_str(&format!(
                "(context ({} \"{}\"))\n",
                k,
                v.replace('"', "\\\"")
            ));
        }
        out.push_str("(goal prove)\n");
        out
    }

    fn check_command(&self, artifact_path: &str) -> String {
        format!("telos check {artifact_path}")
    }

    fn parse_result(&self, output: &str) -> ProofOutcome {
        if output.contains("PROVEN") {
            ProofOutcome::Proven
        } else if output.contains("REJECTED") || output.contains("FAILED") {
            ProofOutcome::NotProven {
                detail: output
                    .lines()
                    .next()
                    .unwrap_or("telos rejected")
                    .to_string(),
            }
        } else {
            ProofOutcome::Inconclusive {
                detail: "no verdict marker in telos output".to_string(),
            }
        }
    }
}

/// Eidos adapter (TPT formal proof system).
#[derive(Debug, Clone, Copy, Default)]
pub struct EidosAdapter;

impl ProverAdapter for EidosAdapter {
    fn name(&self) -> &str {
        "eidos"
    }

    fn file_extension(&self) -> &str {
        "eidos"
    }

    fn render(&self, obligation: &ProofObligation) -> String {
        format!(
            "obligation {}\n  statement: {}\n  status: open\n",
            obligation.id, obligation.statement
        )
    }

    fn check_command(&self, artifact_path: &str) -> String {
        format!("eidos verify {artifact_path}")
    }

    fn parse_result(&self, output: &str) -> ProofOutcome {
        if output.contains("verified") {
            ProofOutcome::Proven
        } else if output.contains("refuted") {
            ProofOutcome::NotProven {
                detail: output.lines().next().unwrap_or("eidos refuted").to_string(),
            }
        } else {
            ProofOutcome::Inconclusive {
                detail: "no verdict marker in eidos output".to_string(),
            }
        }
    }
}

/// A generic adapter for external proof systems, configured with the
/// rendering function's file type and the re-check command template
/// (`{artifact}` is substituted).
#[derive(Debug, Clone)]
pub struct ExternalAdapter {
    pub name: String,
    pub extension: String,
    pub check_command_template: String,
}

impl ProverAdapter for ExternalAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn file_extension(&self) -> &str {
        &self.extension
    }

    fn render(&self, obligation: &ProofObligation) -> String {
        format!(
            "# obligation {}\nstatement: {}\nstatus: open\n",
            obligation.id, obligation.statement
        )
    }

    fn check_command(&self, artifact_path: &str) -> String {
        self.check_command_template
            .replace("{artifact}", artifact_path)
    }

    fn parse_result(&self, output: &str) -> ProofOutcome {
        check_output(output)
    }
}

/// Shared output interpretation used by generic adapters: recognizes the
/// common markers, stays inconclusive otherwise.
pub fn check_output(output: &str) -> ProofOutcome {
    let lower = output.to_lowercase();
    if lower.contains("sorry") || lower.contains("admit") || lower.contains("axiom") {
        ProofOutcome::Inconclusive {
            detail: "artifact contains a proof placeholder".to_string(),
        }
    } else if lower.contains("error") || lower.contains("failed") {
        ProofOutcome::NotProven {
            detail: output
                .lines()
                .find(|l| l.to_lowercase().contains("error") || l.to_lowercase().contains("failed"))
                .unwrap_or("failed")
                .trim()
                .to_string(),
        }
    } else if lower.contains("proven") || lower.contains("verified") {
        ProofOutcome::Proven
    } else {
        ProofOutcome::Inconclusive {
            detail: "no verdict marker in output".to_string(),
        }
    }
}

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obligation() -> ProofObligation {
        let mut o = ProofObligation::new(
            ObligationID::new("obligation.wal-recovery").unwrap(),
            "recovery preserves the committed prefix",
        );
        o.context
            .insert("crash_model".into(), "prefix-crash".into());
        o
    }

    #[test]
    fn lean_render_and_parse() {
        let adapter = LeanAdapter;
        let rendered = adapter.render(&obligation());
        assert!(rendered.starts_with("-- obligation: obligation.wal-recovery"));
        assert!(rendered.contains("theorem obligation_wal_recovery :"));
        assert!(
            rendered.contains("sorry"),
            "scaffold is honest about being unproved"
        );

        // `sorry` must NEVER parse as proven.
        assert_eq!(
            adapter.parse_result("warning: declaration uses 'sorry'"),
            ProofOutcome::Inconclusive {
                detail: "artifact contains `sorry` (unproved placeholder)".into()
            }
        );
        assert_eq!(
            adapter.parse_result("error: unsolved goals"),
            ProofOutcome::NotProven {
                detail: "error: unsolved goals".into()
            }
        );
        assert_eq!(adapter.parse_result(""), ProofOutcome::Proven);
    }

    #[test]
    fn telos_render_and_parse() {
        let adapter = TelosAdapter;
        let rendered = adapter.render(&obligation());
        assert!(rendered.contains("(claim (id \"obligation.wal-recovery\")"));
        assert!(rendered.contains("(goal prove)"));
        assert_eq!(
            adapter.parse_result("PROVEN obligation.wal-recovery"),
            ProofOutcome::Proven
        );
        assert!(matches!(
            adapter.parse_result("FAILED: goal not reachable"),
            ProofOutcome::NotProven { .. }
        ));
        assert!(matches!(
            adapter.parse_result("...nothing..."),
            ProofOutcome::Inconclusive { .. }
        ));
    }

    #[test]
    fn eidos_render_and_parse() {
        let adapter = EidosAdapter;
        let rendered = adapter.render(&obligation());
        assert!(rendered.contains("status: open"));
        assert_eq!(
            adapter.parse_result("obligation verified"),
            ProofOutcome::Proven
        );
        assert!(matches!(
            adapter.parse_result("refuted by model"),
            ProofOutcome::NotProven { .. }
        ));
    }

    #[test]
    fn external_adapter_substitutes_artifact_path() {
        let adapter = ExternalAdapter {
            name: "hol4".into(),
            extension: "sml".into(),
            check_command_template: "Holmake {artifact}".into(),
        };
        assert_eq!(
            adapter.check_command("proofs/x.sml"),
            "Holmake proofs/x.sml"
        );
        assert_eq!(adapter.file_extension(), "sml");
        assert!(matches!(
            check_output("sorry"),
            ProofOutcome::Inconclusive { .. }
        ));
    }

    #[test]
    fn outcome_serializes() {
        let o = ProofOutcome::NotProven { detail: "x".into() };
        let json = serde_json::to_string(&o).unwrap();
        assert!(json.contains("\"outcome\":\"not_proven\""));
    }
}
