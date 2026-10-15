# tpt-concord-model

Executable reference models: deterministic state machines and transition tables.

Part of [TPT Concord](https://github.com/tpt-solutions/tpt-concord) — the evidence and conformance layer that
connects specifications, claims, proofs, tests and release decisions.

## Overview

Executable reference models (spec §5, `tpt-concord-model`).

Two shapes of model:
- [`DeterministicMachine`]: the executable reference — same state + input
  always produces the same outcome. Runs produce [`RunRecord`]s suitable
  for canonical traces.
- [`AbstractModel`]: a (possibly nondeterministic) permission structure —
  which transitions are allowed — used as the target of refinement
  checking in `tpt-concord-check`.

[`TransitionTable`] and [`AbstractTable`] are ready-made table-driven
implementations; most real models implement the traits directly.

## License

Dual-licensed under **MIT OR Apache-2.0**. Copyright TPT Solutions.
