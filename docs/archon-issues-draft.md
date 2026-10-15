# Draft issues for tpt-archon (not filed)

Written for: the tpt-archon maintainers, filed through the repo's bug report
template, as `CONTRIBUTING.md` requires. Not yet posted. Each issue has a
minimal repro and the branch that fixes it.

---

## Issue 1: `StorageEngine` writes uncommitted pages to the device on eviction

**Which crate?** `tpt-archon-core`

**What happened?**
`StorageEngine` uses a steal buffer pool. When a frame is evicted, a dirty
page is written back to the device even if its transaction has not committed.
Recovery is redo-only, so it never undoes that write. After a crash the device
holds uncommitted data, and `recover` does not remove it.

**Expected behavior**
After recovery the device holds exactly the committed writes. Uncommitted data
must not be on the device.

**Repro**

```rust
use tpt_archon_core::block::FileBlockDevice;
use tpt_archon_core::page::PAGE_SIZE;
use tpt_archon_core::storage::StorageEngine;

let path = std::env::temp_dir().join("archon-evict.bin");
let _ = std::fs::remove_file(&path);
let log;
{
    let dev = FileBlockDevice::create(&path, 4).unwrap();
    let mut e = StorageEngine::new(dev, 1); // one frame
    e.write_page(0, &vec![0x01; PAGE_SIZE]).unwrap(); // dirty, uncommitted
    e.write_page(1, &vec![0x01; PAGE_SIZE]).unwrap(); // evicts block 0 to the file
    log = e.wal_bytes().to_vec();
    // crash: no commit
}
let dev = FileBlockDevice::open(&path).unwrap();
let mut r = StorageEngine::new(dev, 2);
assert_eq!(r.recover(&log).unwrap(), 0);
assert_eq!(r.read_page(0).unwrap()[0], 0x00); // fails: reads 0x01
```

**Environment**
- Commit: `928c00d`
- Crate: `tpt-archon-core`, `std` feature

**Notes**
- `Database::put` is not affected today, because it commits each page before
  the next write.
- Any multi-page transaction built on `StorageEngine` is affected.
- The replay fix already in `TODO.md` stops uncommitted records from being
  replayed, but the device has already been written.
- `formal-proofs/wal.telos` has no eviction transition, so the proof does not
  cover this path.

**Fix:** branch `fix/storage-no-steal-commit-order`, commit `86ff808`. The
pool becomes no-steal for `StorageEngine`. A write that would need an
uncommitted page evicted fails with `UncommittedPagesPinned` before anything
is logged.

---

## Issue 2: `StorageEngine::commit` flushes data before the log is durable

**Which crate?** `tpt-archon-core`

**What happened?**
`commit()` appends the commit marker and flushes the data pages itself. The
caller can persist the log only after `commit()` returns, so a crash in
between leaves data on the device whose commit record was never saved.

**Expected behavior**
Write-ahead ordering holds: the log is durable before any data page is flushed.

**Repro**

```rust
use tpt_archon_core::block::FileBlockDevice;
use tpt_archon_core::page::PAGE_SIZE;
use tpt_archon_core::storage::StorageEngine;

let path = std::env::temp_dir().join("archon-commit.bin");
let _ = std::fs::remove_file(&path);
{
    let dev = FileBlockDevice::create(&path, 4).unwrap();
    let mut e = StorageEngine::new(dev, 4);
    e.write_page(0, &vec![0x01; PAGE_SIZE]).unwrap();
    e.commit().unwrap(); // flushes block 0; the log has not been persisted
    // crash here: the persisted log is empty
}
let dev = FileBlockDevice::open(&path).unwrap();
let mut r = StorageEngine::new(dev, 2);
assert_eq!(r.recover(&[]).unwrap(), 0);
assert_eq!(r.read_page(0).unwrap()[0], 0x00); // fails: reads 0x01
```

**Environment**
- Commit: `928c00d`

**Notes**
`Database::put` avoids this: it appends the marker, persists the log, and only
then flushes.

**Fix:** same branch and commit as Issue 1. `commit()` is replaced by
`commit_durable(persist)`, which takes the persist step as a closure. This is a
breaking change, so it needs a changelog entry, and it is recorded under
`### Changed`.

---

## Before filing

- [ ] Confirm neither issue is already in `TODO.md` as a deferred item.
- [ ] Decide whether to link the branch or attach it as a PR. The contribution
      policy says any code change is scoped and merged from an issue.
- [ ] Confirm `tpt-archon-core` repro output matches the expected values on the
      target toolchain. These were run on Windows with `std`.
