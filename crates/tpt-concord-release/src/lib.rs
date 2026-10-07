// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `tpt-concord-release` — the release gate (spec §5).
//!
//! A gate answers one question honestly: *may this be released, and if not,
//! exactly what is missing?* Policy declares required claims (with minimum
//! assurance levels) and required obligations; evaluation walks the
//! Assurance Graph and returns a structured decision — approval or a list of
//! machine-readable rejection reasons.

pub mod gate;
pub mod pipeline;

pub use gate::{evaluate, ClaimRequirement, GateDecision, GateSummary, Policy, RejectionReason};
pub use pipeline::{AcceptanceTrace, Stage, StageOutcome, StageReport};
