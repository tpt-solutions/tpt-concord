// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Concord against the real Archon storage engine (spec §11 / §12).
//!
//! The implementation under test is `tpt-archon-core` at a pinned commit (see
//! the dev-dependency in `Cargo.toml`), not a model written here.
//!
//! The specification is written from Archon's documented contract:
//!
//! > After recovery the device reflects exactly the durable, committed writes.
//!
//! "Durable" means the write's `Commit` record is intact in the log that
//! survived the crash. The crash model is the sidecar protocol that
//! `Database::put` follows: the log is persisted before data is flushed, and a
//! crash leaves the device image as it was and the log as far as it had been
//! persisted.
//!
//! The check is a bounded exhaustive sweep. Every operation sequence up to
//! length [`MAX_LEN`] is run through the real engine, and recovery is checked
//! at each crash point. The crash points are: after each write, after a commit
//! marker is written but before it is persisted, after it is persisted but
//! before the flush, and after the flush.
//!
//! History: the first sweep (against `928c00d`) found two defects, both fixed
//! upstream: uncommitted pages evicted to the device survived recovery, and
//! `StorageEngine::commit` flushed data before the log was persisted. This file
//! pins the fixed behaviour. A regression fails it.

use std::path::{Path, PathBuf};

use tpt_archon_core::block::{FileBlockDevice, StorageError};
use tpt_archon_core::page::PAGE_SIZE;
use tpt_archon_core::storage::StorageEngine;
use tpt_concord_core::{
    AssuranceGraph, AssuranceLevel, Claim, ClaimID, EvidenceID, EvidenceKind, EvidenceRecord,
    Obligation, ObligationID, Scope, ScopeID,
};
use tpt_concord_evidence::{EvidenceBundleBuilder, Section, ToolchainIdentity};
use tpt_concord_release::{evaluate, GateDecision, Policy, RejectionReason};

/// Longest operation sequence swept exhaustively.
const MAX_LEN: usize = 4;
const BLOCKS: u64 = 2;

/// The Archon revision under test. Keep in sync with the dev-dependency.
const ARCHON_REV: &str = "86ff808c5906d749558a4068bbed11629d566406";

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

/// How the caller makes the log durable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protocol {
    /// Commits go through `commit_durable`. Writes are not persisted on their
    /// own, which is what `Database::put` does.
    Sidecar,
    /// The log is persisted after every write as well as at commits. Recovery
    /// must then drop uncommitted writes that are durable in the log.
    PersistEachWrite,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Mismatch {
    ops: Vec<Op>,
    /// Durable log length at the crash, in bytes.
    durable_len: usize,
    expected: [u8; BLOCKS as usize],
    actual: [u8; BLOCKS as usize],
}

#[derive(Debug, Default)]
struct Sweep {
    /// Crash points checked.
    checked: usize,
    /// Writes the engine refused because the pool held only uncommitted pages.
    refused: usize,
    mismatches: Vec<Mismatch>,
}

/// Crash now: the device image is whatever the file holds, and `log` is the
/// durable part of the log. Recover from that pair and compare with the
/// committed state of the operations in `prefix`.
fn crash_check(
    image_path: &Path,
    log: &[u8],
    durable_len: usize,
    prefix: &[Op],
) -> Option<Mismatch> {
    let record_path = image_path.with_file_name("recovered.bin");
    let image = std::fs::read(image_path).expect("image");
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

/// Run every sequence through the real engine with pool capacity `capacity`,
/// checking recovery at every crash point against the specification.
fn sweep(capacity: usize, protocol: Protocol, max_len: usize, dir: &Path) -> Sweep {
    let image_path = dir.join("image.bin");
    let mut out = Sweep::default();

    for ops in sequences(max_len) {
        let _ = std::fs::remove_file(&image_path);
        let device = FileBlockDevice::create(&image_path, BLOCKS).expect("create image");
        let mut engine = StorageEngine::new(device, capacity);
        let mut durable_len = 0usize;

        for k in 0..ops.len() {
            let prefix = &ops[..=k];
            match ops[k] {
                Op::Write { block, value } => {
                    if let Err(e) = engine.write_page(block as u64, &page(value)) {
                        // Only the no-steal refusal is expected. The refused
                        // write is not part of the batch, so check the prefix
                        // before it and end the sequence.
                        assert_eq!(e, StorageError::UncommittedPagesPinned, "unexpected error");
                        out.refused += 1;
                        out.checked += 1;
                        let log = &engine.wal_bytes()[..durable_len];
                        out.mismatches.extend(crash_check(
                            &image_path,
                            log,
                            durable_len,
                            &ops[..k],
                        ));
                        break;
                    }
                    if protocol == Protocol::PersistEachWrite {
                        durable_len = engine.wal_bytes().len();
                    }
                    out.checked += 1;
                    let log = &engine.wal_bytes()[..durable_len];
                    out.mismatches
                        .extend(crash_check(&image_path, log, durable_len, prefix));
                }
                Op::Commit => {
                    let before = durable_len;
                    engine
                        .commit_durable(|log| {
                            // Crash after the marker is written, before it is
                            // persisted: the batch is not durable, and the
                            // device has not been flushed yet.
                            out.checked += 1;
                            out.mismatches.extend(crash_check(
                                &image_path,
                                &log[..before],
                                before,
                                prefix,
                            ));
                            // Crash after the log is persisted, before the
                            // flush: the batch is durable, the device is old.
                            durable_len = log.len();
                            out.checked += 1;
                            out.mismatches.extend(crash_check(
                                &image_path,
                                &log[..durable_len],
                                durable_len,
                                prefix,
                            ));
                            Ok(())
                        })
                        .expect("commit_durable");
                    // Crash after the flush.
                    out.checked += 1;
                    let log = &engine.wal_bytes()[..durable_len];
                    out.mismatches
                        .extend(crash_check(&image_path, log, durable_len, prefix));
                }
            }
        }
    }
    out
}

fn summary(capacity: usize, protocol: Protocol, max_len: usize, s: &Sweep) -> String {
    serde_json::json!({
        "archon_rev": ARCHON_REV,
        "buffer_pool_capacity": capacity,
        "protocol": format!("{protocol:?}"),
        "max_sequence_length": max_len,
        "crash_points_checked": s.checked,
        "writes_refused_no_steal": s.refused,
        "mismatches": s.mismatches.len(),
    })
    .to_string()
}

#[test]
fn spec_framing_matches_archon_record_sizes() {
    // Cheap guard: one committed write must be exactly the predicted bytes.
    let ops = [Op::Write { block: 0, value: 7 }, Op::Commit];
    let image = scratch("framing").join("framing.bin");
    let device = FileBlockDevice::create(&image, BLOCKS).unwrap();
    let mut engine = StorageEngine::new(device, 4);
    engine.write_page(0, &page(7)).unwrap();
    engine.commit_durable(|_| Ok(())).unwrap();
    assert_eq!(
        engine.wal_bytes().len(),
        record_lengths(&ops).iter().sum::<usize>()
    );
}

#[test]
fn sidecar_protocol_matches_the_spec_without_eviction() {
    let dir = scratch("sidecar");
    let result = sweep(4, Protocol::Sidecar, MAX_LEN, &dir);
    assert!(
        result.checked >= sequences(MAX_LEN).len(),
        "every sequence should be checked, checked {}",
        result.checked
    );
    assert_eq!(result.refused, 0, "nothing is refused with room to spare");
    assert!(
        result.mismatches.is_empty(),
        "{} mismatches, first: {:?}",
        result.mismatches.len(),
        result.mismatches.first()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn persisting_each_write_tests_commit_gating() {
    // Uncommitted writes are durable in the log here, so recovery must drop
    // them. The commit-marker crash points cover cuts that split a commit.
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
fn no_steal_refuses_eviction_and_keeps_the_device_clean() {
    // With one frame, a second page can only come in by evicting the first,
    // which is uncommitted. The engine must refuse it, and the device must
    // stay consistent with the committed state at every crash point.
    let dir = scratch("no-steal");
    for protocol in [Protocol::Sidecar, Protocol::PersistEachWrite] {
        let result = sweep(1, protocol, MAX_LEN, &dir);
        assert!(
            result.refused > 0,
            "{protocol:?}: the eviction case must be exercised"
        );
        assert!(
            result.mismatches.is_empty(),
            "{protocol:?}: {} mismatches, first: {:?}",
            result.mismatches.len(),
            result.mismatches.first()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Claims and evidence for the release decision. The evidence records are
/// built from the sweep results, so a failing sweep blocks the release.
fn archon_graph(
    sidecar_ok: bool,
    no_steal_ok: bool,
) -> (AssuranceGraph, ClaimID, ClaimID, ClaimID) {
    let mut g = AssuranceGraph::new();
    let scope = ScopeID::new("scope.archon.crash-model").unwrap();
    g.add_scope(Scope::new(
        scope.clone(),
        "A crash leaves the device image as it was and the log as far as it was persisted",
    ))
    .unwrap();

    let mut backed_claim = |id: &str, statement: &str, ob: &str, desc: &str, ok: bool| {
        let claim_id = ClaimID::new(id).unwrap();
        let mut c = Claim::new(claim_id.clone(), statement);
        c.scopes.push(scope.clone());
        g.add_claim(c).unwrap();
        let ob_id = ObligationID::new(ob).unwrap();
        g.add_obligation(Obligation::new(ob_id.clone(), desc))
            .unwrap();
        g.require_obligation(&claim_id, &ob_id).unwrap();
        let ev_id = EvidenceID::new(format!("evidence.{ob}")).unwrap();
        let mut rec = EvidenceRecord::new(
            ev_id.clone(),
            EvidenceKind::ModelCheck,
            AssuranceLevel::ModelChecked,
            "Exhaustive bounded sweep of the real tpt-archon-core engine (not a proof)",
        );
        if ok {
            rec.validate();
        } else {
            rec.reject();
        }
        g.add_evidence(rec).unwrap();
        g.discharge(&ob_id, &ev_id).unwrap();
        claim_id
    };

    let sidecar = backed_claim(
        "claim.archon.recovery.sidecar",
        "Recovery restores exactly the committed writes when batches are committed with commit_durable",
        "obligation.archon.recovery.sidecar-sweep",
        "Bounded exhaustive sweep, sidecar protocol, no eviction",
        sidecar_ok,
    );
    let no_steal = backed_claim(
        "claim.archon.recovery.no-steal",
        "Recovery restores exactly the committed writes under buffer-pool pressure, because uncommitted pages are never written to the device",
        "obligation.archon.recovery.no-steal-sweep",
        "Bounded exhaustive sweep with one frame, which forces eviction attempts",
        no_steal_ok,
    );

    // Not backed by any evidence. Nothing in the suite checks batches larger
    // than the pool, so the claim must stay unestablished.
    let large = ClaimID::new("claim.archon.recovery.large-batch").unwrap();
    g.add_claim(Claim::new(
        large.clone(),
        "Recovery restores committed batches larger than the buffer pool",
    ))
    .unwrap();
    let ob = ObligationID::new("obligation.archon.recovery.large-batch").unwrap();
    g.add_obligation(Obligation::new(
        ob.clone(),
        "Sweep with batches larger than the pool",
    ))
    .unwrap();
    g.require_obligation(&large, &ob).unwrap();

    (g, sidecar, no_steal, large)
}

#[test]
fn release_gate_approves_the_backed_claims_and_blocks_the_unbacked_one() {
    let dir = scratch("gate");
    let sidecar = sweep(4, Protocol::Sidecar, MAX_LEN, &dir);
    let no_steal = sweep(1, Protocol::PersistEachWrite, MAX_LEN, &dir);
    let (graph, sidecar_id, no_steal_id, large_id) = archon_graph(
        sidecar.mismatches.is_empty(),
        no_steal.mismatches.is_empty(),
    );

    let mut backed = Policy::new("archon-release");
    backed.require_claim(sidecar_id.clone(), AssuranceLevel::ModelChecked);
    backed.require_claim(no_steal_id.clone(), AssuranceLevel::ModelChecked);
    assert!(
        evaluate(&graph, &backed).is_approved(),
        "both sweeps are clean, so both claims are established"
    );

    let mut unbacked = Policy::new("archon-large-batch-release");
    unbacked.require_claim(large_id.clone(), AssuranceLevel::ModelChecked);
    match evaluate(&graph, &unbacked) {
        GateDecision::Rejected { reasons } => assert!(
            reasons.iter().any(|r| matches!(
                r,
                RejectionReason::ClaimUnestablished { claim, .. } if *claim == large_id
            )),
            "expected the unbacked claim to be unestablished: {reasons:?}"
        ),
        GateDecision::Approved(_) => panic!("an unbacked claim must not pass the gate"),
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failing_sweep_blocks_the_release() {
    // A sweep with a mismatch records its evidence as rejected, and the gate
    // must refuse the claim. Simulated here by passing `false`.
    let (graph, sidecar_id, _, _) = archon_graph(false, true);
    let mut policy = Policy::new("archon-release");
    policy.require_claim(sidecar_id.clone(), AssuranceLevel::ModelChecked);
    match evaluate(&graph, &policy) {
        GateDecision::Rejected { reasons } => assert!(reasons.iter().any(|r| matches!(
            r,
            RejectionReason::ClaimUnestablished { claim, .. } if *claim == sidecar_id
        ))),
        GateDecision::Approved(_) => panic!("a rejected sweep must block the release"),
    }
}

#[test]
fn evidence_bundle_records_the_sweeps() {
    let dir = scratch("bundle");
    let sidecar = sweep(4, Protocol::Sidecar, 2, &dir);
    let no_steal = sweep(1, Protocol::PersistEachWrite, 2, &dir);
    let bundle_dir = dir.join("bundle");
    let bundle = EvidenceBundleBuilder::new("bundle.archon-storage")
        .toolchain(ToolchainIdentity::new("archon-core", ARCHON_REV))
        .artifact(
            Section::FuzzCampaigns,
            EvidenceID::new("evidence.archon.sweep.sidecar").unwrap(),
            EvidenceKind::Other("bounded_sweep".into()),
            "sidecar-sweep.json",
            "Sidecar protocol, no eviction (max length 2)",
            summary(4, Protocol::Sidecar, 2, &sidecar).into_bytes(),
        )
        .unwrap()
        .artifact(
            Section::FuzzCampaigns,
            EvidenceID::new("evidence.archon.sweep.no-steal").unwrap(),
            EvidenceKind::Other("bounded_sweep".into()),
            "no-steal-sweep.json",
            "One frame, persist after each write (max length 2)",
            summary(1, Protocol::PersistEachWrite, 2, &no_steal).into_bytes(),
        )
        .unwrap()
        .write(&bundle_dir)
        .expect("write bundle");
    assert_eq!(bundle.artifacts().count(), 2);
    tpt_concord_evidence::read_and_verify(&bundle_dir).expect("hashes verify");
    let _ = std::fs::remove_dir_all(&dir);
}
