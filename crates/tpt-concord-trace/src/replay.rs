// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Deterministic replay: fold a verified trace over a step function.
//!
//! Replay is the reproduction primitive behind the whole assurance story:
//! the same trace, replayed against the same step semantics, must always
//! produce the same result.

use crate::event::Trace;
use crate::TraceError;

/// Replay a trace deterministically.
///
/// Integrity is verified first (content addresses, causal order); then the
/// events are folded over `step` starting from `initial`. Any `step` failure
/// aborts replay with the failing step index.
pub fn replay<S, F>(trace: &Trace, initial: S, mut step: F) -> Result<S, TraceError>
where
    F: FnMut(S, &crate::event::Event) -> Result<S, String>,
{
    crate::event::verify_integrity(trace)?;
    let mut state = initial;
    for (index, event) in trace.events.iter().enumerate() {
        state = step(state, event).map_err(|reason| TraceError::Replay {
            step: index,
            reason,
        })?;
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use crate::event::{Trace, TraceBuilder};
    use crate::replay::replay;
    use serde_json::json;

    #[test]
    fn replay_folds_events_in_order() {
        let mut b = TraceBuilder::new();
        b.event("add", json!({"delta": 2}));
        b.event("add", json!({"delta": 3}));
        b.event("sub", json!({"delta": 1}));
        let t = b.finish();

        let total = replay(&t, 0i64, |acc, ev| {
            let delta = ev.payload["delta"].as_i64().unwrap();
            match ev.kind.as_str() {
                "add" => Ok(acc + delta),
                "sub" => Ok(acc - delta),
                other => Err(format!("unknown kind {other}")),
            }
        })
        .unwrap();
        assert_eq!(total, 4);
    }

    #[test]
    fn replay_failure_reports_step() {
        let mut b = TraceBuilder::new();
        b.event("boom", json!({}));
        let t = b.finish();
        let err = replay(&t, (), |_, ev| {
            if ev.kind == "boom" {
                Err("explode".to_string())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert!(matches!(err, crate::TraceError::Replay { step: 0, .. }));
    }

    #[test]
    fn replay_is_deterministic() {
        let mut b = TraceBuilder::new();
        for i in 0..10 {
            b.event("add", json!({"delta": i}));
        }
        let t = b.finish();
        let f =
            |acc: i64, ev: &crate::event::Event| Ok(acc + ev.payload["delta"].as_i64().unwrap());
        assert_eq!(replay(&t, 0, f).unwrap(), replay(&t, 0, f).unwrap());
    }

    #[test]
    fn replay_refuses_tampered_trace() {
        let mut b = TraceBuilder::new();
        b.event("add", json!({"delta": 2}));
        let mut t = b.finish();
        t.events[0].payload = json!({"delta": 3});
        assert!(replay(&t, 0i64, |acc, _| -> Result<i64, String> { Ok(acc + 1) }).is_err());
    }

    #[test]
    fn empty_trace_replays_to_initial() {
        let t = Trace::empty();
        let s = replay(&t, 42, |_, _| -> Result<i64, String> { unreachable!() }).unwrap();
        assert_eq!(s, 42);
    }
}
