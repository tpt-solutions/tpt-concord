# tpt-concord-release

The Concord release gate: required claims, required obligations, policy enforcement and machine-readable decisions.

## What it is

A release gate answers one question honestly: *may this be released, and if not, exactly what is missing?*
`tpt-concord-release` takes an `AssuranceGraph` (from [`tpt-concord-core`](../tpt-concord-core)) and a
`Policy`, and returns a structured `GateDecision`: either an approval with a summary of what was accepted,
or a rejection carrying a list of `RejectionReason`s, each naming the offending claim, obligation or assumption.

It also provides `AcceptanceTrace`, the spec section 7 pipeline object that records how an AI-proposed
artifact walked the ordered acceptance stages, and where it was rejected.

## Where it sits in the pipeline

The release gate is the last stage of the Concord pipeline:

```
property -> specification -> implementation -> proof/test/model evidence -> release decision
```

Upstream, [`tpt-concord-core`](../tpt-concord-core) defines the graph; [`tpt-concord-check`](../tpt-concord-check),
[`tpt-concord-proof`](../tpt-concord-proof), [`tpt-concord-fuzz`](../tpt-concord-fuzz) and friends produce evidence;
and [`tpt-concord-evidence`](../tpt-concord-evidence) bundles and re-verifies it. This crate only consumes the graph.
The `concord gate` command in [`tpt-concord-cli`](../tpt-concord-cli) is a thin wrapper over `evaluate`.

## Features

- `Policy`: required claims with minimum assurance levels, required obligations, and an assumption policy.
- `evaluate(&AssuranceGraph, &Policy) -> GateDecision`: validates the graph, then checks every requirement and
  collects all failures rather than stopping at the first.
- `RejectionReason`: `GraphInvalid`, `UnknownClaim`, `UnknownObligation`, `ClaimUnestablished`,
  `LevelInsufficient`, `OpenAssumptions`, `UndischargedObligation`. All serialize to JSON with a `reason` tag.
- `GateDecision` / `GateSummary`: serializable decision (`decision` tag: `approved` or `rejected`) listing
  established claims, achieved levels and the count of supporting validated evidence.
- `AcceptanceTrace`, `Stage`, `StageOutcome`, `StageReport`: the nine-stage AI acceptance pipeline as a
  serializable record.

## Installation

```sh
cargo add tpt-concord-release
```

or in `Cargo.toml`:

```toml
[dependencies]
tpt-concord-release = "0.1"
tpt-concord-core = "0.1"
```

Minimum supported Rust version: 1.75.

## Quick start

```rust
use tpt_concord_core::{
    AssuranceGraph, AssuranceLevel, Claim, ClaimID, EvidenceID, EvidenceKind, EvidenceRecord,
    Obligation, ObligationID,
};
use tpt_concord_release::{evaluate, GateDecision, Policy};

let claim = ClaimID::new("wal.claim").unwrap();
let obligation = ObligationID::new("wal.modelcheck").unwrap();
let evidence = EvidenceID::new("ev.modelcheck").unwrap();

// A claim, an obligation it requires, and validated evidence discharging it.
let mut graph = AssuranceGraph::new();
graph.add_claim(Claim::new(claim.clone(), "recovery preserves committed")).unwrap();
graph.add_obligation(Obligation::new(obligation.clone(), "model check")).unwrap();
graph.require_obligation(&claim, &obligation).unwrap();

let mut record = EvidenceRecord::new(
    evidence.clone(),
    EvidenceKind::ModelCheck,
    AssuranceLevel::ModelChecked,
    "exhaustive",
);
record.validate();
graph.add_evidence(record).unwrap();
graph.discharge(&obligation, &evidence).unwrap();

// State the policy and ask for a decision.
let mut policy = Policy::new("wal-release");
policy.require_claim(claim.clone(), AssuranceLevel::ModelChecked);

match evaluate(&graph, &policy) {
    GateDecision::Approved(summary) => println!("approved: {:?}", summary.established_claims),
    GateDecision::Rejected { reasons } => println!("rejected: {reasons:#?}"),
}

// Asking for more than the evidence supports is rejected with LevelInsufficient.
policy.require_claim(claim, AssuranceLevel::FullyProven);
assert!(!evaluate(&graph, &policy).is_approved());
```

## Tour

### `gate`

- `Policy { name, required_claims, required_obligations, allow_open_assumptions }`, built with `Policy::new`,
  `require_claim` and `require_obligation`. `allow_open_assumptions` defaults to `true`; set it to `false` to
  reject any required claim that rests on open assumptions.
- `ClaimRequirement::at_least(level)`: the minimum `AssuranceLevel` for a required claim.
- `evaluate`: the decision function. It:
  1. rejects with `GraphInvalid` if `graph.validate()` fails (dangling references, dependency cycles);
  2. for each required claim, rejects as `UnknownClaim`, `ClaimUnestablished`, `LevelInsufficient` or
     `OpenAssumptions` as appropriate;
  3. for each required obligation, rejects as `UnknownObligation` or `UndischargedObligation` unless at least
     one linked evidence record has status `Validated`.
- `GateDecision::is_approved()`.

### `pipeline`

`Stage::ORDERED` lists the nine stages: `AiProposal`, `CompilerTypeSystem`, `SpecificationObligations`,
`FormalProofOrModelChecking`, `ImplementationConformance`, `PropertyTesting`, `Fuzzing`, `EvidenceValidation`,
`ReleaseGate`. `AcceptanceTrace::begin()` records the proposal as received (trusted for nothing); `pass` appends
a passing stage in order (checked with a `debug_assert!`); `fail` records a failure and stops; `accepted()` is
true only when all nine stages passed; `rejected_at()` returns the first failed stage and its reason.

## Design notes and guarantees

- **Decisions are data.** Every rejection names the object at fault and serializes to JSON, so CI can act on it.
  The `GateDecision` and `Policy` JSON shapes are treated as stable contracts (see
  [`docs/versioning.md`](../../docs/versioning.md)).
- **Evidence kind is not erased.** Minimum levels use the assurance-level ordering, which is a policy hint
  (`strictness_rank`), not an equivalence. Where the kind of evidence matters, require the obligation (obligation
  to evidence links preserve kind exactly), not just the level. A fuzz campaign is never a proof.
- **Only validated evidence discharges a required obligation.** Unvalidated records do not count.
- **Failure collection.** Requirements are evaluated independently so one run reports everything missing. Within a
  single claim, the first failing check wins (unestablished, then level, then open assumptions).
- **Honest traces.** A failed `AcceptanceTrace` ends at the failure; later stages are never claimed.
- **Limits.** The gate does not re-run proofs, tests or hash checks; it trusts the statuses recorded in the graph.
  Re-verification belongs to `tpt-concord-evidence` and the adapters that mark evidence validated. Stage ordering in
  `AcceptanceTrace::pass` is a debug assertion, not a runtime error. The gate is a decision procedure over recorded
  evidence, not a guarantee that the software is correct.

## Relation to other crates

Depends on [`tpt-concord-core`](../tpt-concord-core) only (plus `serde` and `thiserror`). Used by
[`tpt-concord-cli`](../tpt-concord-cli). See [`docs/pipeline.md`](../../docs/pipeline.md) for the
acceptance pipeline.

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
