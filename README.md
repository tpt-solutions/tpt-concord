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
| [`tpt-concord-core`](crates/core) | Core IDs, assurance levels, assumptions, scopes, versions, Assurance Graph |
| [`tpt-concord-spec`](crates/spec) | Specification representation: pre/postconditions, invariants, transitions, effects, bounds |
| [`tpt-concord-model`](crates/model) | Executable reference models: deterministic state machines, transition tables |
| [`tpt-concord-trace`](crates/trace) | Canonical traces: event identity, causal ordering, normalization, replay, divergence |
| [`tpt-concord-check`](crates/check) | Conformance engine: comparison, invariant/refinement checking, counterexamples |
| [`tpt-concord-property`](crates/property) | Property-based testing derived from specifications |
| [`tpt-concord-fuzz`](crates/fuzz) | Specification-driven fuzzing, corpus management, reproducible failures |
| [`tpt-concord-diff`](crates/diff) | Differential verification: impl vs model, impl vs impl, version vs version |
| [`tpt-concord-proof`](crates/proof) | Proof-obligation abstraction; Lean/Telos/Eidos adapters; checkable artifacts |
| [`tpt-concord-sim`](crates/sim) | Deterministic simulation: virtual time, scheduling, fault injection |
| [`tpt-concord-contract`](crates/contract) | Optional runtime contracts (enforcement, not proof) |
| [`tpt-concord-evidence`](crates/evidence) | Evidence bundles: artifacts, traces, campaigns, toolchain identities |
| [`tpt-concord-release`](crates/release) | Release gate: obligations, completeness, policy enforcement, decisions |
| [`tpt-concord-cli`](crates/cli) | `spec`, `model`, `check`, `prove`, `simulate`, `fuzz`, `diff`, `evidence`, `gate` |
| [`pilot-archon-wal`](crates/pilot-archon-wal) | Milestone pilot: WAL/transaction claim + Assurance Graph end-to-end |

## Worked example

The pilot crate demonstrates the full milestone pipeline end-to-end: a WAL
specification, a deterministic reference model, a simulated implementation,
trace conformance, a counterexample lifecycle, an evidence bundle and a
release-gate decision. See [`crates/pilot-archon-wal`](crates/pilot-archon-wal).

## License

Dual-licensed under **MIT OR Apache-2.0** — see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE). Copyright TPT Solutions.
