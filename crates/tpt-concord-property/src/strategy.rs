// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Input-generation strategies.

use crate::rng::Rng;
use tpt_concord_model::AbstractModel;

/// A reproducible generator of inputs.
///
/// `size` scales the generated values (bigger => wider/deeper); campaigns
/// grow it over time.
pub trait Strategy<I> {
    fn generate(&self, rng: &mut Rng, size: u32) -> I;
}

/// Integers in an inclusive range.
#[derive(Debug, Clone, Copy)]
pub struct IntStrategy {
    min: i64,
    max: i64,
}

/// Strategy: integer in `min..=max`.
pub fn int_in(min: i64, max: i64) -> IntStrategy {
    assert!(min <= max);
    IntStrategy { min, max }
}

impl Strategy<i64> for IntStrategy {
    fn generate(&self, rng: &mut Rng, size: u32) -> i64 {
        // Larger sizes bias toward the extremes to probe boundaries.
        if size >= 8 && rng.next_u64() % 4 == 0 {
            if rng.next_u64() % 2 == 0 {
                self.min
            } else {
                self.max
            }
        } else {
            rng.range(self.min, self.max)
        }
    }
}

/// Pick from a fixed alphabet of values.
#[derive(Debug, Clone)]
pub struct PickStrategy<T> {
    choices: Vec<T>,
}

/// Strategy: uniform choice from `choices`.
pub fn pick_from<T: Clone>(choices: Vec<T>) -> PickStrategy<T> {
    assert!(!choices.is_empty());
    PickStrategy { choices }
}

impl<T: Clone> Strategy<T> for PickStrategy<T> {
    fn generate(&self, rng: &mut Rng, _size: u32) -> T {
        rng.pick(&self.choices).clone()
    }
}

/// **Stateful generation**: walk an [`AbstractModel`], at each step choosing
/// only inputs the model permits from the current state.
///
/// The result is always a legal input sequence of (up to) `len` steps —
/// the stateful counterpart of blind alphabets.
pub fn stateful_sequence<M>(
    model: &M,
    alphabet: &[M::Input],
    rng: &mut Rng,
    len: usize,
) -> Vec<M::Input>
where
    M: AbstractModel,
    M::State: Clone + Ord,
    M::Input: Clone + Ord,
{
    let mut state = model.initial();
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        let legal: Vec<&M::Input> = alphabet
            .iter()
            .filter(|i| !model.transitions(&state, i).is_empty())
            .collect();
        if legal.is_empty() {
            break;
        }
        let input = (*rng.pick(&legal)).clone();
        // Advance deterministically to an arbitrary permitted successor
        // (first in sorted order — the abstract model may be
        // nondeterministic; any successor is legal).
        if let Some(next) = model.transitions(&state, &input).into_iter().next() {
            state = next;
        }
        out.push(input);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    enum St {
        Open,
        Closed,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    enum Op {
        Open,
        Close,
        Boom,
    }

    fn door() -> impl AbstractModel<State = St, Input = Op> {
        #[derive(Clone)]
        struct Door;
        impl AbstractModel for Door {
            type State = St;
            type Input = Op;
            fn initial(&self) -> St {
                St::Closed
            }
            fn transitions(&self, s: &St, i: &Op) -> BTreeSet<St> {
                let mut out = BTreeSet::new();
                match (s, i) {
                    (St::Closed, Op::Open) => {
                        out.insert(St::Open);
                    }
                    (St::Open, Op::Close) => {
                        out.insert(St::Closed);
                    }
                    _ => {}
                }
                out
            }
        }
        Door
    }

    #[test]
    fn stateful_sequences_are_always_legal() {
        let model = door();
        let alphabet = vec![Op::Open, Op::Close, Op::Boom];
        let mut rng = Rng::new(123);
        for _ in 0..200 {
            let seq = stateful_sequence(&model, &alphabet, &mut rng, 12);
            // Every prefix must be executable by the model.
            let mut state = model.initial();
            for input in &seq {
                let nexts = model.transitions(&state, input);
                assert!(!nexts.is_empty(), "illegal input {input:?} in {seq:?}");
                state = nexts.into_iter().next().unwrap();
            }
        }
    }

    #[test]
    fn stateful_never_starts_with_boom() {
        let model = door();
        let alphabet = vec![Op::Open, Op::Boom];
        let mut rng = Rng::new(5);
        let seq = stateful_sequence(&model, &alphabet, &mut rng, 10);
        assert!(!seq.is_empty());
        assert_eq!(seq[0], Op::Open, "Boom is illegal from the initial state");
    }

    #[test]
    fn int_strategy_respects_bounds() {
        let s = int_in(-5, 5);
        for fork in Rng::new(3).iter().take(500) {
            let mut fork = fork;
            let v = s.generate(&mut fork, 1);
            assert!((-5..=5).contains(&v));
        }
    }
}
