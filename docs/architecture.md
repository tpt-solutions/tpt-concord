# Architecture: what a Concord release can answer (spec §13)

Trust in Concord is **compositional and inspectable**. Every artifact is a
machine-readable object; every judgment is backed by content-addressed,
re-verifiable evidence. This document walks through the eight questions of
spec §13 and shows where each answer lives, using the Archon WAL pilot
(`crates/tpt-concord-pilot-archon-wal`) as the running example.

The end-to-end flow (spec §4):

```
Specification → Obligations → Reference Model → Implementation → Conformance
             → Evidence → Claim → Release Gate
```

## 1. What do we claim?

`Claim` objects in the `AssuranceGraph` (`tpt-concord-core`). Example from
the pilot:

> *"Archon WAL recovery preserves committed transactions: after recovery the
> committed prefix contains exactly the records committed before the crash."*

Claims carry versions, so the answer is always scoped to a specific artifact.

## 2. Why do we claim it?

The graph's edges: the claim requires **obligations**; obligations are
discharged by **evidence**; claims may depend on other claims. The pilot's
claim is accepted because three obligations are discharged:

1. an exhaustive small-space model check of recovery preservation,
2. trace conformance of the implementation against the reference model,
3. deterministic replay of the conformance trace.

`graph.evaluate(claim)` returns this justification as data
(`ClaimEvaluation`), not as prose.

## 3. What assumptions does it depend on?

Explicit `Assumption` objects attached to claims — e.g. *"the durable disk
prefix always contains all records committed before the crash"* — qualified
by `Scope` objects (*"prefix-crash model: a crash truncates the log at an
arbitrary point but never corrupts surviving records"*). A release gate can
forbid open assumptions (`Policy::allow_open_assumptions = false`), forcing
them to become proof obligations or explicit acceptance decisions.

## 4. What evidence supports it?

`EvidenceRecord`s: typed (`EvidenceKind`), leveled (`AssuranceLevel`),
status-tracked (`Unvalidated`/`Validated`/`Rejected`), and pointing at
artifacts with SHA-256 hashes in an evidence bundle
(`tpt-concord-evidence`). The bundle's `manifest.json` plus its artifact
files are the portable, verifiable form.

## 5. Which parts are proven?

Obligations discharged by `lean_proof` / `telos_proof` / `eidos_proof`
evidence whose artifacts re-verify (`tpt-concord-proof`: source hash + the
recorded re-check command). A `sorry`-bearing artifact parses as
`Inconclusive` and can never claim `Proven`. Claims fully covered by proof
evidence reach `fully_proven`.

## 6. Which are model-checked?

Obligations discharged by `model_check` evidence — e.g. the pilot's
exhaustive sweep of every legal input sequence over the crash alphabet —
plus conformance transferring model-level guarantees to the implementation.
Level: `model_checked`.

## 7. Which are tested?

Obligations discharged by `property_test_campaign`, `fuzz_campaign`,
`differential_comparison`, `test_run`, or `simulation` evidence. These are
empirical levels (`property_tested`, `fuzz_tested`, `differentially_checked`)
and are never promoted into proof levels — see `docs/pipeline.md`.

## 8. Which remain unknown?

Everything the graph does not establish: obligations with no evidence
(`UnmetReason::NoEvidence`), evidence that failed re-checking
(`EvidenceRejected`), dependencies that are themselves unestablished, and
stages of the acceptance pipeline that were never reached
(`AcceptanceTrace::rejected_at`). A gate rejection enumerates these
(`RejectionReason`), so "unknown" is always a list, never a shrug.

## Determinism as the connective tissue

Every layer is deterministic by construction: seeded generation
(`tpt-concord-property`), total transition functions (`tpt-concord-model`),
canonical content-addressed traces (`tpt-concord-trace`), replayable
simulations (`tpt-concord-sim`). That is what makes evidence *re-checkable*
rather than anecdotal: the same input, the same trace, the same digest —
today and at any future audit.

## Worked example

`crates/tpt-concord-pilot-archon-wal/tests/milestone.rs` runs the entire chain twice:
once with a conformant WAL implementation (gate **approves**) and once with a
deliberately broken recovery that marks uncommitted records as committed
(gate **rejects**, a first-class `Counterexample` is produced, and the
re-check workflow resolves it against the fixed implementation).
