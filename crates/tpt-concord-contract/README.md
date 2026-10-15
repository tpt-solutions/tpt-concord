# tpt-concord-contract

Optional runtime contracts: pre/postcondition and invariant checks (enforcement, not proof).

Part of [TPT Concord](https://github.com/tpt-solutions/tpt-concord) — the evidence and conformance layer that
connects specifications, claims, proofs, tests and release decisions.

## Overview

`tpt-concord-contract` — optional runtime contracts (spec §5).

Precondition, postcondition and invariant checks executable at runtime.
**Runtime contracts are enforcement, not proof**: a checked run that
passes is not evidence of correctness beyond the states actually visited
(that is `type_checked`-family instrumentation, never `proven`).

Enforcement modes:
- [`Enforcement::Always`] — checks run in debug *and* release;
- [`Enforcement::DebugOnly`] — checks run under `debug_assertions` and
  are compiled out of release (zero-cost instrumentation).

## License

Dual-licensed under **MIT OR Apache-2.0**. Copyright TPT Solutions.
