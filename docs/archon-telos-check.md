# Telos check: eviction in the Archon WAL model

Written for: Archon maintainers, to review before changing `formal-proofs/wal.telos`.

## What was run

`telos` 0.1.0 (`telos verify`), on three models:

| File | Model | Result |
|---|---|---|
| `docs/telos/wal-archon-current.telos` | Archon's current `wal.telos` (no eviction) | PASS, all constraints satisfied |
| `docs/telos/wal-steal.telos` | Adds `evict_uncommitted`: the pool writes a dirty, uncommitted page to the device | **FAIL**: `invariant Durability maintained: w.uncommitted_on_device == 0` |
| `docs/telos/wal-nosteal.telos` | Adds `evict_clean`: eviction writes only clean pages | PASS, all constraints satisfied |

## What this shows

The current proof is correct for the model it describes. It does not model
eviction, so it cannot see the defect in `docs/archon-findings.md`. Once
eviction is modelled, the verifier rejects the steal behaviour and accepts the
no-steal behaviour that the fix implements.

## Not done

- `wal.telos` and `wal.telos.proof.json` are unchanged. The proof record stores
  the source hash, and regenerating it needs the Telos proof pipeline. The
  no-steal model should replace `wal.telos` only in a PR with the regenerated
  record.
- These models are abstractions. The verifier does not check the Rust code.
  The Concord harness (`crates/tpt-concord-pilot-archon-wal/tests/archon_storage.rs`)
  covers the code by testing.
