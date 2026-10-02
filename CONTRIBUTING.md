# Contributing to TPT Concord

Thank you for contributing to Concord. This document covers the mechanics.
The *architecture* lives in [`spec.txt`](spec.txt); the roadmap in
[`todo.md`](todo.md).

## Ground rules

1. **Evidence types are never conflated.** Concord exists to preserve the
   distinction between proof, model checking, property testing, fuzzing,
   differential testing and simulation. Never describe a fuzz campaign as a
   proof, or a passing test suite as formal verification — in code, docs,
   claim statements or evidence bundles. Assurance levels
   (`tpt_concord_core::AssuranceLevel`) exist precisely to keep these apart.
2. **Nothing is trusted because someone (or something) said so.** Evidence is
   recorded as independently checkable artifacts with content hashes. The core
   never takes an adapter's word for a proof result; it records the artifact so
   the result can be re-verified.
3. **Determinism is a feature.** Traces, models, simulations and counterexamples
   must replay deterministically. Canonical serialization (sorted keys, stable
   encodings) is mandatory for anything content-addressed.

## Development workflow

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs exactly these plus `cargo build --workspace`. A PR that fails any of
them is not ready for review.

- **Formatting:** `rustfmt` defaults; no custom config.
- **Linting:** clippy warnings are errors. Justify any `#[allow(...)]` with a
  comment explaining why the warning is wrong.
- **Tests:** every public API that makes a decision (conformance verdict,
  counterexample, gate decision) needs a unit test exercising both accept and
  reject paths.

## Licensing

Dual **MIT OR Apache-2.0**, copyright TPT Solutions. By contributing you agree
your work is released under both licenses. Every source file carries:

```rust
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions
```

Dependencies must be compatible with dual MIT/Apache-2.0 distribution.

## Commit style

Short imperative subject lines (`Add refinement checker to tpt-concord-check`),
blank line, then a body explaining *why* when it is not obvious. One logical
change per commit.

## What needs design discussion first

- New crates (the crate layout in `spec.txt` §5 is deliberate).
- Changes to the Assurance Graph model or the evidence bundle format — these
  are the machine-readable contracts everything else builds on.
- Anything that would make an evidence type indistinguishable from another.

Open an issue before large refactors.
