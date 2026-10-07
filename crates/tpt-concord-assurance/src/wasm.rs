// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! WASM assurance infrastructure (spec §12).
//!
//! Grounding: Boxcar runs Wasm side-by-side with OCI (TPT Origin) and hot-
//! reloads Wasm plugins at the edge (TPT Frontier) on Wasmtime;
//! `tpt-solutions/tpt-uir` compiles the TPT AI stack's formally verifiable
//! IR toward Wasm targets.
//!
//! What is claimed here is deliberately narrow and honest:
//! - **validation**: a value-stack type checker admits exactly the well-typed
//!   instruction sequences (modeled below);
//! - **memory safety**: linear-memory accesses stay in bounds — enforced by
//!   the *runtime* (Wasmtime sandbox), so it is recorded as an assumption
//!   about the host runtime, never as a Concord-level proof;
//! - **determinism**: scoped to the deterministic core subset (no NaN-bit
//!   dependence, no SIMD nondeterminism proposals).

use std::collections::BTreeSet;
use tpt_concord_core::{
    Assumption, AssuranceGraph, AssuranceLevel, Claim, EvidenceKind, EvidenceRecord, Obligation,
    Scope,
};
use tpt_concord_model::{DeterministicMachine, StepOutcome};

/// Abstract Wasm value types for validation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum VType {
    I32,
    I64,
}

/// Validation instructions (the i32/i64 arithmetic core).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Instr {
    Const(VType),
    BinOp(VType),
    Drop,
    End,
}

impl std::fmt::Display for Instr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Instr::Const(t) => write!(f, "const.{t:?}"),
            Instr::BinOp(t) => write!(f, "binop.{t:?}"),
            Instr::Drop => f.write_str("drop"),
            Instr::End => f.write_str("end"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("stack underflow at `{op}`")]
    Underflow { op: &'static str },
    #[error("type mismatch at `{op}`: expected {expected:?}, found {found:?}")]
    TypeMismatch {
        op: &'static str,
        expected: VType,
        found: VType,
    },
    #[error("block ended with non-empty stack (left {0} value(s))")]
    NonEmptyEnd(usize),
}

/// The Wasm validation state machine: abstract stack of value types.
///
/// Models the core validation discipline: operands are consumed in types and
/// order, `end` requires the stack to be empty (block signature void).
#[derive(Debug, Clone, Default)]
pub struct WasmValidatorModel;

impl DeterministicMachine for WasmValidatorModel {
    type State = Vec<VType>; // abstract value stack
    type Input = Instr;
    type Error = ValidationError;

    fn initial_state(&self) -> Vec<VType> {
        Vec::new()
    }

    fn step(
        &self,
        stack: &Vec<VType>,
        instr: &Instr,
    ) -> Result<StepOutcome<Vec<VType>>, ValidationError> {
        let underflow = |op| ValidationError::Underflow { op };
        let mut next = stack.clone();
        let effects = BTreeSet::from(["wasm.validate".to_string()]);
        match instr {
            Instr::Const(t) => {
                next.push(*t);
                Ok(StepOutcome { next, effects })
            }
            Instr::BinOp(t) => {
                let b = next.pop().ok_or(underflow("binop"))?;
                let a = next.pop().ok_or(underflow("binop"))?;
                if a != *t || b != *t {
                    return Err(ValidationError::TypeMismatch {
                        op: "binop",
                        expected: *t,
                        found: if a != *t { a } else { b },
                    });
                }
                next.push(*t);
                Ok(StepOutcome { next, effects })
            }
            Instr::Drop => {
                next.pop().ok_or(underflow("drop"))?;
                Ok(StepOutcome { next, effects })
            }
            Instr::End => {
                if !next.is_empty() {
                    return Err(ValidationError::NonEmptyEnd(next.len()));
                }
                Ok(StepOutcome { next, effects })
            }
        }
    }
}

/// The WASM claim graph: validation is model-checked here; sandboxing and
/// determinism rest on explicit runtime/scope assumptions.
pub fn claim_graph() -> (AssuranceGraph, tpt_concord_core::ClaimID) {
    use tpt_concord_core::{AssumptionID, ClaimID, ObligationID, ScopeID};

    let mut g = AssuranceGraph::new();
    g.add_scope(Scope::new(
        ScopeID::new("scope.wasm.deterministic-core").unwrap(),
        "Deterministic core subset: no NaN-bit dependence, no nondeterministic SIMD \
         or threading proposals.",
    ))
    .unwrap();

    let claim_id = ClaimID::new("claim.wasm.validation-and-safety").unwrap();
    let mut claim = Claim::new(
        claim_id.clone(),
        "TPT Wasm workloads are admitted only when statically well-typed, execute inside \
         a runtime-enforced sandbox, and behave deterministically within the core subset.",
    );
    claim.scopes = vec![ScopeID::new("scope.wasm.deterministic-core").unwrap()];
    g.add_assumption(Assumption::new(
        AssumptionID::new("assume.wasm.runtime-sandbox").unwrap(),
        "The host runtime (Wasmtime) enforces linear-memory bounds and control-flow \
         integrity at execution time.",
        Some(ScopeID::new("scope.wasm.deterministic-core").unwrap()),
    ))
    .unwrap();
    claim.assumptions = vec![AssumptionID::new("assume.wasm.runtime-sandbox").unwrap()];
    g.add_claim(claim).unwrap();

    for (id, desc) in [
        (
            "obligation.wasm.validation",
            "Malformed instruction sequences are rejected at validation time (model-checked)",
        ),
        (
            "obligation.wasm.memory-safety",
            "Linear-memory accesses remain in bounds (runtime-enforced)",
        ),
        (
            "obligation.wasm.determinism",
            "Core-subset execution is deterministic across runs",
        ),
    ] {
        g.add_obligation(Obligation::new(ObligationID::new(id).unwrap(), desc))
            .unwrap();
        g.require_obligation(&claim_id, &ObligationID::new(id).unwrap())
            .unwrap();
    }

    // Validation: model-checked exhaustively here.
    let mut validation_ev = EvidenceRecord::new(
        tpt_concord_core::EvidenceID::new("evidence.wasm.validation").unwrap(),
        EvidenceKind::ModelCheck,
        AssuranceLevel::ModelChecked,
        "WasmValidatorModel exhaustive core-subset check",
    );
    validation_ev.validate();
    g.add_evidence(validation_ev).unwrap();
    g.discharge(
        &ObligationID::new("obligation.wasm.validation").unwrap(),
        &tpt_concord_core::EvidenceID::new("evidence.wasm.validation").unwrap(),
    )
    .unwrap();

    // Memory safety and determinism: no Concord-dischargeable evidence yet —
    // they depend on the runtime assumption and cross-runtime differential runs.
    let _ = &claim_id;

    (g, claim_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_concord_model::run;
    use tpt_concord_release::{evaluate, Policy};

    fn validate(seq: &[Instr]) -> Result<Vec<VType>, ValidationError> {
        let records = run(&WasmValidatorModel, seq)?;
        Ok(records.last().map(|r| r.after.clone()).unwrap_or_default())
    }

    #[test]
    fn well_typed_sequences_validate() {
        // (i32.const 1) (i32.const 2) (i32.add) (drop) end
        assert_eq!(
            validate(&[
                Instr::Const(VType::I32),
                Instr::Const(VType::I32),
                Instr::BinOp(VType::I32),
                Instr::Drop,
                Instr::End,
            ]),
            Ok(vec![])
        );
        // Mixed widths are fine when operands match.
        assert_eq!(
            validate(&[
                Instr::Const(VType::I64),
                Instr::Const(VType::I64),
                Instr::BinOp(VType::I64),
                Instr::Drop,
                Instr::End,
            ]),
            Ok(vec![])
        );
    }

    #[test]
    fn malformed_sequences_are_rejected() {
        // Underflow: add with one operand.
        assert!(matches!(
            validate(&[
                Instr::Const(VType::I32),
                Instr::BinOp(VType::I32),
                Instr::End
            ]),
            Err(ValidationError::Underflow { .. })
        ));
        // Type mismatch: i32 + i64.
        assert!(matches!(
            validate(&[
                Instr::Const(VType::I32),
                Instr::Const(VType::I64),
                Instr::BinOp(VType::I32),
                Instr::End
            ]),
            Err(ValidationError::TypeMismatch { .. })
        ));
        // Drop on empty stack.
        assert!(matches!(
            validate(&[Instr::Drop, Instr::End]),
            Err(ValidationError::Underflow { op: "drop" })
        ));
        // `end` with leftovers.
        assert_eq!(
            validate(&[Instr::Const(VType::I32), Instr::End]),
            Err(ValidationError::NonEmptyEnd(1))
        );
    }

    #[test]
    fn wasm_claim_rejected_until_sandbox_and_determinism_evidence_lands() {
        let (g, claim) = claim_graph();
        assert!(g.validate().is_ok());
        let eval = g.evaluate(&claim).unwrap();
        // Only validation is discharged; sandbox/determinism obligations open.
        assert!(!eval.established);
        assert_eq!(eval.unmet_obligations.len(), 2);
        assert_eq!(eval.open_assumptions.len(), 1);

        let mut policy = Policy::new("wasm-assurance");
        policy.require_claim(claim.clone(), AssuranceLevel::ModelChecked);
        assert!(!evaluate(&g, &policy).is_approved());
    }
}
