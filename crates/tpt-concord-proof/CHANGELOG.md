# Changelog

All notable changes to `tpt-concord-proof` are recorded here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project adheres to the versioning policy in
[`docs/versioning.md`](../../docs/versioning.md) (Semantic Versioning, pre-1.0:
breaking changes bump the minor version).

## [Unreleased]

## [0.1.0] — 2026-10-05

### Added

- `ProofObligation` (id, statement, free-form context) and `ProofOutcome`
  (`Proven`, `NotProven`, `Inconclusive`), serializable with a snake_case
  `outcome` tag.
- `ProverAdapter` trait: `name`, `file_extension`, `render`,
  `check_command` and `parse_result`.
- `LeanAdapter` (renders a `theorem ... := by sorry` scaffold; re-check via
  `lake env lean`), `TelosAdapter` and `EidosAdapter`, plus a configurable
  `ExternalAdapter` with an `{artifact}` command template.
- Honest output parsing: a `sorry` is never proven; output without a verdict
  marker is `Inconclusive`. Shared `check_output` for generic adapters that
  also treats `admit` and `axiom` as proof placeholders.
- `ProofArtifact` recording the obligation, adapter, source path, SHA-256 of
  the source, re-check command and recorded outcome, with `verify_source` and
  `claims_proven`.
- `ProofRegistry` with `for_obligation`, `proven_and_intact` (tamper and
  missing-file detection for artifacts claiming `Proven`) and JSON
  round-tripping.
