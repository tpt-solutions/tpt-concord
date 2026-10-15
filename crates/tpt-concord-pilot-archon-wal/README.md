# tpt-concord-pilot-archon-wal

Milestone pilot: Archon WAL/transaction claim + Assurance Graph end-to-end (spec section 11).

Part of [TPT Concord](https://github.com/tpt-solutions/tpt-concord) — the evidence and conformance layer that
connects specifications, claims, proofs, tests and release decisions.

## Overview

`tpt-concord-pilot-archon-wal` — the Phase 1 milestone pilot (spec §11).

First real Claim + Assurance Graph, proved end-to-end against an
Archon-style WAL/transaction subsystem:

1. a machine-readable WAL [`wal::wal_specification`],
2. a deterministic reference model [`wal::WalModel`],
3. simulated implementations ([`wal::ConformantWal`], [`wal::NaiveWal`]),
4. canonical traces with content-addressed events,
5. trace conformance + counterexample lifecycle,
6. an exhaustive small-space model check of the recovery claim,
7. an evidence bundle on disk,
8. an Assurance Graph and a release-gate decision.

The real Archon integration (running this pipeline against the actual
`tpt-archon` storage engine) is Phase 5; this pilot pins down the shape of
that work.

## Against the real Archon engine

`tests/archon_storage.rs` checks `tpt-archon-core` (pinned to commit `928c00d`
as a dev-dependency) against its documented recovery contract. It runs an
exhaustive sweep of every write and commit sequence up to length 4, under
several caller protocols and buffer sizes.

It found two defects, both pinned as passing tests that assert the defect and
will fail once it is fixed. Full write-up and reproduction:
[`docs/archon-findings.md`](../../docs/archon-findings.md).

- Uncommitted pages evicted to the device survive recovery (no undo).
- `StorageEngine::commit` flushes data before the caller can persist the log.

## License

Dual-licensed under **MIT OR Apache-2.0**. Copyright TPT Solutions.
