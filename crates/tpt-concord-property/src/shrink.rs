// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Invariant-aware shrinking: minimize a failing input sequence while the
//! observed failure still reproduces.
//!
//! Delta-debugging style: try removing contiguous chunks (from the largest
//! granularity down), keep any removal that preserves the failure. The
//! failure predicate `still_fails` is caller-supplied so the shrink target
//! can be anything — an invariant violation, a conformance divergence, a
//! crash — as long as it is deterministic.

/// Statistics from a shrink run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShrinkStats {
    /// Length of the input sequence before shrinking.
    pub original_len: usize,
    /// Length after shrinking.
    pub minimal_len: usize,
    /// Number of candidate sequences tried.
    pub attempts: usize,
}

/// Shrink `inputs` while `still_fails` reports the same failure.
///
/// `still_fails` must be deterministic (re-running the same input must give
/// the same answer) — which Concord guarantees anywhere upstream by using
/// seeded generation and deterministic machines.
pub fn shrink<I: Clone, F>(inputs: &[I], mut still_fails: F) -> (Vec<I>, ShrinkStats)
where
    F: FnMut(&[I]) -> bool,
{
    let original_len = inputs.len();
    let mut current: Vec<I> = inputs.to_vec();
    let mut attempts = 0usize;

    if !still_fails(&current) {
        // Nothing to shrink if the original doesn't fail (defensive).
        let minimal_len = current.len();
        return (
            current,
            ShrinkStats {
                original_len,
                minimal_len,
                attempts,
            },
        );
    }

    let mut chunk = current.len().max(1) / 2;
    while chunk >= 1 {
        let mut i = 0;
        while i < current.len() {
            let end = (i + chunk).min(current.len());
            let mut candidate: Vec<I> = Vec::with_capacity(current.len());
            candidate.extend_from_slice(&current[..i]);
            candidate.extend_from_slice(&current[end..]);
            attempts += 1;
            if still_fails(&candidate) {
                current = candidate;
                // Restart the scan at the same index with the shorter
                // sequence, easing off the chunk size.
                chunk = (chunk * 3) / 4;
                if chunk == 0 {
                    chunk = 1;
                }
            } else {
                i += 1;
            }
        }
        if chunk == 1 && current.len() <= 1 {
            break;
        }
        chunk /= 2;
    }

    let minimal_len = current.len();
    (
        current,
        ShrinkStats {
            original_len,
            minimal_len,
            attempts,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinks_to_single_counterexample() {
        let input: Vec<usize> = (0..50).collect();
        let (minimal, stats) = shrink(&input, |seq| seq.contains(&3));
        assert_eq!(minimal, vec![3]);
        assert_eq!(stats.original_len, 50);
        assert_eq!(stats.minimal_len, 1);
        assert!(stats.attempts > 0);
    }

    #[test]
    fn shrink_preserves_arbitrary_failure_pattern() {
        // Failure: sequence contains 7 immediately followed by 9.
        let fails = |seq: &[usize]| seq.windows(2).any(|w| w[0] == 7 && w[1] == 9);
        let mut input: Vec<usize> = (0..30).collect();
        input.push(7);
        input.push(9);
        let (minimal, _) = shrink(&input, fails);
        assert!(fails(&minimal));
        assert!(
            minimal.len() <= 4,
            "minimal should be tiny, got {minimal:?}"
        );
    }

    #[test]
    fn non_failing_input_is_returned_unchanged() {
        let input = vec![1, 2, 3];
        let (minimal, stats) = shrink(&input, |seq| seq.contains(&99));
        assert_eq!(minimal, vec![1, 2, 3]);
        assert_eq!(stats.attempts, 0);
    }

    #[test]
    fn empty_input_is_fine() {
        let (minimal, _) = shrink::<usize, _>(&[], |_| false);
        assert!(minimal.is_empty());
    }
}
