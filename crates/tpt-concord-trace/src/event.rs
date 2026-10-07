// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Events, content addressing, and trace construction/verification.

use crate::TraceError;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;
use tpt_concord_core::TraceID;

/// Content-addressed event identity: SHA-256 hex of the canonical encoding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EventID(String);

impl EventID {
    /// Wrap a pre-computed hex digest (validated: 64 lowercase hex chars).
    pub fn from_hex(s: impl Into<String>) -> Result<Self, TraceError> {
        let s = s.into();
        if s.len() == 64
            && s.bytes().all(|b| b.is_ascii_hexdigit())
            && s.bytes().all(|b| !b.is_ascii_uppercase())
        {
            Ok(Self(s))
        } else {
            Err(TraceError::Integrity {
                index: 0,
                reason: format!("invalid event id {s:?}: expected 64 lowercase hex chars"),
            })
        }
    }

    /// Compute the ID of an event from its parts.
    pub fn compute(seq: u64, parent: Option<&EventID>, kind: &str, payload: &Json) -> Self {
        let canonical =
            serde_json::json!([seq, parent.as_ref().map(|p| p.as_str()), kind, payload,]);
        Self::sha256_hex(canonical.to_string().as_bytes())
    }

    /// SHA-256 of bytes, lowercase hex.
    pub fn sha256_hex(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let digest = hasher.finalize();
        Self(hex::encode(digest))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for EventID {
    type Error = TraceError;
    fn try_from(s: String) -> Result<Self, TraceError> {
        Self::from_hex(s)
    }
}

impl From<EventID> for String {
    fn from(id: EventID) -> String {
        id.0
    }
}

impl FromStr for EventID {
    type Err = TraceError;
    fn from_str(s: &str) -> Result<Self, TraceError> {
        Self::from_hex(s)
    }
}

impl fmt::Display for EventID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One canonical trace event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Position in the trace; events are strictly ordered by `seq`.
    pub seq: u64,
    /// Content address (see [`EventID::compute`]).
    pub id: EventID,
    /// Causal parent, always an earlier event in the same trace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<EventID>,
    /// What happened (free-form label, part of the identity).
    pub kind: String,
    /// Canonical payload — JSON with sorted keys (serde_json default map).
    pub payload: Json,
}

impl Event {
    /// Construct an event and compute its content address.
    pub fn new(seq: u64, parent: Option<EventID>, kind: impl Into<String>, payload: Json) -> Self {
        let kind = kind.into();
        let id = EventID::compute(seq, parent.as_ref(), &kind, &payload);
        Self {
            seq,
            id,
            parent,
            kind,
            payload,
        }
    }
}

/// A canonical, content-addressed trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    /// Content address over the ordered event IDs.
    pub id: TraceID,
    /// Events ordered by `seq` starting at 0.
    pub events: Vec<Event>,
}

impl Trace {
    /// Compute the trace identity: SHA-256 over the concatenation of the
    /// ordered event IDs.
    pub fn compute_id(events: &[Event]) -> TraceID {
        let mut hasher = Sha256::new();
        for event in events {
            hasher.update(event.id.as_str().as_bytes());
        }
        TraceID::new(hex::encode(hasher.finalize())).expect("hex digest is a valid TraceID")
    }

    /// Wrap already-built events into a trace (computing its ID).
    pub fn from_events(events: Vec<Event>) -> Self {
        let id = Self::compute_id(&events);
        Self { id, events }
    }

    /// The empty trace has a well-defined identity.
    pub fn empty() -> Self {
        Self::from_events(Vec::new())
    }
}

/// Builder for incrementally constructing a trace.
#[derive(Debug, Default)]
pub struct TraceBuilder {
    events: Vec<Event>,
}

impl TraceBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an event with the previous event as causal parent.
    pub fn event(&mut self, kind: impl Into<String>, payload: Json) -> &mut Self {
        let parent = self.events.last().map(|e| e.id.clone());
        self.event_with_parent(kind, payload, parent);
        self
    }

    /// Append an event with an explicit causal parent (or none).
    pub fn event_with_parent(
        &mut self,
        kind: impl Into<String>,
        payload: Json,
        parent: Option<EventID>,
    ) -> &mut Self {
        let seq = self.events.len() as u64;
        self.events.push(Event::new(seq, parent, kind, payload));
        self
    }

    /// Finalize into a [`Trace`].
    pub fn finish(&self) -> Trace {
        Trace::from_events(self.events.clone())
    }
}

/// Verify structural integrity of a trace:
/// - `seq` is contiguous starting at 0,
/// - every event ID matches its recomputed content address,
/// - every causal parent exists and precedes its child.
pub fn verify_integrity(trace: &Trace) -> Result<(), TraceError> {
    for (index, event) in trace.events.iter().enumerate() {
        if event.seq != index as u64 {
            return Err(TraceError::Integrity {
                index,
                reason: format!("seq {} out of order (expected {index})", event.seq),
            });
        }
        let expected = EventID::compute(
            event.seq,
            event.parent.as_ref(),
            &event.kind,
            &event.payload,
        );
        if expected != event.id {
            return Err(TraceError::Integrity {
                index,
                reason: "event id does not match its content (tampered or non-canonical)"
                    .to_string(),
            });
        }
        if let Some(parent) = &event.parent {
            let parent_index = trace.events.iter().position(|e| &e.id == parent);
            match parent_index {
                None => {
                    return Err(TraceError::Integrity {
                        index,
                        reason: format!("parent {parent} not found in trace"),
                    })
                }
                Some(p) if p >= index => {
                    return Err(TraceError::Integrity {
                        index,
                        reason: format!("parent {parent} does not precede event"),
                    })
                }
                Some(_) => {}
            }
        }
    }
    // Trace ID must match the events too.
    let expected_trace_id = Trace::compute_id(&trace.events);
    if expected_trace_id != trace.id {
        return Err(TraceError::Integrity {
            index: trace.events.len(),
            reason: "trace id does not match its events".to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_trace() -> Trace {
        let mut b = TraceBuilder::new();
        b.event("append", json!({"record": "a", "len": 1}));
        b.event("commit", json!({"committed_len": 1}));
        b.finish()
    }

    #[test]
    fn event_ids_are_content_addresses() {
        let t1 = sample_trace();
        let t2 = sample_trace();
        assert_eq!(t1, t2, "same events -> same trace");
        assert_eq!(t1.events[0].id.as_str().len(), 64);
    }

    #[test]
    fn payload_key_order_does_not_matter() {
        let mut a = TraceBuilder::new();
        a.event(
            "append",
            serde_json::from_value::<Json>(json!({"a": 1, "b": 2})).unwrap(),
        );
        let mut c = TraceBuilder::new();
        c.event(
            "append",
            serde_json::from_value::<Json>(json!({"b": 2, "a": 1})).unwrap(),
        );
        assert_eq!(a.finish(), c.finish(), "canonicalization sorts keys");
    }

    #[test]
    fn different_payload_different_id() {
        let mut a = TraceBuilder::new();
        a.event("append", json!({"record": "a"}));
        let mut c = TraceBuilder::new();
        c.event("append", json!({"record": "b"}));
        assert_ne!(a.finish(), c.finish());
    }

    #[test]
    fn integrity_passes_on_well_formed_trace() {
        let t = sample_trace();
        assert!(verify_integrity(&t).is_ok());
        assert!(verify_integrity(&Trace::empty()).is_ok());
    }

    #[test]
    fn tampered_payload_is_detected() {
        let mut t = sample_trace();
        t.events[0].payload = json!({"record": "a", "len": 999});
        assert!(verify_integrity(&t).is_err());
    }

    #[test]
    fn bad_parent_is_detected() {
        let ghost = EventID::sha256_hex(b"ghost");
        let mut b = TraceBuilder::new();
        b.event_with_parent("append", json!({}), Some(ghost));
        let t = b.finish();
        let err = verify_integrity(&t).unwrap_err();
        assert!(matches!(err, TraceError::Integrity { .. }));
    }

    #[test]
    fn forward_parent_is_detected() {
        let t = sample_trace();
        let mut broken = vec![t.events[1].clone()];
        broken.insert(
            0,
            Event::new(0, Some(t.events[1].id.clone()), "x", json!({})),
        );
        let t2 = Trace::from_events(broken);
        assert!(verify_integrity(&t2).is_err());
    }

    #[test]
    fn trace_id_covers_all_events() {
        let t = sample_trace();
        let mut tampered = t.clone();
        tampered.events.pop();
        assert_ne!(Trace::compute_id(&tampered.events), t.id);
        assert!(verify_integrity(&tampered).is_err(), "id must mismatch");
    }

    #[test]
    fn event_id_hex_validation() {
        assert!(EventID::from_hex("0123abcd").is_err());
        let good = "a".repeat(64);
        assert!(EventID::from_hex(good.clone()).is_ok());
        assert!(EventID::from_hex(good.to_uppercase()).is_err());
    }
}
