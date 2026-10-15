# tpt-concord-diff

Differential verification: impl vs model, impl vs impl, version vs version.

Part of [TPT Concord](https://github.com/tpt-solutions/tpt-concord) — the evidence and conformance layer that
connects specifications, claims, proofs, tests and release decisions.

## Overview

`tpt-concord-diff` — differential verification (spec §5).

Compares:
- implementation vs model,
- implementation vs implementation,
- version vs version.

All three reduce to the same primitive — canonical trace comparison —
wrapped with the identities and versions involved so the result is a
self-describing, evidence-ready object. A differential result is
`differentially_checked` evidence; it is never a proof.

## License

Dual-licensed under **MIT OR Apache-2.0**. Copyright TPT Solutions.
