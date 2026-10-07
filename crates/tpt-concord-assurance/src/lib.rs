// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `tpt-concord-assurance` — assurance infrastructure for the TPT ecosystem
//! (spec §12).
//!
//! For each subsystem this crate provides the Concord side of the assurance
//! relationship: machine-readable [`Specification`]s of its behavioral
//! guarantees, deterministic reference [`DeterministicMachine`] models where
//! the behavior is modelable offline, and [`AssuranceGraph`] claim structures
//! whose scope/assumption sets encode the subsystem's *honestly stated
//! maturity* (each module cites the subsystem repo it models).
//!
//! The specialized verification systems remain authoritative for their
//! proofs (Telos, Eidos): where a subsystem delegates a guarantee to them,
//! the evidence here is recorded as a reference and stays `Unvalidated`
//! until its artifact re-verifies. Wiring the real implementations into
//! conformance runs happens in each subsystem's repo; this crate pins the
//! claims they must answer to.
//!
//! | Module | Subsystem | Source |
//! |---|---|---|
//! | [`archon`] | proof-native storage/kernel/db stack | `tpt-solutions/tpt-archon` |
//! | [`boxcar`] | sandbox/proxy/eBPF/distiller/mesh monorepo | `tpt-solutions/tpt-boxcar` |
//! | [`keystone`] | multi-engine Postgres-compatible data platform | `tpt-solutions/tpt-keystone-db` |
//! | [`fabric`] | distributed substrate (intents/epochs/deterministic sim) | `tpt-solutions/tpt-fabric` |
//! | [`repro`] | content-addressable reproducible computation | `tpt-solutions/tpt-repro` |
//! | [`wasm`] | Wasm validation & determinism | Boxcar mesh / `tpt-uir` targets |
//! | [`crypto`] | protocol-structure claims (nonces, SCRAM, framing) | Keystone auth / Archon WAL framing / tpt-shatter |
//! | [`network`] | session-typed channels, connection lifecycle | `tpt-solutions/tpt-pulse`, Keystone wire |
//! | [`science`] | integrators, spectral round-trips, estimation | `tpt-physics-engine`, `tpt-system-zero`, `tpt-ignis` |

pub mod archon;
pub mod boxcar;
pub mod crypto;
pub mod fabric;
pub mod keystone;
pub mod network;
pub mod repro;
pub mod science;
pub mod wasm;
