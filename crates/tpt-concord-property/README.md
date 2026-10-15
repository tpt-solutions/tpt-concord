# tpt-concord-property

Property-based testing derived from specifications: input generation, stateful generation, invariant-aware shrinking.

Part of [TPT Concord](https://github.com/tpt-solutions/tpt-concord) — the evidence and conformance layer that
connects specifications, claims, proofs, tests and release decisions.

## Overview

`tpt-concord-property` — property-based testing derived from
specifications (spec §5).

Everything is driven by a seeded, deterministic RNG
([`rng::Rng`]): the same seed always produces the same campaign, so any
property failure is reproducible and can be recorded as evidence.

Three capabilities:
- **input generation** via [`Strategy`] implementations,
- **stateful generation** that walks an [`AbstractModel`], emitting only
  legal input sequences,
- **invariant-aware shrinking** that minimizes a failing sequence while
  the failure (e.g. an invariant violation) still reproduces.

## License

Dual-licensed under **MIT OR Apache-2.0**. Copyright TPT Solutions.
