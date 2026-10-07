# Changelog

All notable changes to `tpt-concord-check` are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); this crate follows
[Semantic Versioning](https://semver.org/) as described in the workspace
[versioning policy](../../docs/versioning.md).

## [Unreleased]

## [0.1.0] — 2026-10-05

### Added

- Step-level conformance against a full `Specification`: `StepView`, `check_steps`,
  `check_states`, `check_trace` and `ConformanceReport` / `StepViolation`. Checks schema,
  global and transition pre/postconditions, invariants, declared effects, permitted
  behaviours, per-operation resource bounds and state continuity between steps.
- `StepView::to_event_payload` / `from_event_payload` and `step_to_event` for recording
  steps as canonical `tpt-concord-trace` events (kind `"step"`).
- Refinement checking: `refinement_check` over any `AbstractModel`, with optional
  stuttering, `RefinementReport` / `RefinementFailure`, and conversion of the first
  failure to a spec `Violation`.
- Deviation classification: `DeviationClass`, `classify_divergence` and
  `classify_violation`.
- First-class `Counterexample` objects (spec, implementation and optional model refs,
  replayable input, trace, environment, violation) with JSON round-tripping, a SHA-256
  `digest`, and the `recheck` lifecycle (`CounterexampleStatus`: Open, Reproduced,
  Resolved, NotReproducible; `RecheckOutcome`).
- `CheckError` (`ArityMismatch`, `MalformedEventPayload`).
