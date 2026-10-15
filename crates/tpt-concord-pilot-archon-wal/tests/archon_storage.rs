// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Concord against the real Archon storage engine (spec §11 / §12).
//!
//! The implementation under test is `tpt-archon-core` at a pinned commit
//! (see the dev-dependency in `Cargo.toml`), not a model written here.
//!
//! The specification is written from Archon's documented contract:
//!
//! > After recovery the device reflects exactly the durable, committed writes.
//!
//! "Durable" means the write's `Commit` record is intact in the log that
//! survived the crash. The crash model is: the log is cut at an arbitrary
//! byte offset, and the data device is whatever a file-backed device held
//! when the process died.
//!
//! Findings this file pins. Each is a test that passes while the defect is
//! present, so the suite stays green and a fix makes the test fail, which is
//! the signal to flip it:
//!
//! - `eviction_leaves_uncommitted_data_on_the_device`: the buffer pool writes
//!   dirty pages back before commit, and recovery is redo-only, so uncommitted
//!   data stays on the device after recovery.
//! - `raw_commit_flushes_data_before_the_log_is_durable`: `StorageEngine::commit`
//!   flushes data before the caller can persist the commit record. The
//!   `Database` facade avoids this by persisting the log first.
//!
//! The check is a bounded exhaustive sweep: every operation sequence up to
//! length [`MAX_LEN`], every cut point that matters (each record's start,
//! first byte, last byte and end), and two buffer-pool sizes:
//!
//! - capacity 4 holds both blocks, so nothing is evicted before a commit;
//! - capacity 1 forces dirty-page eviction, i.e. a steal policy.

use std::path::{Path, PathBuf};

use tpt_archon_core::block::FileBlockDevice;
use tpt_archon_core::page::PAGE_SIZE;
use tpt_archon_core::storage::StorageEngine;
use tpt_concord_check::Counterexample;
use tpt_concord_core::{
    AssuranceGraph, AssuranceLevel, Claim, ClaimID, EvidenceID, EvidenceKind, EvidenceRecord,
    ImplementationID, Obligation, ObligationID, Scope, ScopeID, SpecificationID,
};
use tpt_concord_evidence::{EvidenceBundleBuilder, Section, ToolchainIdentity};
use tpt_concord_release::{evaluate, GateDecision, Policy, RejectionReason};
use tpt_concord_spec::{Violation, ViolationKind};
use tpt_concord_trace::TraceBuilder;

/// Longest operation sequence swept exhaustively.
const MAX_LEN: usize = 4;
const BLOCKS: u64 = 2;

/// Archon's on-disk framing (`wal.rs`): u32 length, u64 lsn, u8 kind, u64
/// block id, u32 payload length, payload, u32 crc. A `Commit` has no payload.
const FRAME_OVERHEAD: usize = 4 + 8 + 1 + 8 + 4 + 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Write { block: u8, value: u8 },
    Commit,
}

const ALPHABET: [Op; 5] = [
    Op::Write { block: 0, value: 1 },
    Op::Write { block: 0, value: 2 },
    Op::Write { block: 1, value: 1 },
    Op::Write { block: 1, value: 2 },
    Op::Commit,
];

/// Byte length of each record a sequence produces, in log order.
fn record_lengths(ops: &[Op]) -> Vec<usize> {
    ops.iter()
        .map(|op| match op {
            Op::Write { .. } => FRAME_OVERHEAD + PAGE_SIZE,
            Op::Commit => FRAME_OVERHEAD,
        })
        .collect()
}

/// The specification: the block contents recovery must produce when the log
/// is cut at `cut` bytes. Only writes covered by a `Commit` that ends at or
/// before `cut` survive.
fn spec_state(ops: &[Op], cut: usize) -> [u8; BLOCKS as usize] {
    let mut end = 0usize;
    let mut current = [0u8; BLOCKS as usize];
    let mut durable = [0u8; BLOCKS as usize];
    for (op, len) in ops.iter().zip(record_lengths(ops)) {
        end += len;
        match *op {
            Op::Write { block, value } => current[block as usize] = value,
            Op::Commit => {
                if end <= cut {
                    durable = current;
                }
            }
        }
    }
    durable
}

fn page(byte: u8) -> Vec<u8> {
    vec![byte; PAGE_SIZE]
}

/// A scratch directory unique to this test run.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("concord-archon-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Every sequence of length `1..=max_len` over [`ALPHABET`], shortest first.
fn sequences(max_len: usize) -> Vec<Vec<Op>> {
    let mut out = Vec::new();
    let mut frontier: Vec<Vec<Op>> = vec![vec![]];
    for _ in 0..max_len {
        let mut next = Vec::new();
        for seq in &frontier {
            for op in ALPHABET {
                let mut s = seq.clone();
                s.push(op);
                next.push(s);
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    out
}

/// How the caller drives the engine and when it makes the log durable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protocol {
    /// The sidecar protocol `Database::put` follows: a commit appends its
    /// marker, the log is persisted, and only then is data flushed.
    Caller,
    /// The raw `StorageEngine::commit()`: it flushes data itself, so the
    /// caller can only persist the log after the data is already on the device.
    RawCommit,
    /// The log is persisted after every write, not only at commits. Recovery
    /// must then drop uncommitted `PageWrite` records that are durable in the
    /// log but have no following `Commit`.
    PersistEachWrite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Mismatch {
    ops: Vec<Op>,
    /// Durable log length at the crash, in bytes.
    durable_len: usize,
    expected: [u8; BLOCKS as usize],
    actual: [u8; BLOCKS as usize],
}

struct Sweep {
    checked: usize,
    mismatches: Vec<Mismatch>,
}

/// Run every sequence through the real engine. After each operation the
/// process "crashes": the device image is whatever the file holds, and the
/// durable log is the prefix the caller had persisted by then. Recovery from
/// that pair must equal the committed state of that prefix.
fn sweep(capacity: usize, protocol: Protocol, max_len: usize, dir: &Path) -> Sweep {
    let image_path = dir.join("image.bin");
    let mut checked = 0usize;
    let mut mismatches = Vec::new();

    for ops in sequences(max_len) {
        let _ = std::fs::remove_file(&image_path);
        let device = FileBlockDevice::create(&image_path, BLOCKS).expect("create image");
        let mut engine = StorageEngine::new(device, capacity);
        let mut durable_len = 0usize;

        for k in 0..ops.len() {
            let prefix = &ops[..=k];
            match ops[k] {
                Op::Write { block, value } => {
                    engine
                        .write_page(block as u64, &page(value))
                        .expect("write_page");
                    if protocol == Protocol::PersistEachWrite {
                        durable_len = engine.wal_bytes().len();
                    }
                    checked += 1;
                    if let Some(m) = crash_check(&engine, &image_path, durable_len, prefix) {
                        mismatches.push(m);
                    }
                }
                Op::Commit => match protocol {
                    Protocol::Caller | Protocol::PersistEachWrite => {
                        engine.append_commit().expect("append_commit");
                        // Crash with the commit marker written but not yet
                        // persisted: the writes before it are durable, the
                        // commit is not, so none of them may survive.
                        checked += 1;
                        if let Some(m) = crash_check(&engine, &image_path, durable_len, prefix) {
                            mismatches.push(m);
                        }
                        let persisted = engine.wal_bytes().len();
                        engine.flush_data().expect("flush_data");
                        durable_len = persisted;
                        checked += 1;
                        if let Some(m) = crash_check(&engine, &image_path, durable_len, prefix) {
                            mismatches.push(m);
                        }
                    }
                    Protocol::RawCommit => {
                        // `commit()` is `append_commit()` then `flush_data()`.
                        // The caller can only persist the log after it
                        // returns, so the crash window is the flush itself.
                        engine.append_commit().expect("append_commit");
                        engine.flush_data().expect("flush_data");
                        checked += 1;
                        if let Some(m) = crash_check(&engine, &image_path, durable_len, prefix) {
                            mismatches.push(m);
                        }
                        durable_len = engine.wal_bytes().len();
                        checked += 1;
                        if let Some(m) = crash_check(&engine, &image_path, durable_len, prefix) {
                            mismatches.push(m);
                        }
                    }
                },
            }
        }
    }
    Sweep {
        checked,
        mismatches,
    }
}

/// Crash now: the device image is whatever the file holds, and the durable
/// log is the first `durable_len` bytes. Recover from that pair and compare
/// with the committed state of the operations so far.
fn crash_check(
    engine: &StorageEngine<FileBlockDevice>,
    image_path: &Path,
    durable_len: usize,
    prefix: &[Op],
) -> Option<Mismatch> {
    let record_path = image_path.with_file_name("recovered.bin");
    let image = std::fs::read(image_path).expect("image");
    let log = &engine.wal_bytes()[..durable_len];
    std::fs::write(&record_path, &image).expect("write image copy");
    let device = FileBlockDevice::open(&record_path).expect("reopen image");
    let mut recovered = StorageEngine::new(device, BLOCKS as usize);
    recovered
        .recover(log)
        .expect("recover never fails on a valid log");
    let mut actual = [0u8; BLOCKS as usize];
    for (block, slot) in actual.iter_mut().enumerate() {
        *slot = recovered.read_page(block as u64).expect("read")[0];
    }
    let expected = spec_state(prefix, durable_len);
    (actual != expected).then(|| Mismatch {
        ops: prefix.to_vec(),
        durable_len,
        expected,
        actual,
    })
}

/// Render an operation sequence as a short human-readable label.
fn label(ops: &[Op]) -> String {
    ops.iter()
        .map(|op| match op {
            Op::Write { block, value } => format!("write(b{block}={value})"),
            Op::Commit => "commit".to_string(),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Turn a mismatch into Concord's counterexample object. The input is the
/// operation sequence, the trace records the surviving log cut, and the
/// violation names the broken obligation.
fn counterexample(m: &Mismatch, capacity: usize, protocol: Protocol) -> Counterexample {
    let mut trace = TraceBuilder::new();
    trace.event(
        "crash",
        serde_json::json!({
            "durable_log_bytes": m.durable_len,
            "buffer_pool_capacity": capacity,
            "protocol": format!("{protocol:?}"),
        }),
    );
    let mut env = std::collections::BTreeMap::new();
    env.insert("buffer_pool_capacity".into(), capacity.to_string());
    env.insert("protocol".into(), format!("{protocol:?}"));
    env.insert("archon_rev".into(), ARCHON_REV.into());
    Counterexample::new(
        format!(
            "cx.archon.recovery.{protocol:?}.cap{capacity}.log{}",
            m.durable_len
        ),
        SpecificationID::new("spec.archon.recovery").unwrap(),
        ImplementationID::new("impl.archon-core").unwrap(),
        None,
        serde_json::json!({ "ops": label(&m.ops) }),
        trace.finish(),
        env,
        Violation::new(
            ViolationKind::Invariant,
            "recovery-matches-committed-state",
            format!(
                "after recovery block contents are {:?}, the committed state is {:?}",
                m.actual, m.expected
            ),
        ),
    )
}

/// The Archon revision under test. Keep in sync with the dev-dependency.
const ARCHON_REV: &str = "928c00dec13560844a52222e3d02107516a7c09d";

#[test]
fn spec_framing_matches_archon_record_sizes() {
    // Cheap guard: one committed write must be exactly the predicted bytes.
    let ops = [Op::Write { block: 0, value: 7 }, Op::Commit];
    let image = scratch("framing").join("framing.bin");
    let device = FileBlockDevice::create(&image, BLOCKS).unwrap();
    let mut engine = StorageEngine::new(device, 4);
    engine.write_page(0, &page(7)).unwrap();
    engine.commit().unwrap();
    assert_eq!(
        engine.wal_bytes().len(),
        record_lengths(&ops).iter().sum::<usize>()
    );
}

#[test]
fn sidecar_protocol_without_eviction_matches_the_spec() {
    let dir = scratch("no-eviction");
    let result = sweep(4, Protocol::Caller, MAX_LEN, &dir);
    assert!(
        result.checked >= sequences(MAX_LEN).len(),
        "every sequence should be checked at least once, checked {}",
        result.checked
    );
    assert!(
        result.mismatches.is_empty(),
        "{} mismatches without eviction, first: {:?}",
        result.mismatches.len(),
        result.mismatches.first()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn persisting_each_write_tests_commit_gating_without_eviction() {
    // Uncommitted writes are durable in the log here, so recovery must drop
    // them. Cuts that split commits are covered because the durable length
    // moves after every record, not only at commits.
    let dir = scratch("persist-each");
    let result = sweep(4, Protocol::PersistEachWrite, MAX_LEN, &dir);
    assert!(
        result.mismatches.is_empty(),
        "{} mismatches with uncommitted-but-durable writes, first: {:?}",
        result.mismatches.len(),
        result.mismatches.first()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn eviction_leaves_uncommitted_data_on_the_device() {
    let dir = scratch("eviction");
    let result = sweep(1, Protocol::Caller, MAX_LEN, &dir);
    assert!(
        !result.mismatches.is_empty(),
        "expected the eviction defect to reproduce"
    );

    // Sequences are enumerated shortest first, so the first mismatch is a
    // minimal counterexample.
    let minimal = &result.mismatches[0];
    assert_eq!(
        minimal.ops.len(),
        2,
        "minimal case: {}",
        label(&minimal.ops)
    );
    assert!(
        matches!(minimal.ops[..], [Op::Write { .. }, Op::Write { .. }]),
        "minimal case should be two uncommitted writes: {}",
        label(&minimal.ops)
    );
    assert_eq!(minimal.durable_len, 0, "nothing was persisted");
    assert_eq!(minimal.expected, [0, 0], "nothing was committed");
    assert_eq!(
        minimal.actual[0], 1,
        "the evicted, uncommitted write is still on the device after recovery"
    );

    let cx = counterexample(minimal, 1, Protocol::Caller);
    let json = cx.to_json().expect("serializes");
    let back = Counterexample::from_json(&json).expect("round-trips");
    assert_eq!(back.id, cx.id);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_commit_flushes_data_before_the_log_is_durable() {
    let dir = scratch("raw-commit");
    let result = sweep(4, Protocol::RawCommit, 2, &dir);
    let minimal = result
        .mismatches
        .first()
        .expect("raw commit() must reproduce the ordering hazard");
    assert_eq!(
        minimal.ops,
        vec![Op::Write { block: 0, value: 1 }, Op::Commit],
        "minimal case is one write and its commit"
    );
    assert_eq!(
        minimal.durable_len, 0,
        "the caller had not yet persisted the log"
    );
    assert_eq!(minimal.expected, [0, 0]);
    assert_eq!(minimal.actual[0], 1, "the data reached the device first");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Claims and evidence for the release decision.
///
/// The two claims differ only in scope: recovery with no eviction is backed
/// by the exhaustive sweep, while recovery under eviction is not established.
fn archon_graph(
    no_eviction_passed: bool,
    eviction_passed: bool,
) -> (AssuranceGraph, ClaimID, ClaimID) {
    let mut g = AssuranceGraph::new();
    let no_steal_scope = ScopeID::new("scope.archon.no-eviction").unwrap();
    g.add_scope(Scope::new(
        no_steal_scope.clone(),
        "Buffer pool never evicts a dirty page before the commit that covers it",
    ))
    .unwrap();

    let no_steal = ClaimID::new("claim.archon.recovery.no-eviction").unwrap();
    let mut c = Claim::new(
        no_steal.clone(),
        "Recovery restores exactly the committed writes when no dirty page is evicted before commit",
    );
    c.scopes.push(no_steal_scope);
    g.add_claim(c).unwrap();
    let ob1 = ObligationID::new("obligation.archon.recovery.sweep").unwrap();
    g.add_obligation(Obligation::new(
        ob1.clone(),
        "Bounded exhaustive sweep over every operation sequence up to length 4 and every cut point",
    ))
    .unwrap();
    g.require_obligation(
        &ClaimID::new("claim.archon.recovery.no-eviction").unwrap(),
        &ob1,
    )
    .unwrap();
    let ev1 = EvidenceID::new("evidence.archon.recovery.sweep").unwrap();
    let mut rec = EvidenceRecord::new(
        ev1.clone(),
        EvidenceKind::ModelCheck,
        AssuranceLevel::ModelChecked,
        "Exhaustive bounded sweep of the real tpt-archon-core engine (not a proof)",
    );
    if no_eviction_passed {
        rec.validate();
    } else {
        rec.reject();
    }
    g.add_evidence(rec).unwrap();
    g.discharge(&ob1, &ev1).unwrap();

    let general = ClaimID::new("claim.archon.recovery.general").unwrap();
    g.add_claim(Claim::new(
        general.clone(),
        "Recovery restores exactly the committed writes under any buffer-pool policy",
    ))
    .unwrap();
    let ob2 = ObligationID::new("obligation.archon.recovery.eviction-sweep").unwrap();
    g.add_obligation(Obligation::new(
        ob2.clone(),
        "Bounded exhaustive sweep with dirty-page eviction (buffer capacity 1)",
    ))
    .unwrap();
    g.require_obligation(&general, &ob2).unwrap();
    let ev2 = EvidenceID::new("evidence.archon.recovery.eviction-sweep").unwrap();
    let mut rec2 = EvidenceRecord::new(
        ev2.clone(),
        EvidenceKind::ModelCheck,
        AssuranceLevel::ModelChecked,
        "Eviction sweep: reproduces the device-state defect",
    );
    if eviction_passed {
        rec2.validate();
    } else {
        rec2.reject();
    }
    g.add_evidence(rec2).unwrap();
    g.discharge(&ob2, &ev2).unwrap();

    (g, no_steal, general)
}

#[test]
fn release_gate_approves_the_scoped_claim_and_blocks_the_general_one() {
    // The no-eviction sweep is clean; the eviction sweep found the defect.
    let (graph, no_steal, general) = archon_graph(true, false);

    let mut scoped = Policy::new("archon-no-eviction-release");
    scoped.require_claim(no_steal.clone(), AssuranceLevel::ModelChecked);
    assert!(
        evaluate(&graph, &scoped).is_approved(),
        "the scoped claim is supported by a clean sweep"
    );

    let mut general_policy = Policy::new("archon-general-release");
    general_policy.require_claim(general.clone(), AssuranceLevel::ModelChecked);
    match evaluate(&graph, &general_policy) {
        GateDecision::Rejected { reasons } => assert!(
            reasons.iter().any(|r| matches!(
                r,
                RejectionReason::ClaimUnestablished { claim, .. } if *claim == general
            )),
            "expected the general claim to be unestablished: {reasons:?}"
        ),
        GateDecision::Approved(_) => panic!("the defect must block the general release"),
    }
}

#[test]
fn evidence_bundle_records_the_sweep_and_counterexample() {
    let dir = scratch("bundle");
    let steal = sweep(1, Protocol::Caller, 2, &dir);
    let minimal = steal.mismatches.first().expect("defect reproduces");
    let cx_json = counterexample(minimal, 1, Protocol::Caller)
        .to_json()
        .unwrap();
    let bundle_dir = dir.join("bundle");
    let bundle = EvidenceBundleBuilder::new("bundle.archon-storage")
        .toolchain(ToolchainIdentity::new("archon-core", ARCHON_REV))
        .artifact(
            Section::FuzzCampaigns,
            EvidenceID::new("evidence.archon.recovery.eviction-sweep").unwrap(),
            EvidenceKind::Other("bounded_sweep".into()),
            "eviction-sweep.json",
            "Bounded sweep with buffer capacity 1 (max length 2)",
            format!(
                "{{\"checked\": {}, \"mismatches\": {}}}",
                steal.checked,
                steal.mismatches.len()
            )
            .into_bytes(),
        )
        .unwrap()
        .artifact(
            Section::TestResults,
            EvidenceID::new("evidence.archon.recovery.counterexample").unwrap(),
            EvidenceKind::Other("counterexample".into()),
            "counterexample.json",
            "Minimal counterexample: uncommitted writes survive recovery on the device",
            cx_json.into_bytes(),
        )
        .unwrap()
        .write(&bundle_dir)
        .expect("write bundle");
    assert_eq!(bundle.artifacts().count(), 2);
    tpt_concord_evidence::read_and_verify(&bundle_dir).expect("hashes verify");
    let _ = std::fs::remove_dir_all(&dir);
}
