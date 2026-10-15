# Archon storage engine: findings from the Concord pilot

Checked against `tpt-solutions/tpt-archon` at commit
`928c00dec13560844a52222e3d02107516a7c09d`.

**Status:** both defects are fixed on branch `fix/storage-no-steal-commit-order`
of `tpt-solutions/tpt-archon` (commit `86ff808`). The pilot pins that commit.
See "Fix" at the end.

Written for: Archon maintainers. The method, the spec and the reproduction are
in `crates/tpt-concord-pilot-archon-wal/tests/archon_storage.rs`.

## What was checked

The specification is Archon's own recovery contract, taken from the doc
comment on `StorageEngine::recover`: after recovery the device reflects
exactly the durable, committed writes. A write is durable when its `Commit`
record is intact in the log that survived the crash.

The check runs the real engine on a file-backed device. After each step it
simulates a crash: the device is the file as it stands, and the log is the
prefix the caller had persisted. Recovery must then match the spec. Sequences
of writes and commits are enumerated exhaustively up to length 4, under two
buffer sizes and four caller protocols.

## Verified (no defect found)

For every sequence up to length 4:

- **Sidecar protocol, no eviction.** Append commit, persist log, flush data.
  Recovery matches the spec at every crash point.
- **Uncommitted but durable writes are dropped.** With the log persisted after
  every write, recovery drops writes that have no following commit. This is
  the fix recorded in `TODO.md` ("WAL replay didn't check for a Commit
  marker"), and it holds.
- **Torn commit marker.** A crash after the commit is appended but before it
  is persisted leaves the writes uncommitted, and recovery drops them.

## Defect 1 (high): uncommitted data survives recovery when pages are evicted

`BufferPool` writes a dirty frame back to the device when it evicts it. That
can happen before the transaction commits. Recovery is redo-only: it applies
committed `PageWrite` records and never undoes anything on the device. So an
uncommitted page image that was evicted before the crash stays on disk after
recovery.

Minimal reproduction (buffer capacity 1):

```
write_page(block 0, 0x01)    // dirty in the pool
write_page(block 1, 0x01)    // evicts block 0 to the device
crash                        // no commit anywhere
recover(log)                 // applies 0 pages
read_page(0)[0]              // 0x01; committed state is 0x00
```

- Not covered by the fix in `TODO.md`. That fix stops uncommitted records
  from being replayed, but the device already holds the bytes.
- Not covered by `formal-proofs/wal.telos`. The model has no eviction
  transition, so its `applied` counter is not the device state that eviction
  changes. The proof is valid for the model it describes (the hash in
  `wal.telos.proof.json` matches the file), but that model does not describe
  this code path.
- Facade impact: `Database::put` commits each page before the next write
  and flushes at each commit, so it avoids the uncommitted window today.
  Any multi-page transaction built on `StorageEngine` is exposed.

Suggested fixes, in order of simplicity:
1. Pin dirty frames until their transaction commits (no-steal). Recovery
   then never sees uncommitted device bytes.
2. Add an undo record before eviction, and roll back uncommitted pages during
   recovery (steal with undo).
3. Either way, add eviction and undo transitions to `wal.telos`, so the
   proof covers the behaviour that fails.

## Defect 2 (medium): `StorageEngine::commit` flushes before the log is durable

`commit()` appends the commit marker and then flushes data itself. The caller
can only persist the log after `commit()` returns, which is after the data is
already on the device. A crash in that window leaves data on the device whose
commit record is not durable.

Reproduction (buffer capacity 4, no eviction):

```
write_page(block 0, 0x01)
commit()                     // flushes block 0; the log is not yet persisted
crash                        // persisted log is empty
recover(empty log)           // applies 0 pages; block 0 reads 0x01, committed state is 0x00
```

`Database::put` avoids this: it appends the marker, persists the log, and only
then flushes. The hazard is in the raw `StorageEngine` API.

Suggested fix: remove `commit()` in favour of `append_commit()` and
`flush_data()`, or have `commit()` take a callback that persists the log
before the flush.

## Reproducing

```
cargo test -p tpt-concord-pilot-archon-wal --test archon_storage
```

The two defect tests pass while the defects are present. They assert the
defect and will fail once it is fixed. That is the signal to flip them.

## Fix

Commit `86ff808` on `fix/storage-no-steal-commit-order` in `tpt-archon`:

- `StorageEngine` uses a no-steal buffer pool. Eviction never writes a dirty,
  uncommitted page, and a write that would need one fails with
  `UncommittedPagesPinned` before anything is logged.
- `commit()` is replaced by `commit_durable(persist)`, which appends the commit
  marker, persists the log, then flushes. If `persist` fails the marker is
  withdrawn.
- `write_page` checks bounds and reserves a frame before logging, and withdraws
  its record if staging fails.

The workspace gate passes on the branch: fmt, clippy with `-D warnings`, 314
tests, the `no_std` build and the publish dry run. Concord's harness passes
against the fixed branch (7 tests, exhaustive sweep, no mismatches), with
refused writes exercised under eviction pressure.

