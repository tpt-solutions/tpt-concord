# tpt-concord-check

Conformance engine: impl/model comparison, invariant & refinement checking, counterexamples.

Part of [TPT Concord](https://github.com/tpt-solutions/tpt-concord) — the evidence and conformance layer that
connects specifications, claims, proofs, tests and release decisions.

## Overview

`tpt-concord-check` — the conformance engine (spec §5).

Bridges implementations and executable reference models:
- step-level conformance against a [`Specification`],
- invariant checking over states and traces,
- refinement checking against abstract models,
- deviation classification,
- first-class [`Counterexample`]s with a re-check workflow.

## License

Dual-licensed under **MIT OR Apache-2.0**. Copyright TPT Solutions.
