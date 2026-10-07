// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! A small, seeded, deterministic RNG (xorshift64*) — no platform
//! dependence, so campaigns are reproducible across machines.

/// Deterministic xorshift64* generator.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Create a generator from a non-zero seed.
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x9E3779B97F4A7C15 } else { seed },
        }
    }

    /// Next raw 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Uniform value in `0..n` (n must be > 0).
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below() requires n > 0");
        // Rejection sampling keeps the distribution uniform.
        let limit = u64::MAX - (u64::MAX % n);
        loop {
            let v = self.next_u64();
            if v < limit {
                return v % n;
            }
        }
    }

    /// A value in `min..=max`.
    pub fn range(&mut self, min: i64, max: i64) -> i64 {
        assert!(min <= max);
        min + self.below((max - min + 1) as u64) as i64
    }

    /// Pick a random element.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }

    /// Endless stream of derived generators: each element forks the current
    /// state so strategies can be applied step by step.
    pub fn iter(self) -> RngIter {
        RngIter { rng: self }
    }
}

/// Iterator of forked generators (see [`Rng::iter`]).
pub struct RngIter {
    rng: Rng,
}

impl Iterator for RngIter {
    type Item = Rng;
    fn next(&mut self) -> Option<Rng> {
        let fork = Rng::new(self.rng.next_u64());
        Some(fork)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_stays_in_bounds_and_covers() {
        let mut rng = Rng::new(7);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..1000 {
            let v = rng.range(-3, 3);
            assert!((-3..=3).contains(&v));
            seen.insert(v);
        }
        assert_eq!(seen.len(), 7, "all values in the range should appear");
    }

    #[test]
    fn below_is_uniform_enough() {
        let mut rng = Rng::new(9);
        let mut counts = [0usize; 4];
        for _ in 0..4000 {
            counts[rng.below(4) as usize] += 1;
        }
        for c in counts {
            assert!(c > 700, "bucket too sparse: {counts:?}");
        }
    }

    #[test]
    fn zero_seed_is_valid() {
        // Same seed => identical sequences; successive outputs differ.
        let mut a = Rng::new(0);
        let mut b = Rng::new(0);
        let a1 = a.next_u64();
        let b1 = b.next_u64();
        assert_eq!(a1, b1);
        assert_ne!(a.next_u64(), a1, "consecutive values differ");
    }
}
