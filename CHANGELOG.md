# Changelog

All notable changes to TPT Concord are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versioning policy is in
[`docs/versioning.md`](docs/versioning.md).

## [0.1.0] — 2026-10-05

### Added

- **Workspace** with 15 crates: core, spec, model, trace, check, property,
  fuzz, diff, proof, sim, contract, evidence, release, cli, and the
  `pilot-archon-wal` milestone pilot. Dual MIT OR Apache-2.0 licensing,
  README, CONTRIBUTING guide, CI (fmt/clippy/test/build on Linux + Windows).
- **`tpt-concord-core`**: validated ID types, `Version`, `Assumption`,
  `Scope`, the eight `AssuranceLevel`s with category/rank policy semantics,
  `EvidenceKind`, and the append-only `AssuranceGraph` with structural
  validation (dangling refs, dependency cycles) and claim evaluation
  (establishment, achieved level, open assumptions).
- **`tpt-concord-spec`**: serializable predicate AST (`Expr`/`Value` over
  states), `Specification` with preconditions, postconditions, invariants,
  transitions, effects, resource bounds and permitted behaviours, plus
  structured `Violation` reporting.
- **`tpt-concord-model`**: `DeterministicMachine` trait, run recorder,
  table-driven `TransitionTable` and `AbstractTable` (nondeterministic
  permission sets) with entry-list JSON, and a blanket `AbstractModel` impl
  for deterministic machines.
- **`tpt-concord-trace`**: content-addressed events (SHA-256 over canonical
  encodings), causal ordering, integrity verification, deterministic replay,
  positional divergence comparison.
- **`tpt-concord-check`**: step-level conformance against a full
  `Specification` (schema, global/transition pre-post, effects, permissions,
  bounds, continuity), refinement checking with optional stuttering,
  deviation classification, and first-class `Counterexample`s with the
  re-check lifecycle (Open/Reproduced/Resolved/NotReproducible).
- **`tpt-concord-evidence`**: bundle format (`manifest.json` +
  content-addressed artifacts) with typed sections, toolchain identities,
  assumptions, write/read/verify with hash checking.
- **`tpt-concord-release`**: `Policy` (required claims at minimum assurance
  levels, required obligations, assumption policy), gate `evaluate` with
  machine-readable `RejectionReason`s, and the §7 `AcceptanceTrace` pipeline
  object.
- **`tpt-concord-property`**: seeded xorshift RNG, strategies (bounded ints,
  alphabet picks), model-walking stateful generation, delta-debugging
  shrinking.
- **`tpt-concord-fuzz`**: stateful campaign runner with shrinking, seed-stamped
  reproducible failures, content-addressed corpus management.
- **`tpt-concord-diff`**: provenance-carrying differential reports
  (impl/model, impl/impl, version/version).
- **`tpt-concord-proof`**: `ProofObligation`, Lean4/Telos/Eidos adapters plus
  configurable external adapters, honest output parsing (`sorry` is never
  proven), and `ProofArtifact`/`ProofRegistry` recording source hashes and
  re-check commands.
- **`tpt-concord-sim`**: virtual-time `Scheduler`, fault injection at exact
  steps with replay verification, and BFS state-space exploration with
  minimal witness paths.
- **`tpt-concord-contract`**: runtime contracts (pre/post/invariant) with
  Always/DebugOnly enforcement and `require!`/`ensure!` macros.
- **`tpt-concord-cli`**: `concord` binary — `spec`, `model`, `check`,
  `prove`, `simulate`, `fuzz`, `diff`, `evidence verify/show`, `gate` over
  JSON objects, with documented exit codes.
- **Pilot**: Archon WAL specification, deterministic reference model,
  conformant and deliberately-broken implementations, exhaustive
  recovery-preservation model check, evidence bundle, and release-gate
  approve/reject with the counterexample lifecycle — end to end in tests.
- **`tpt-concord-assurance`**: §12 assurance infrastructure for the TPT
  ecosystem, grounded in the `tpt-solutions` GitHub repos — Archon (buffer
  pool model, B-Link structural spec, Telos evidence recorded as unvalidated
  references), Boxcar (Frontier plugin lifecycle, Origin honest execution
  scope), Keystone (wire-v3 session machine, MVCC visibility spec),
  Fabric (pipeline authority model), Repro (cache soundness, honest
  determinism reporting), WASM (validation machine, runtime-sandbox
  assumption), crypto (nonce lifecycle, SCRAM sequencing — structure only),
  networking (session-typed channel conformance), and science (symplectic
  integrator determinism/energy bounds).
- **Docs**: `docs/architecture.md` (§13 questions), `docs/pipeline.md`
  (§7 acceptance pipeline + §6/§8 evidence-kind distinctions),
  `docs/versioning.md`.
