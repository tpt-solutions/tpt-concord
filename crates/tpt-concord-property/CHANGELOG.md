# Changelog

All notable changes to `tpt-concord-property` are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); this crate follows
[Semantic Versioning](https://semver.org/) as described in the workspace
[versioning policy](../../docs/versioning.md).

## [Unreleased]

## [0.1.0] — 2026-10-05

### Added

- Seeded, platform-independent xorshift64* `Rng` (`new`, `next_u64`, `below`, `range`,
  `pick`, `iter`) so campaigns reproduce across machines.
- `Strategy` trait with `int_in` (boundary-biased at larger sizes) and `pick_from`.
- `stateful_sequence`: model-walking generation that emits only inputs the
  `AbstractModel` permits from the current state.
- `shrink`: delta-debugging style minimisation with a caller-supplied failure predicate,
  returning `ShrinkStats` (original length, minimal length, attempts).
