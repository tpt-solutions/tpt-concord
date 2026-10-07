// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Canonical execution traces (spec §5, `tpt-concord-trace`).
//!
//! A [`Trace`] is a content-addressed sequence of events:
//! - **event identity**: every event's ID is the SHA-256 of its canonical
//!   encoding (`seq`, parent, kind, payload) — tamper-evident by construction;
//! - **causal ordering**: events may reference a parent event, always
//!   earlier in the trace;
//! - **normalization**: payloads are canonically serialized (sorted keys) so
//!   equal states have equal encodings;
//! - **replay**: fold a trace over a step function, verifying integrity first;
//! - **divergence comparison**: locate and classify where two traces disagree.

pub mod divergence;
pub mod event;
pub mod replay;

pub use divergence::{compare, Divergence, DivergenceKind};
pub use event::{verify_integrity, Event, EventID, Trace, TraceBuilder};
pub use replay::replay;

/// Errors from trace construction, integrity verification and replay.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TraceError {
    #[error("integrity failure at event {index}: {reason}")]
    Integrity { index: usize, reason: String },
    #[error("replay failed at step {step}: {reason}")]
    Replay { step: usize, reason: String },
    #[error("traces diverge at index {index}: {reason}")]
    Divergence { index: usize, reason: String },
}
