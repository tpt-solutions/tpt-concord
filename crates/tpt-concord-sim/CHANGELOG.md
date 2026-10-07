# Changelog

All notable changes to `tpt-concord-sim` are recorded here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project adheres to the versioning policy in
[`docs/versioning.md`](../../docs/versioning.md) (Semantic Versioning, pre-1.0:
breaking changes bump the minor version).

## [Unreleased]

## [0.1.0] — 2026-10-05

### Added

- Virtual-time `Scheduler<P>` ordering events by `(time, insertion seq)`, with
  `schedule`, `pop_next`, `drain_until`, `len`, `is_empty` and `next_time`;
  no wall clock is used anywhere.
- Fault injection: `Fault { at_step, kind }` applied at exact steps by
  `run_with_faults`, which records a serializable `SimulationLog` of
  `StepRecord`s and the faults actually applied.
- Replay verification: `verify_replay` runs a simulation twice from identical
  inputs and faults and reports divergence.
- Breadth-first state-space exploration of any `DeterministicMachine`:
  `explore` (depth-bounded) and `explore_bounded` (with a transition budget),
  returning an `ExplorationReport` with state and transition counts, maximum
  depth and, when a bad state is found, the minimal witness input path.
