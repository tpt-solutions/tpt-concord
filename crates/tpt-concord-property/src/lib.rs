// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `tpt-concord-property` — property-based testing derived from
//! specifications (spec §5).
//!
//! Everything is driven by a seeded, deterministic RNG
//! ([`rng::Rng`]): the same seed always produces the same campaign, so any
//! property failure is reproducible and can be recorded as evidence.
//!
//! Three capabilities:
//! - **input generation** via [`Strategy`] implementations,
//! - **stateful generation** that walks an [`AbstractModel`], emitting only
//!   legal input sequences,
//! - **invariant-aware shrinking** that minimizes a failing sequence while
//!   the failure (e.g. an invariant violation) still reproduces.

pub mod rng;
pub mod shrink;
pub mod strategy;

pub use rng::Rng;
pub use shrink::{shrink, ShrinkStats};
pub use strategy::{int_in, pick_from, stateful_sequence, Strategy};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_campaign() {
        let a: Vec<i64> = Rng::new(42)
            .iter()
            .map(|mut r| strategy::int_in(-100, 100).generate(&mut r, 10))
            .take(50)
            .collect();
        let b: Vec<i64> = Rng::new(42)
            .iter()
            .map(|mut r| strategy::int_in(-100, 100).generate(&mut r, 10))
            .take(50)
            .collect();
        assert_eq!(a, b);
        assert!(a.iter().any(|&v| v != a[0]), "values must vary");
    }

    #[test]
    fn different_seeds_differ() {
        let a: Vec<i64> = Rng::new(1)
            .iter()
            .map(|mut r| strategy::int_in(0, 1_000_000).generate(&mut r, 30))
            .take(20)
            .collect();
        let b: Vec<i64> = Rng::new(2)
            .iter()
            .map(|mut r| strategy::int_in(0, 1_000_000).generate(&mut r, 30))
            .take(20)
            .collect();
        assert_ne!(a, b);
    }
}
