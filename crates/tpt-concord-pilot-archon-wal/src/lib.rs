// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `tpt-concord-pilot-archon-wal` — the Phase 1 milestone pilot (spec §11).
//!
//! First real Claim + Assurance Graph, proved end-to-end against an
//! Archon-style WAL/transaction subsystem:
//!
//! 1. a machine-readable WAL [`wal::wal_specification`],
//! 2. a deterministic reference model [`wal::WalModel`],
//! 3. simulated implementations ([`wal::ConformantWal`], [`wal::NaiveWal`]),
//! 4. canonical traces with content-addressed events,
//! 5. trace conformance + counterexample lifecycle,
//! 6. an exhaustive small-space model check of the recovery claim,
//! 7. an evidence bundle on disk,
//! 8. an Assurance Graph and a release-gate decision.
//!
//! The real Archon integration (running this pipeline against the actual
//! `tpt-archon` storage engine) is Phase 5; this pilot pins down the shape of
//! that work.

pub mod pipeline;
pub mod wal;
