# tpt-concord-proof

Proof-obligation abstraction with Lean/Telos/Eidos adapters and
independently checkable proof artifacts.

## What it is

Concord is not a theorem prover. It is the evidence and conformance layer
that connects provers to the rest of an assurance argument. `tpt-concord-proof`
is the seam where a prover plugs in: it describes *what must be proven* (a
`ProofObligation`), renders that obligation into a prover's source format,
interprets the prover's output, and records the result as a
`ProofArtifact` that anyone can re-check later.

The central rule is that **the core never takes an adapter's word for
anything.** What gets recorded is the artifact itself, its SHA-256 hash and
the exact command an independent verifier can re-run. The recorded
`ProofOutcome` is an annotation, not the source of truth.

### Where it sits in the pipeline

In the Concord pipeline (see [`docs/pipeline.md`](../../docs/pipeline.md)),
this crate enforces stage 4, "formal proof / model checking": each proof
obligation is discharged by an artifact whose hash and re-check command are
recorded, and `sorry`/`admit`/unverifiable output is *inconclusive*, never
proven. It builds on
[`tpt-concord-core`](../tpt-concord-core) (for `ObligationID`), and its
registry serializes into bundles produced by
[`tpt-concord-evidence`](../tpt-concord-evidence). The
[`tpt-concord-cli`](../tpt-concord-cli) `prove` subcommand is a thin wrapper
over the adapters. Spec section 7 applies here too: an AI may *propose* proof
scripts, but they stay unvalidated until the independent check passes.

## Features

- `ProofObligation`: id, precise statement and free-form context
  (definitions, assumptions in scope, pointers to the formalization).
- `ProverAdapter` trait: render an obligation, name the re-check command,
  interpret prover output.
- Built-in adapters: `LeanAdapter` (`lean4`), `TelosAdapter` (`telos`),
  `EidosAdapter` (`eidos`) and a configurable `ExternalAdapter` for any other
  proof system.
- Honest output parsing: a Lean `sorry` is `Inconclusive`, never `Proven`;
  output with no verdict marker is `Inconclusive`.
- `ProofArtifact`: obligation, adapter, source path, SHA-256 of the source,
  recheck command, recorded outcome and metadata.
- `ProofRegistry`: collects artifacts, queries by obligation, checks that
  every artifact claiming `Proven` is still intact, and round-trips through
  JSON.

## Installation

```sh
cargo add tpt-concord-proof
```

or in `Cargo.toml`:

```toml
[dependencies]
tpt-concord-proof = "0.1"
tpt-concord-core = "0.1"   # for ObligationID
```

## Quick start

```rust
use std::collections::BTreeMap;
use tpt_concord_core::ObligationID;
use tpt_concord_proof::{
    LeanAdapter, ProofArtifact, ProofObligation, ProofRegistry, ProverAdapter,
};

fn main() {
    // 1. State what must be proven.
    let mut obligation = ProofObligation::new(
        ObligationID::new("obligation.wal-recovery").unwrap(),
        "recovery preserves the committed prefix",
    );
    obligation
        .context
        .insert("crash_model".into(), "prefix-crash".into());

    // 2. Render it for the prover. The Lean scaffold ends in `sorry`:
    //    it is honest about being unproved.
    let adapter = LeanAdapter;
    let source = adapter.render(&obligation);

    // 3. Run the prover (not shown), feed its output back to the adapter.
    let outcome = adapter.parse_result("warning: declaration uses 'sorry'");

    // 4. Record the artifact: content hash + exact re-check command.
    let artifact = ProofArtifact::record(
        &obligation,
        &adapter,
        "artifacts/wal-recovery.lean",
        &source,
        outcome,
    );
    assert_eq!(artifact.recheck_command, "lake env lean artifacts/wal-recovery.lean");
    assert!(artifact.verify_source(&source));
    assert!(!artifact.claims_proven()); // a `sorry` never claims proof

    // 5. Keep it in a registry and check it later against the files on disk.
    let mut registry = ProofRegistry::new();
    registry.record(artifact);
    let mut sources = BTreeMap::new();
    sources.insert("artifacts/wal-recovery.lean".to_string(), source);
    assert!(registry.proven_and_intact(&sources));
    let json = registry.to_json().unwrap();
    assert_eq!(ProofRegistry::from_json(&json).unwrap(), registry);
}
```

From the command line, `concord prove` renders an obligation without
running a prover:

```sh
concord prove --adapter lean4 --obligation obligation.wal-recovery \
  --statement "recovery preserves the committed prefix" --out wal-recovery.lean
# external systems: --adapter name:ext:command-template
```

## API tour

| Item | Purpose |
|---|---|
| `ProofObligation` | `id: ObligationID`, `statement`, `context: BTreeMap<String, String>`; `ProofObligation::new(id, statement)` |
| `ProofOutcome` | `Proven`, `NotProven { detail }`, `Inconclusive { detail }`; serializes as `{"outcome": "not_proven", ...}` |
| `ProverAdapter` | `name`, `file_extension`, `render`, `check_command`, `parse_result` |
| `LeanAdapter` | Renders `theorem <sanitized_id> : <statement> := by sorry`; re-check is `lake env lean <path>` |
| `TelosAdapter` | Renders an s-expression `(claim ...)` with `(goal prove)`; re-check is `telos check <path>`; looks for `PROVEN` / `REJECTED` / `FAILED` |
| `EidosAdapter` | Renders `obligation ... status: open`; re-check is `eidos verify <path>`; looks for `verified` / `refuted` |
| `ExternalAdapter` | Public fields `name`, `extension`, `check_command_template` (`{artifact}` is substituted) |
| `check_output(&str)` | Shared generic interpretation: `sorry`/`admit`/`axiom` is inconclusive, `error`/`failed` is not proven, `proven`/`verified` is proven, anything else inconclusive |
| `ProofArtifact` | `record`, `verify_source`, `claims_proven` |
| `ProofRegistry` | `record`, `for_obligation`, `proven_and_intact`, `to_json`, `from_json` |

Modules: `adapter` and `artifact` (everything is also re-exported at the
crate root).

## Design notes, guarantees and limits

- **Recorded outcome is an assertion.** `claims_proven()` only says the
  adapter *interpreted* the output as `Proven`. Gates should treat it as a
  claim to be re-verified by running `recheck_command`, never as ground
  truth.
- **Tamper detection.** `verify_source` and
  `ProofRegistry::proven_and_intact` catch changed or missing source files
  for artifacts that claim `Proven`. Artifacts that do not claim a proof are
  not required to be present.
- **Parsing is heuristic and string-based.** Adapters look for markers in
  prover output; they do not run provers and do not understand proof terms.
  The Lean adapter treats any occurrence of `sorry` in the output as
  inconclusive and any line starting with `error` (or containing `error:`)
  as not proven; `check_output` additionally treats any mention of `axiom` or
  `admit` as a placeholder, which is deliberately conservative.
- **The Telos and Eidos adapters render simple scaffolds.** The `.telos` and
  `.eidos` formats here are Concord's rendering of an obligation, not a full
  front end for those systems; the external toolchain remains the authority.
- **No process execution.** This crate never launches a prover. Run it
  yourself (or via your CI) and pass the output to `parse_result`.
- Evidence kind matters: a proof artifact discharges a proof obligation; a
  fuzz campaign never does (see the assurance-level rules in
  [`docs/pipeline.md`](../../docs/pipeline.md)).

## Related crates

- [`tpt-concord-core`](../tpt-concord-core): `ObligationID`, evidence kinds
  and the Assurance Graph.
- [`tpt-concord-evidence`](../tpt-concord-evidence): bundles that store the
  rendered artifacts and the registry JSON.
- [`tpt-concord-release`](../tpt-concord-release): gates that require
  obligations to be discharged.
- [`tpt-concord-cli`](../tpt-concord-cli): `concord prove`.

## MSRV

Rust 1.75. MSRV bumps are breaking changes (see
[`docs/versioning.md`](../../docs/versioning.md)).

## License

Dual-licensed under **MIT OR Apache-2.0**; see
[LICENSE-MIT](../../LICENSE-MIT) and [LICENSE-APACHE](../../LICENSE-APACHE).
Copyright TPT Solutions.

See [CHANGELOG.md](CHANGELOG.md) for release history.
