# Changelog

All notable changes to `tpt-concord-diff` are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); this crate follows
[Semantic Versioning](https://semver.org/) as described in the workspace
[versioning policy](../../docs/versioning.md).

## [Unreleased]

## [0.1.0] — 2026-10-05

### Added

- `DiffReport`: self-describing comparison result carrying both sides, both trace IDs
  and all positional divergences, with `agree()` and a one-line `verdict()`.
- `DiffSide` (`Model` / `Implementation`, with optional versions) and `label()`.
- `diff` primitive plus `impl_vs_model` and `version_vs_version` convenience functions
  (impl vs impl uses `diff` directly).
- Serde support so reports can be stored as evidence.
