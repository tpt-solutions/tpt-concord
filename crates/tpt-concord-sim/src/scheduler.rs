// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Virtual time and controlled scheduling.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An event scheduled in virtual time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledEvent<P> {
    /// Virtual time the event fires at.
    pub at: u64,
    /// Global insertion sequence — the tie-breaker that makes order total.
    pub seq: u64,
    pub payload: P,
}

/// A deterministic, virtual-time scheduler.
///
/// Events fire in `(time, seq)` order: earlier time first; ties break by
/// insertion order. Nothing here touches a wall clock, so a schedule is
/// replayable data.
#[derive(Debug, Clone)]
pub struct Scheduler<P> {
    next_seq: u64,
    queue: BTreeMap<(u64, u64), P>,
}

impl<P> Default for Scheduler<P> {
    fn default() -> Self {
        Self {
            next_seq: 0,
            queue: BTreeMap::new(),
        }
    }
}

impl<P> Scheduler<P> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedule `payload` to fire at virtual time `at`.
    pub fn schedule(&mut self, at: u64, payload: P) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.queue.insert((at, seq), payload);
        seq
    }

    /// Pop the next event if it fires at or before `until`.
    pub fn pop_next(&mut self, until: u64) -> Option<ScheduledEvent<P>> {
        let key = *self.queue.keys().next()?;
        if key.0 > until {
            return None;
        }
        let payload = self.queue.remove(&key)?;
        Some(ScheduledEvent {
            at: key.0,
            seq: key.1,
            payload,
        })
    }

    /// Drain every event firing at or before `until`, in order, applying `f`.
    pub fn drain_until<F: FnMut(ScheduledEvent<P>)>(&mut self, until: u64, mut f: F) {
        while let Some(event) = self.pop_next(until) {
            f(event);
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Virtual time of the next event to fire.
    pub fn next_time(&self) -> Option<u64> {
        self.queue.keys().next().map(|k| k.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_in_time_order_then_insertion_order() {
        let mut s = Scheduler::new();
        s.schedule(10, "late-a");
        s.schedule(5, "early");
        s.schedule(10, "late-b");
        s.schedule(5, "early-2");

        let mut fired = Vec::new();
        s.drain_until(100, |e| fired.push((e.at, e.payload)));
        let expected: Vec<(u64, &str)> =
            vec![(5, "early"), (5, "early-2"), (10, "late-a"), (10, "late-b")];
        assert_eq!(fired, expected);
    }

    #[test]
    fn respects_virtual_time_horizon() {
        let mut s = Scheduler::new();
        s.schedule(5, "a");
        s.schedule(50, "b");
        s.drain_until(10, |e| assert_eq!(e.payload, "a"));
        assert_eq!(s.len(), 1);
        assert_eq!(s.next_time(), Some(50));
        s.drain_until(10, |_| panic!("must not fire past horizon"));
        s.drain_until(50, |e| assert_eq!(e.payload, "b"));
        assert!(s.is_empty());
    }

    #[test]
    fn schedules_are_replayable_data() {
        // The same schedule drives two schedulers identically.
        let build = || {
            let mut s = Scheduler::new();
            s.schedule(3, 1);
            s.schedule(1, 2);
            s.schedule(2, 3);
            s
        };
        let mut a = build();
        let mut b = build();
        let mut out_a = Vec::new();
        let mut out_b = Vec::new();
        a.drain_until(10, |e| out_a.push(e.seq));
        b.drain_until(10, |e| out_b.push(e.seq));
        assert_eq!(out_a, out_b);
        assert_eq!(
            out_a,
            vec![1, 2, 0],
            "fires by time; seq breaks ties (insertion order)"
        );
    }
}
