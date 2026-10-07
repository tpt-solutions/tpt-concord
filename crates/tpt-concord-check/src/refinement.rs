// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Refinement checking: every concrete behaviour must stay inside the
//! abstract model's permission set.
//!
//! The concrete run is presented post-abstraction `alpha`: a sequence of
//! abstract states `concrete_alpha` with `inputs[i]` the input between
//! states `i` and `i+1`. The check succeeds iff for every `i`, the abstract
//! machine permits `concrete_alpha[i] --inputs[i]--> concrete_alpha[i+1]`.
//! Stuttering (a concrete step invisible at the abstract level) can be
//! allowed or forbidden.

use tpt_concord_model::AbstractModel;
use tpt_concord_spec::Violation;
use tpt_concord_spec::ViolationKind;

/// One refinement failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefinementFailure<S, I> {
    /// Step index (0-based) of the failing transition.
    pub step: usize,
    /// Abstract image of the pre-state.
    pub from: S,
    /// Input taken.
    pub input: I,
    /// Abstract image of the post-state (not permitted).
    pub to: S,
}

/// Outcome of a refinement check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefinementReport<S, I> {
    pub failures: Vec<RefinementFailure<S, I>>,
}

impl<S, I> RefinementReport<S, I> {
    pub fn ok(&self) -> bool {
        self.failures.is_empty()
    }

    /// Convert the first failure into a spec-shaped violation for reporting.
    pub fn first_violation(&self) -> Option<Violation>
    where
        S: std::fmt::Debug,
        I: std::fmt::Debug,
    {
        self.failures.first().map(|f| {
            Violation::new(
                ViolationKind::Permission,
                "refinement",
                format!(
                    "step {f_step}: abstract state {f_from:?} on input {f_input:?} does not permit {f_to:?}",
                    f_step = f.step,
                    f_from = f.from,
                    f_input = f.input,
                    f_to = f.to,
                ),
            )
        })
    }
}

/// Check refinement of a concrete run (post-abstraction) against an abstract
/// model.
///
/// `concrete_alpha` must have exactly `inputs.len() + 1` elements.
pub fn refinement_check<A, I>(
    abstract_machine: &A,
    concrete_alpha: &[A::State],
    inputs: &[I],
    allow_stuttering: bool,
) -> Result<RefinementReport<A::State, I>, crate::CheckError>
where
    A: AbstractModel<Input = I>,
    A::State: Clone + Ord,
    I: Clone + Ord,
{
    if concrete_alpha.len() != inputs.len() + 1 {
        return Err(crate::CheckError::ArityMismatch {
            expected: inputs.len() + 1,
            inputs: inputs.len(),
            got: concrete_alpha.len(),
        });
    }
    let mut failures = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        let from = &concrete_alpha[i];
        let to = &concrete_alpha[i + 1];
        if from == to && allow_stuttering {
            continue;
        }
        let permitted = abstract_machine.transitions(from, input);
        if !permitted.contains(to) {
            failures.push(RefinementFailure {
                step: i,
                from: from.clone(),
                input: input.clone(),
                to: to.clone(),
            });
        }
    }
    Ok(RefinementReport { failures })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_concord_model::AbstractTable;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    enum Level {
        Lo,
        Hi,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    enum Cmd {
        Up,
        Down,
    }

    fn machine() -> AbstractTable<Level, Cmd> {
        let mut m = AbstractTable::new(Level::Lo);
        m.permit(Level::Lo, Cmd::Up, Level::Hi);
        m.permit(Level::Hi, Cmd::Down, Level::Lo);
        m
    }

    #[test]
    fn conforming_run_refines() {
        let run = vec![Level::Lo, Level::Hi, Level::Lo];
        let inputs = vec![Cmd::Up, Cmd::Down];
        let report = refinement_check(&machine(), &run, &inputs, false).unwrap();
        assert!(report.ok());
        assert!(report.first_violation().is_none());
    }

    #[test]
    fn illegal_run_does_not_refine() {
        let run = vec![Level::Lo, Level::Hi, Level::Hi];
        let inputs = vec![Cmd::Up, Cmd::Up];
        let report = refinement_check(&machine(), &run, &inputs, false).unwrap();
        assert!(!report.ok());
        assert_eq!(report.failures[0].step, 1);
        let v = report.first_violation().unwrap();
        assert_eq!(v.kind, ViolationKind::Permission);
    }

    #[test]
    fn stuttering_is_optional() {
        let run = vec![Level::Lo, Level::Lo];
        let inputs = vec![Cmd::Up];
        assert!(refinement_check(&machine(), &run, &inputs, true)
            .unwrap()
            .ok());
        assert!(!refinement_check(&machine(), &run, &inputs, false)
            .unwrap()
            .ok());
    }

    #[test]
    fn arity_is_validated() {
        let err = refinement_check(&machine(), &[Level::Lo], &[Cmd::Up], false).unwrap_err();
        assert!(matches!(err, crate::CheckError::ArityMismatch { .. }));
    }
}
