// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `tpt-concord-contract` — optional runtime contracts (spec §5).
//!
//! Precondition, postcondition and invariant checks executable at runtime.
//! **Runtime contracts are enforcement, not proof**: a checked run that
//! passes is not evidence of correctness beyond the states actually visited
//! (that is `type_checked`-family instrumentation, never `proven`).
//!
//! Enforcement modes:
//! - [`Enforcement::Always`] — checks run in debug *and* release;
//! - [`Enforcement::DebugOnly`] — checks run under `debug_assertions` and
//!   are compiled out of release (zero-cost instrumentation).

use std::marker::PhantomData;
use thiserror::Error;
use tpt_concord_model::{DeterministicMachine, StepOutcome};

/// A single contract violation with the failing clause's name.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("contract violation `{kind}` `{name}`: {detail}")]
pub struct ContractViolation {
    pub kind: &'static str,
    pub name: String,
    pub detail: String,
}

/// When contract checks execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enforcement {
    /// Check in debug and release builds.
    Always,
    /// Check only under `debug_assertions`; release builds skip the checks.
    DebugOnly,
}

impl Enforcement {
    /// Whether checks execute in the current build.
    pub fn active(&self) -> bool {
        match self {
            Enforcement::Always => true,
            Enforcement::DebugOnly => cfg!(debug_assertions),
        }
    }
}

/// A named state predicate.
pub type Predicate<S> = (String, Box<dyn Fn(&S) -> Result<(), String>>);

/// Runtime contract over a machine's states: invariants checked before and
/// after every step, plus per-operation pre/post predicates selected by the
/// caller.
pub struct Contract<S> {
    pub invariants: Vec<Predicate<S>>,
    pub preconditions: Vec<Predicate<S>>,
    pub postconditions: Vec<Predicate<S>>,
}

impl<S> Default for Contract<S> {
    fn default() -> Self {
        Self {
            invariants: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        }
    }
}

impl<S> Contract<S> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a named invariant (holds in every observed state).
    pub fn invariant(
        mut self,
        name: impl Into<String>,
        f: impl Fn(&S) -> Result<(), String> + 'static,
    ) -> Self {
        self.invariants.push((name.into(), Box::new(f)));
        self
    }

    /// Add a named precondition (checked before every step).
    pub fn precondition(
        mut self,
        name: impl Into<String>,
        f: impl Fn(&S) -> Result<(), String> + 'static,
    ) -> Self {
        self.preconditions.push((name.into(), Box::new(f)));
        self
    }

    /// Add a named postcondition (checked after every step).
    pub fn postcondition(
        mut self,
        name: impl Into<String>,
        f: impl Fn(&S) -> Result<(), String> + 'static,
    ) -> Self {
        self.postconditions.push((name.into(), Box::new(f)));
        self
    }

    fn run(
        predicates: &[Predicate<S>],
        kind: &'static str,
        state: &S,
    ) -> Result<(), ContractViolation> {
        for (name, f) in predicates {
            if let Err(detail) = f(state) {
                return Err(ContractViolation {
                    kind,
                    name: name.clone(),
                    detail,
                });
            }
        }
        Ok(())
    }

    /// Check invariants only.
    pub fn check_invariants(&self, state: &S) -> Result<(), ContractViolation> {
        Self::run(&self.invariants, "invariant", state)
    }

    /// Check invariants + preconditions.
    pub fn check_pre(&self, state: &S) -> Result<(), ContractViolation> {
        self.check_invariants(state)?;
        Self::run(&self.preconditions, "precondition", state)
    }

    /// Check invariants + postconditions.
    pub fn check_post(&self, state: &S) -> Result<(), ContractViolation> {
        self.check_invariants(state)?;
        Self::run(&self.postconditions, "postcondition", state)
    }
}

/// A machine wrapped in a runtime contract.
pub struct Checked<'a, M: DeterministicMachine> {
    inner: &'a M,
    contract: Contract<M::State>,
    enforcement: Enforcement,
    _state: PhantomData<M::State>,
}

impl<'a, M: DeterministicMachine> Checked<'a, M> {
    /// Wrap `machine` with `contract`, enforced per `enforcement`.
    pub fn new(machine: &'a M, contract: Contract<M::State>, enforcement: Enforcement) -> Self {
        Self {
            inner: machine,
            contract,
            enforcement,
            _state: PhantomData,
        }
    }

    /// Whether checks are active in this build.
    pub fn checks_active(&self) -> bool {
        self.enforcement.active()
    }

    /// Execute one step under the contract.
    ///
    /// With checks active: invariants+preconditions on the before-state,
    /// invariants+postconditions on the after-state; any failure is a
    /// [`ContractViolation`] error. With checks inactive (release +
    /// `DebugOnly`), this is a direct delegation.
    pub fn step(
        &self,
        state: &M::State,
        input: &M::Input,
    ) -> Result<StepOutcome<M::State>, ContractError<M::Error>> {
        if self.enforcement.active() {
            self.contract
                .check_pre(state)
                .map_err(ContractError::Violation)?;
            let outcome = self
                .inner
                .step(state, input)
                .map_err(ContractError::Machine)?;
            self.contract
                .check_post(&outcome.next)
                .map_err(ContractError::Violation)?;
            Ok(outcome)
        } else {
            self.inner
                .step(state, input)
                .map_err(ContractError::Machine)
        }
    }
}

/// Errors from a checked step.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ContractError<M> {
    #[error(transparent)]
    Violation(#[from] ContractViolation),
    #[error("machine error: {0}")]
    Machine(M),
}

/// Guard macros for imperative contract checks.
///
/// `require!(cond, "detail {}", args..)` — a precondition; returns
/// `Err(ContractViolation)` from the enclosing function when false.
#[macro_export]
macro_rules! require {
    ($cond:expr, $name:expr, $($detail:tt)*) => {
        if !$cond {
            return Err($crate::ContractViolation {
                kind: "precondition",
                name: $name.to_string(),
                detail: format!($($detail)*),
            });
        }
    };
}

/// `ensure!(cond, name, detail..)` — a postcondition.
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $name:expr, $($detail:tt)*) => {
        if !$cond {
            return Err($crate::ContractViolation {
                kind: "postcondition",
                name: $name.to_string(),
                detail: format!($($detail)*),
            });
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Stack {
        items: Vec<i64>,
    }

    /// Stack machine with a capacity of 2.
    struct BoundedStack;
    impl DeterministicMachine for BoundedStack {
        type State = Stack;
        type Input = i64; // >0 push, <0 pop, 0 no-op
        type Error = String;
        fn initial_state(&self) -> Stack {
            Stack { items: Vec::new() }
        }
        fn step(&self, s: &Stack, i: &i64) -> Result<StepOutcome<Stack>, String> {
            let mut items = s.items.clone();
            if *i > 0 {
                if items.len() >= 2 {
                    return Err("overflow".to_string());
                }
                items.push(*i);
            } else if *i < 0 {
                items.pop().ok_or("underflow")?;
            }
            Ok(StepOutcome::to(Stack { items }))
        }
    }

    fn contract() -> Contract<Stack> {
        Contract::<Stack>::new()
            .invariant("bounded", |s: &Stack| {
                if s.items.len() <= 2 {
                    Ok(())
                } else {
                    Err(format!("len {}", s.items.len()))
                }
            })
            .precondition("alive", |s| {
                if !s.items.contains(&666) {
                    Ok(())
                } else {
                    Err("poisoned".into())
                }
            })
            .postcondition("no_negative", |s| {
                if s.items.iter().all(|&x| x >= 0) {
                    Ok(())
                } else {
                    Err("negative item".into())
                }
            })
    }

    #[test]
    fn happy_path_passes_all_clauses() {
        let checked = Checked::new(&BoundedStack, contract(), Enforcement::Always);
        let s = checked.step(&BoundedStack.initial_state(), &1).unwrap();
        assert_eq!(s.next.items, vec![1]);
        assert!(checked.checks_active());
    }

    #[test]
    fn precondition_violation_is_reported() {
        let checked = Checked::new(&BoundedStack, contract(), Enforcement::Always);
        let poisoned = Stack { items: vec![666] };
        let err = checked.step(&poisoned, &1).unwrap_err();
        match err {
            ContractError::Violation(v) => {
                assert_eq!(v.kind, "precondition");
                assert_eq!(v.name, "alive");
            }
            other => panic!("expected violation, got {other:?}"),
        }
    }

    #[test]
    fn postcondition_violation_is_reported() {
        // The machine itself permits negatives; the contract rejects them.
        struct NaughtyStack;
        impl DeterministicMachine for NaughtyStack {
            type State = Stack;
            type Input = i64;
            type Error = String;
            fn initial_state(&self) -> Stack {
                Stack { items: Vec::new() }
            }
            fn step(&self, _s: &Stack, _i: &i64) -> Result<StepOutcome<Stack>, String> {
                Ok(StepOutcome::to(Stack { items: vec![-1] }))
            }
        }
        let checked = Checked::new(&NaughtyStack, contract(), Enforcement::Always);
        let err = checked.step(&NaughtyStack.initial_state(), &0).unwrap_err();
        match err {
            ContractError::Violation(v) => {
                assert_eq!(v.kind, "postcondition");
                assert_eq!(v.name, "no_negative");
            }
            other => panic!("expected violation, got {other:?}"),
        }
    }

    #[test]
    fn debug_only_enforcement_follows_build_profile() {
        let checked = Checked::new(&BoundedStack, contract(), Enforcement::DebugOnly);
        assert_eq!(checked.checks_active(), cfg!(debug_assertions));
        // In either case the step itself must behave.
        let s = checked.step(&BoundedStack.initial_state(), &2).unwrap();
        assert_eq!(s.next.items, vec![2]);
    }

    #[test]
    fn require_and_ensure_macros() {
        fn transfer(from: &i64, amount: &i64) -> Result<i64, ContractViolation> {
            require!(
                *from >= *amount,
                "sufficient_funds",
                "balance {from} < {amount}"
            );
            Ok(from - amount)
        }
        assert_eq!(transfer(&10, &4).unwrap(), 6);
        let err = transfer(&1, &5).unwrap_err();
        assert_eq!(err.kind, "precondition");
        assert_eq!(err.name, "sufficient_funds");
        assert_eq!(err.detail, "balance 1 < 5");

        fn broken() -> Result<(), ContractViolation> {
            ensure!(1 == 2, "impossible", "math broke");
            Ok(())
        }
        assert!(broken().is_err());
    }
}
