# TPT Concord

Pure-Rust framework for representing **specifications, claims, evidence,
conformance and assurance** as first-class, machine-readable objects.

**Status:** design implemented per [`spec.txt`](spec.txt); progress tracked in
[`todo.md`](todo.md). Early development — APIs may change.

## Purpose

Verification evidence is fragmented. A project may have formal proofs,
executable models, tests, fuzz campaigns, model checking, differential
testing, benchmarks and protocol conformance tests — but often no
machine-readable relationship between:

```
property → specification → implementation → proof/test/model evidence → release decision
```

Concord's central question:

> Given an implementation, exactly what do we know about it, why do we know it,
> under which assumptions, and what remains unproven?

Concord is **not** a theorem prover, fuzzer, model checker or test framework.
It is the evidence and conformance layer that connects Lean, Telos, Eidos,
model checkers, property testing and fuzzing — preserving the distinction
between evidence types (a fuzz campaign is never misreported as a proof).

For AI-assisted development, Concord treats AI as an *untrusted proposer*:
AI-generated implementations, tests and proof scripts are independently
accountable to specifications and evidence, never trusted because an AI
produced them.

## Crates

| Crate | Role |
|---|---|
| [`tpt-concord-core`](crates/tpt-concord-core) | Core IDs, assurance levels, assumptions, scopes, versions, Assurance Graph |
| [`tpt-concord-spec`](crates/tpt-concord-spec) | Specification representation: pre/postconditions, invariants, transitions, effects, bounds |
| [`tpt-concord-model`](crates/tpt-concord-model) | Executable reference models: deterministic state machines, transition tables |
| [`tpt-concord-trace`](crates/tpt-concord-trace) | Canonical traces: event identity, causal ordering, normalization, replay, divergence |
| [`tpt-concord-check`](crates/tpt-concord-check) | Conformance engine: comparison, invariant/refinement checking, counterexamples |
| [`tpt-concord-property`](crates/tpt-concord-property) | Property-based testing derived from specifications |
| [`tpt-concord-fuzz`](crates/tpt-concord-fuzz) | Specification-driven fuzzing, corpus management, reproducible failures |
| [`tpt-concord-diff`](crates/tpt-concord-diff) | Differential verification: impl vs model, impl vs impl, version vs version |
| [`tpt-concord-proof`](crates/tpt-concord-proof) | Proof-obligation abstraction; Lean/Telos/Eidos adapters; checkable artifacts |
| [`tpt-concord-sim`](crates/tpt-concord-sim) | Deterministic simulation: virtual time, scheduling, fault injection |
| [`tpt-concord-contract`](crates/tpt-concord-contract) | Optional runtime contracts (enforcement, not proof) |
| [`tpt-concord-evidence`](crates/tpt-concord-evidence) | Evidence bundles: artifacts, traces, campaigns, toolchain identities |
| [`tpt-concord-release`](crates/tpt-concord-release) | Release gate: obligations, completeness, policy enforcement, decisions |
| [`tpt-concord-cli`](crates/tpt-concord-cli) | `spec`, `model`, `check`, `prove`, `simulate`, `fuzz`, `diff`, `evidence`, `gate` |
| [`tpt-concord-derive`](crates/tpt-concord-derive) | `#[derive(SpecState)]`: map a Rust struct onto spec state and generate a specification skeleton (re-exported by `tpt-concord-spec` with its `derive` feature) |
| [`tpt-concord-pilot-archon-wal`](crates/tpt-concord-pilot-archon-wal) | Milestone pilot: WAL/transaction claim + Assurance Graph end-to-end |
| [`tpt-concord-assurance`](crates/tpt-concord-assurance) | §12 assurance infrastructure for TPT subsystems: Archon, Boxcar, Keystone, Fabric, Repro, WASM, crypto, networking, scientific/engineering |

## Worked example

The pilot crate demonstrates the full milestone pipeline end-to-end: a WAL
specification, a deterministic reference model, a simulated implementation,
trace conformance, a counterexample lifecycle, an evidence bundle and a
release-gate decision. See [`crates/tpt-concord-pilot-archon-wal`](crates/tpt-concord-pilot-archon-wal).

## Quick start

```
cargo install tpt-concord-cli --locked
concord init demo && cd demo && cat README.txt
```

`concord init` writes a runnable example project; see
[`docs/getting-started.md`](docs/getting-started.md) for the workflow and a
ready-made GitHub Action that gates releases and posts the assurance report to
the job summary.

## Command line

```
concord init [dir]                            # scaffold a runnable starter project
concord report <graph.json> --policy p.json   # plain markdown: claimed / why / unknown
concord watch <paths>... -- <command>        # re-run a command when its inputs change
concord schema <kind>                        # JSON Schema for spec|model|trace|graph|policy|manifest
concord completions <shell>                   # bash, zsh, fish, powershell, elvish
concord spec <spec.json>                      # validate a specification
concord model <model.json> --inputs inputs.json
concord check <spec.json> <trace.json>        # conformance verdict + exit code
concord diff <a.json> <b.json>                # differential comparison
concord simulate <model.json> --alphabet ...  # state-space exploration
concord fuzz <model.json> --alphabet ... [--impl impl.json]  # stateful fuzz; --impl tests an implementation
concord prove --adapter lean4 --statement ... # render a proof obligation
concord evidence verify <bundle-dir>          # re-verify artifact hashes
concord gate <graph.json> <policy.json> [--bundle dir]  # release decision; --bundle re-hashes evidence
```

## Phase 5: ecosystem assurance

`crates/tpt-concord-assurance` encodes each TPT subsystem's guarantees as Concord claims
grounded in the upstream repos (`tpt-solutions/*` on GitHub) — with scopes
and assumptions that mirror upstream's *own* honesty about maturity: Boxcar's
gate stays rejected while its OCI/Wasm integration is bookkeeping-only;
Keystone's stays open on unverified LSM recovery; Repro's approves.
Runtime conformance harnesses (running the real engines through these
models) land in each subsystem's repo as the next step.

## Documentation

- [`docs/getting-started.md`](docs/getting-started.md) — install, scaffold, CI gate
- [`docs/architecture.md`](docs/architecture.md) — how a release answers the
  §13 questions (what we claim / why / assumptions / evidence / what's unproven)
- [`docs/pipeline.md`](docs/pipeline.md) — the §7 AI acceptance pipeline and
  the §6/§8 assurance-level distinctions
- [`docs/versioning.md`](docs/versioning.md) — versioning & stability policy
- [`CHANGELOG.md`](CHANGELOG.md)

## License

Dual-licensed under **MIT OR Apache-2.0** — see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE). Copyright TPT Solutions.
