# Changelog

All notable changes to `tpt-concord-release` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](../../docs/versioning.md).

## [Unreleased]

## [0.1.0] — 2026-10-05

### Added

- `Policy` with required claims (each with a minimum `AssuranceLevel` via `ClaimRequirement`), required
  obligations, and an `allow_open_assumptions` switch; builders `Policy::new`, `require_claim`, `require_obligation`.
- `evaluate`, the gate function: validates the Assurance Graph, then checks required claims and obligations,
  collecting every failure.
- Machine-readable `RejectionReason`s: `GraphInvalid`, `UnknownClaim`, `UnknownObligation`, `ClaimUnestablished`,
  `LevelInsufficient`, `OpenAssumptions`, `UndischargedObligation`.
- Serializable `GateDecision` (`Approved(GateSummary)` / `Rejected`) with `is_approved`; `GateSummary` records the
  policy name, established claims, achieved levels and supporting evidence count.
- Only evidence with `Validated` status discharges a required obligation.
- The spec section 7 `AcceptanceTrace` pipeline object with `Stage` (nine ordered stages), `StageOutcome` and
  `StageReport`: `begin`, `pass`, `fail`, `accepted`, `rejected_at`.
