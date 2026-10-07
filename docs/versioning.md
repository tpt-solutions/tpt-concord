# Versioning and Stability Policy

Copyright 2026 TPT Solutions. Dual-licensed MIT OR Apache-2.0.

## Status

TPT Concord is **pre-1.0**: all crates are versioned `0.x` and the API may
change without notice. This document records the rules that *do* hold.

## Crate versioning

- All workspace crates share the workspace version (`[workspace.package]`).
  Concord is one system; crates move together.
- Breaking changes within `0.x` bump the minor version (`0.2.0`), and are
  listed in `CHANGELOG.md` under a **Breaking** heading.
- Additions bump the patch version (`0.1.1`).

## Machine-readable formats

The following are Concord's contracts with the outside world, and changes to
them are treated as breaking even between `0.x` releases:

- the `AssuranceGraph` serialization (claims, obligations, evidence, edges),
- the evidence bundle layout (`manifest.json` + `artifacts/`),
- the canonical trace format (content-addressed events),
- `Counterexample`, `GateDecision`, `Policy`, `AcceptanceTrace` JSON,
- the `concord` CLI output shapes and exit codes.

Content addressing (SHA-256 over canonical encodings) means a format change
changes digests — recorded evidence bundles remain verifiable under the
format version that produced them, so format evolution must add, not mutate.

## Assurance semantics

The `AssuranceLevel` set (spec §6) is closed. Adding a level is a breaking,
design-reviewed change. The categorical distinctions (proof ≠ model-checked
≠ empirical) are load-bearing and non-negotiable; no future version may
reorder, rename, or conflate them in a way that lets one evidence kind pass
for another.

## MSRV

The minimum supported Rust version is recorded in
`[workspace.package] rust-version`. MSRV bumps are breaking changes.

## Reaching 1.0

A crate reaches 1.0 when: its public API has survived at least one minor
deprecation cycle without reshaping; its formats have a written schema
(doc); and the milestone items of `todo.md` covering it are complete and
tested in CI.
