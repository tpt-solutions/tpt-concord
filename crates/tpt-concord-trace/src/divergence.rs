// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Divergence comparison: where and how two traces disagree.

use crate::event::Trace;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// One point of disagreement between two traces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Divergence {
    /// Event index where the disagreement was observed.
    pub index: usize,
    pub kind: DivergenceKind,
}

/// How the traces disagree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DivergenceKind {
    /// One trace is longer than the other.
    LengthMismatch { a: usize, b: usize },
    /// Same position, different event kind.
    KindMismatch { a: String, b: String },
    /// Same position and kind, different payload.
    PayloadMismatch { a: Json, b: Json },
    /// Same position, different causal parent.
    ParentMismatch {
        a: Option<String>,
        b: Option<String>,
    },
}

impl DivergenceKind {
    /// Human-readable one-line summary.
    pub fn describe(&self) -> String {
        match self {
            DivergenceKind::LengthMismatch { a, b } => {
                format!("length mismatch: {a} vs {b} events")
            }
            DivergenceKind::KindMismatch { a, b } => format!("kind mismatch: `{a}` vs `{b}`"),
            DivergenceKind::PayloadMismatch { .. } => "payload mismatch".to_string(),
            DivergenceKind::ParentMismatch { a, b } => {
                format!("parent mismatch: {:?} vs {:?}", a, b)
            }
        }
    }
}

/// Compare two traces event by event and report every divergence.
///
/// Traces with equal event counts are compared positionally; length
/// differences are reported once as an extra divergence. Causal parents are
/// compared *positionally* (by parent index), not by ID, so a divergence
/// does not spuriously cascade into every subsequent event.
pub fn compare(a: &Trace, b: &Trace) -> Vec<Divergence> {
    fn parent_index(t: &Trace, event: &crate::event::Event) -> Option<usize> {
        event
            .parent
            .as_ref()
            .and_then(|p| t.events.iter().position(|e| &e.id == p))
    }
    let mut out = Vec::new();
    let common = a.events.len().min(b.events.len());
    for i in 0..common {
        let (ea, eb) = (&a.events[i], &b.events[i]);
        if ea.kind != eb.kind {
            out.push(Divergence {
                index: i,
                kind: DivergenceKind::KindMismatch {
                    a: ea.kind.clone(),
                    b: eb.kind.clone(),
                },
            });
            continue;
        }
        if parent_index(a, ea) != parent_index(b, eb) {
            out.push(Divergence {
                index: i,
                kind: DivergenceKind::ParentMismatch {
                    a: ea.parent.as_ref().map(|p| p.as_str().to_string()),
                    b: eb.parent.as_ref().map(|p| p.as_str().to_string()),
                },
            });
            continue;
        }
        if ea.payload != eb.payload {
            out.push(Divergence {
                index: i,
                kind: DivergenceKind::PayloadMismatch {
                    a: ea.payload.clone(),
                    b: eb.payload.clone(),
                },
            });
        }
    }
    if a.events.len() != b.events.len() {
        out.push(Divergence {
            index: common,
            kind: DivergenceKind::LengthMismatch {
                a: a.events.len(),
                b: b.events.len(),
            },
        });
    }
    out
}

/// The first index at which the traces diverge, if any.
pub fn first_divergence(a: &Trace, b: &Trace) -> Option<usize> {
    compare(a, b).into_iter().map(|d| d.index).min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::TraceBuilder;
    use serde_json::json;

    fn trace(kinds: &[&str], payload: Json) -> Trace {
        let mut b = TraceBuilder::new();
        for k in kinds {
            b.event(*k, payload.clone());
        }
        b.finish()
    }

    #[test]
    fn identical_traces_have_no_divergence() {
        let a = trace(&["x", "y"], json!({}));
        let b = trace(&["x", "y"], json!({}));
        assert_eq!(compare(&a, &b), Vec::new());
        assert_eq!(first_divergence(&a, &b), None);
    }

    #[test]
    fn kind_mismatch_is_localized() {
        let a = trace(&["x", "y"], json!({}));
        let b = trace(&["x", "z"], json!({}));
        let divs = compare(&a, &b);
        assert_eq!(divs.len(), 1);
        assert_eq!(divs[0].index, 1);
        assert_eq!(
            divs[0].kind,
            DivergenceKind::KindMismatch {
                a: "y".into(),
                b: "z".into()
            }
        );
    }

    #[test]
    fn payload_mismatch_is_detected() {
        let mut a = TraceBuilder::new();
        a.event("x", json!({"v": 1}));
        let mut b = TraceBuilder::new();
        b.event("x", json!({"v": 2}));
        let divs = compare(&a.finish(), &b.finish());
        assert_eq!(divs.len(), 1);
        assert!(matches!(
            divs[0].kind,
            DivergenceKind::PayloadMismatch { .. }
        ));
    }

    #[test]
    fn length_mismatch_reported_once() {
        let a = trace(&["x", "y", "z"], json!({}));
        let b = trace(&["x", "y"], json!({}));
        let divs = compare(&a, &b);
        assert_eq!(divs.len(), 1);
        assert_eq!(divs[0].kind, DivergenceKind::LengthMismatch { a: 3, b: 2 });
    }

    #[test]
    fn divergence_serializes() {
        let a = trace(&["x"], json!({}));
        let b = trace(&["y"], json!({}));
        let divs = compare(&a, &b);
        let json = serde_json::to_string(&divs).unwrap();
        let back: Vec<Divergence> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, divs);
    }
}
