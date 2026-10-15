# TPT Concord — Project TODO

Tracking checklist for the whole project, derived from `spec.txt`. Phases are
sequential; crates within a phase can generally proceed in parallel once their
dependencies (earlier phases) are in place.

License: dual **MIT OR Apache-2.0**, copyright TPT Solutions.

---

## Phase 0 — Project Setup & Licensing

- [x] Initialize git repository
- [x] Create Cargo workspace `Cargo.toml` with members for all crates (see §5 list below) — 15 members (14 spec crates + pilot)
- [x] Add `.gitignore` (Rust: `/target`, editor/OS cruft)
- [x] Add `LICENSE-MIT`
- [x] Add `LICENSE-APACHE`
- [x] Add SPDX `MIT OR Apache-2.0` license headers/policy to crate manifests
- [x] Write `README.md` (project purpose, from spec §1–2)
- [x] Write `CONTRIBUTING.md`
- [x] Set up CI pipeline (fmt, clippy, test, build workspace)

## Phase 1 — Initial Milestone (spec §11): Core Assurance Foundation

### `tpt-concord-core`
- [x] `SpecificationID`, `ClaimID`, `ObligationID`, `EvidenceID`, `ModelID`, `ImplementationID`, `TraceID` types
- [x] Assurance metadata
- [x] Assumptions
- [x] Scopes
- [x] Versions
- [x] Assurance Graph (§3.4): Claims/Obligations/Evidence as a directed graph
- [x] Assurance Level enum (§6): specified, type_checked, model_checked, property_tested, fuzz_tested, differentially_checked, partially_proven, fully_proven

### `tpt-concord-spec`
- [x] Preconditions
- [x] Postconditions
- [x] Invariants
- [x] State transitions
- [x] Effects
- [x] Resource bounds
- [x] Permitted behaviours

### `tpt-concord-model`
- [x] Deterministic state machines
- [x] Abstract models
- [x] Transition functions
- [x] Reference semantics

### `tpt-concord-trace`
- [x] Event identity
- [x] Causal ordering
- [x] Normalization
- [x] Replay
- [x] Divergence comparison

### `tpt-concord-check`
- [x] Implementation/model comparison
- [x] Invariant checking
- [x] Refinement checking
- [x] Deviation classification
- [x] Counterexample generation

### Counterexample object (§3.6 / §9)
- [x] Specification reference
- [x] Implementation reference
- [x] Input/state
- [x] Trace
- [x] Environment
- [x] Observed violation
- [x] Re-check workflow: fixed implementation vs. exact counterexample

### `tpt-concord-evidence`
- [x] Evidence bundle format
- [x] Store: proof artifacts
- [x] Store: model-check results
- [x] Store: traces
- [x] Store: simulations
- [x] Store: fuzz campaigns
- [x] Store: test results
- [x] Store: toolchain identities
- [x] Store: assumptions

### `tpt-concord-release`
- [x] Required obligations
- [x] Evidence completeness checking
- [x] Validity checking
- [x] Policy enforcement
- [x] Release decisions

### Milestone integration
- [x] Deterministic replay working end-to-end across core/model/trace
- [x] Pilot: first real Claim + Assurance Graph proved against an existing TPT subsystem — Archon WAL/transaction behaviour (§11)

## Phase 2 — Extended Evidence-Generation Crates

### `tpt-concord-property`
- [x] Input generation
- [x] Stateful generation
- [x] Invariant-aware shrinking

### `tpt-concord-fuzz`
- [x] State-transition-aware fuzzing
- [x] Corpus management
- [x] Invariant checks
- [x] Reproducible failures

### `tpt-concord-diff`
- [x] Implementation vs model comparison
- [x] Implementation vs implementation comparison
- [x] Version vs version comparison

### `tpt-concord-proof`
- [x] Proof-obligation abstraction
- [x] Lean adapter
- [x] Telos adapter
- [x] Eidos adapter
- [x] External proof system adapter interface
- [x] Independently checkable proof artifact recording (core never trusts adapter's word)

### `tpt-concord-sim`
- [x] Virtual time
- [x] Controlled scheduling
- [x] Fault injection
- [x] State-space exploration
- [x] Replay

### `tpt-concord-contract`
- [x] Precondition checks
- [x] Postcondition checks
- [x] Invariant checks
- [x] Debug/release instrumentation

## Phase 3 — CLI & Developer Experience

### `tpt-concord-cli`
- [x] `spec` command
- [x] `model` command
- [x] `check` command
- [x] `prove` command
- [x] `simulate` command
- [x] `fuzz` command
- [x] `diff` command
- [x] `evidence` command
- [x] `gate` command

## Phase 4 — AI Trust / Acceptance Pipeline (§7)

- [x] Wire pipeline stage: AI proposal → compiler/type system
- [x] Wire pipeline stage: → specification obligations
- [x] Wire pipeline stage: → formal proof/model checking
- [x] Wire pipeline stage: → implementation conformance
- [x] Wire pipeline stage: → property testing
- [x] Wire pipeline stage: → fuzzing
- [x] Wire pipeline stage: → evidence validation
- [x] Wire pipeline stage: → release gate
- [x] Document assurance-level distinctions (§6, §8) so no evidence type is ever mischaracterized as another (e.g. fuzzing ≠ proof)

## Phase 5 — TPT Ecosystem Integration (§12)

- [x] Archon assurance infrastructure (full, beyond the Phase 1 WAL pilot)
- [x] Boxcar assurance infrastructure
- [x] Keystone assurance infrastructure
- [x] Fabric assurance infrastructure
- [x] Repro assurance infrastructure
- [x] WASM assurance infrastructure
- [x] Cryptography assurance infrastructure
- [x] Networking assurance infrastructure
- [x] Scientific/engineering systems assurance infrastructure

## Phase 6 — Documentation & Release Readiness

- [x] Architecture docs demonstrating Concord can answer (§13): What do we claim? Why? What assumptions? What evidence? Proven vs model-checked vs tested vs unknown?
- [x] Worked examples (end-to-end: spec → claim → evidence → release gate)
- [x] Versioning/stability policy
- [ ] Publish crates

## Phase 7 — Adoption & Tooling

- [x] `concord init` starter project (valid by construction, hash-verified evidence bundle)
- [x] `concord report` — plain-markdown §13 answers
- [x] `concord gate --bundle` — re-verify evidence artifacts; never trust a `validated` flag alone
- [x] `concord fuzz --impl` — fuzz a real implementation, not the model against itself
- [x] Shell completions
- [x] Composite GitHub Action + job-summary report
- [x] Release-binaries workflow (Linux/macOS/Windows)
- [x] `docs/getting-started.md`, README for every crate, registry metadata
- [x] `cargo publish --workspace --dry-run` passes
- [ ] Publish crates (`cargo publish --workspace`, then tag `v0.1.0`)
- [x] JSON Schema export for every CLI object (`concord schema`, `schema` feature per crate)
- [x] `concord watch` — re-run check/gate on file change
- [x] Derive macro to generate a spec/model skeleton from annotated Rust types (`SpecState` for state; model tables still hand-written)
- [ ] Runtime conformance harnesses in each upstream TPT repo (§12 follow-through)
