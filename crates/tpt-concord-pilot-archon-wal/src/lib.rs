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
//! The real Archon storage engine is checked in `tests/archon_storage.rs`:
//! the `tpt-archon-core` crate is a dev-dependency pinned to an exact commit,
//! and the specification there is written from Archon's documented recovery
//! contract. That run found a defect, pinned as an expected failure; see the
//! crate README.

pub mod pipeline;
pub mod wal;
