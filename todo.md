# TPT Concord — Project TODO

Tracking checklist for the whole project, derived from `spec.txt`. Phases are
sequential; crates within a phase can generally proceed in parallel once their
dependencies (earlier phases) are in place.

License: dual **MIT OR Apache-2.0**, copyright TPT Solutions.

---

## Phase 0 — Project Setup & Licensing

- [x] Initialize git repository
- [ ] Create Cargo workspace `Cargo.toml` with members for all crates (see §5 list below)
- [x] Add `.gitignore` (Rust: `/target`, editor/OS cruft)
- [ ] Add `LICENSE-MIT`
- [ ] Add `LICENSE-APACHE`
- [ ] Add SPDX `MIT OR Apache-2.0` license headers/policy to crate manifests
- [ ] Write `README.md` (project purpose, from spec §1–2)
- [ ] Write `CONTRIBUTING.md`
- [ ] Set up CI pipeline (fmt, clippy, test, build workspace)

## Phase 1 — Initial Milestone (spec §11): Core Assurance Foundation

### `tpt-concord-core`
- [ ] `SpecificationID`, `ClaimID`, `ObligationID`, `EvidenceID`, `ModelID`, `ImplementationID`, `TraceID` types
- [ ] Assurance metadata
- [ ] Assumptions
- [ ] Scopes
- [ ] Versions
- [ ] Assurance Graph (§3.4): Claims/Obligations/Evidence as a directed graph
- [ ] Assurance Level enum (§6): specified, type_checked, model_checked, property_tested, fuzz_tested, differentially_checked, partially_proven, fully_proven

### `tpt-concord-spec`
- [ ] Preconditions
- [ ] Postconditions
- [ ] Invariants
- [ ] State transitions
- [ ] Effects
- [ ] Resource bounds
- [ ] Permitted behaviours

### `tpt-concord-model`
- [ ] Deterministic state machines
- [ ] Abstract models
- [ ] Transition functions
- [ ] Reference semantics

### `tpt-concord-trace`
- [ ] Event identity
- [ ] Causal ordering
- [ ] Normalization
- [ ] Replay
- [ ] Divergence comparison

### `tpt-concord-check`
- [ ] Implementation/model comparison
- [ ] Invariant checking
- [ ] Refinement checking
- [ ] Deviation classification
- [ ] Counterexample generation

### Counterexample object (§3.6 / §9)
- [ ] Specification reference
- [ ] Implementation reference
- [ ] Input/state
- [ ] Trace
- [ ] Environment
- [ ] Observed violation
- [ ] Re-check workflow: fixed implementation vs. exact counterexample

### `tpt-concord-evidence`
- [ ] Evidence bundle format
- [ ] Store: proof artifacts
- [ ] Store: model-check results
- [ ] Store: traces
- [ ] Store: simulations
- [ ] Store: fuzz campaigns
- [ ] Store: test results
- [ ] Store: toolchain identities
- [ ] Store: assumptions

### `tpt-concord-release`
- [ ] Required obligations
- [ ] Evidence completeness checking
- [ ] Validity checking
- [ ] Policy enforcement
- [ ] Release decisions

### Milestone integration
- [ ] Deterministic replay working end-to-end across core/model/trace
- [ ] Pilot: first real Claim + Assurance Graph proved against an existing TPT subsystem — Archon WAL/transaction behaviour (§11)

## Phase 2 — Extended Evidence-Generation Crates

### `tpt-concord-property`
- [ ] Input generation
- [ ] Stateful generation
- [ ] Invariant-aware shrinking

### `tpt-concord-fuzz`
- [ ] State-transition-aware fuzzing
- [ ] Corpus management
- [ ] Invariant checks
- [ ] Reproducible failures

### `tpt-concord-diff`
- [ ] Implementation vs model comparison
- [ ] Implementation vs implementation comparison
- [ ] Version vs version comparison

### `tpt-concord-proof`
- [ ] Proof-obligation abstraction
- [ ] Lean adapter
- [ ] Telos adapter
- [ ] Eidos adapter
- [ ] External proof system adapter interface
- [ ] Independently checkable proof artifact recording (core never trusts adapter's word)

### `tpt-concord-sim`
- [ ] Virtual time
- [ ] Controlled scheduling
- [ ] Fault injection
- [ ] State-space exploration
- [ ] Replay

### `tpt-concord-contract`
- [ ] Precondition checks
- [ ] Postcondition checks
- [ ] Invariant checks
- [ ] Debug/release instrumentation

## Phase 3 — CLI & Developer Experience

### `tpt-concord-cli`
- [ ] `spec` command
- [ ] `model` command
- [ ] `check` command
- [ ] `prove` command
- [ ] `simulate` command
- [ ] `fuzz` command
- [ ] `diff` command
- [ ] `evidence` command
- [ ] `gate` command

## Phase 4 — AI Trust / Acceptance Pipeline (§7)

- [ ] Wire pipeline stage: AI proposal → compiler/type system
- [ ] Wire pipeline stage: → specification obligations
- [ ] Wire pipeline stage: → formal proof/model checking
- [ ] Wire pipeline stage: → implementation conformance
- [ ] Wire pipeline stage: → property testing
- [ ] Wire pipeline stage: → fuzzing
- [ ] Wire pipeline stage: → evidence validation
- [ ] Wire pipeline stage: → release gate
- [ ] Document assurance-level distinctions (§6, §8) so no evidence type is ever mischaracterized as another (e.g. fuzzing ≠ proof)

## Phase 5 — TPT Ecosystem Integration (§12)

- [ ] Archon assurance infrastructure (full, beyond the Phase 1 WAL pilot)
- [ ] Boxcar assurance infrastructure
- [ ] Keystone assurance infrastructure
- [ ] Fabric assurance infrastructure
- [ ] Repro assurance infrastructure
- [ ] WASM assurance infrastructure
- [ ] Cryptography assurance infrastructure
- [ ] Networking assurance infrastructure
- [ ] Scientific/engineering systems assurance infrastructure

## Phase 6 — Documentation & Release Readiness

- [ ] Architecture docs demonstrating Concord can answer (§13): What do we claim? Why? What assumptions? What evidence? Proven vs model-checked vs tested vs unknown?
- [ ] Worked examples (end-to-end: spec → claim → evidence → release gate)
- [ ] Versioning/stability policy
- [ ] Publish crates
