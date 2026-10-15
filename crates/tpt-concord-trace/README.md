# tpt-concord-trace

Canonical traces: event identity, causal ordering, normalization, replay, divergence.

Part of [TPT Concord](https://github.com/tpt-solutions/tpt-concord) — the evidence and conformance layer that
connects specifications, claims, proofs, tests and release decisions.

## Overview

Canonical execution traces (spec §5, `tpt-concord-trace`).

A [`Trace`] is a content-addressed sequence of events:
- **event identity**: every event's ID is the SHA-256 of its canonical
  encoding (`seq`, parent, kind, payload) — tamper-evident by construction;
- **causal ordering**: events may reference a parent event, always
  earlier in the trace;
- **normalization**: payloads are canonically serialized (sorted keys) so
  equal states have equal encodings;
- **replay**: fold a trace over a step function, verifying integrity first;
- **divergence comparison**: locate and classify where two traces disagree.

## License

Dual-licensed under **MIT OR Apache-2.0**. Copyright TPT Solutions.
