# Getting started

Concord answers one question: **given an implementation, what exactly do we
know about it, why, under which assumptions, and what remains unproven?**
This page gets you from zero to a CI gate in about ten minutes.

## 1. Install

```
cargo install tpt-concord-cli --locked
```

Prebuilt binaries for Linux, macOS and Windows are attached to each GitHub
release. Optional shell completions: `concord completions <bash|zsh|fish|powershell>`.

## 2. Scaffold and explore

```
concord init demo && cd demo
cat README.txt
```

`init` writes a door-controller example: a specification, a reference model,
a good and a bad trace, a deliberately buggy implementation, an assurance
graph, a release policy and a hash-verified evidence bundle. Every file is
plain JSON — open them to learn the schemas, then replace the contents with
your own system.

## 3. The workflow

| You want to know… | Run | Evidence it produces |
|---|---|---|
| Is my spec well-formed? | `concord spec spec.json` | `specified` |
| Does a recorded run obey the spec? | `concord check spec.json trace.json` | conformance trace |
| Can the model reach a bad state? | `concord simulate model.json --alphabet … --bad-state …` | `model_checked` |
| Does my implementation match the model? | `concord fuzz model.json --alphabet … --impl impl.json` | `fuzz_tested` |
| Did two versions diverge? | `concord diff a.json b.json` | `differentially_checked` |
| What do we know and not know? | `concord report graph.json --policy policy.json --bundle evidence` | — |
| May we ship? | `concord gate graph.json policy.json --bundle evidence` | release decision |

Store each result as an artifact in an evidence bundle (see
`tpt-concord-evidence`), reference it from an `EvidenceRecord` with its SHA-256,
and link the record to an obligation of a claim. `--bundle` makes the gate
re-hash every artifact, so an edited result cannot silently keep its
"validated" status.

## 4. Gate releases in CI

```yaml
# .github/workflows/release-gate.yml
on: [pull_request]
jobs:
  gate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: tpt-solutions/tpt-concord@v0.1.0
        with:
          graph: assurance/graph.json
          policy: assurance/policy.json
          bundle: assurance/evidence
```

The job fails if the gate rejects, and the markdown report — claims,
assumptions, evidence by family, and what is still unknown — appears in the
job summary.

## 5. Use it as a library

Each stage is a crate: `tpt-concord-core` (graph), `-spec`, `-model`,
`-trace`, `-check`, `-property`, `-fuzz`, `-diff`, `-proof`, `-sim`,
`-contract`, `-evidence`, `-release`. Start from
[`tpt-concord-pilot-archon-wal`](../crates/tpt-concord-pilot-archon-wal) for a
complete Rust example.

## Reading the evidence levels

Levels are families, not a ladder to climb by volume: a million fuzz cases
are still `fuzz_tested`, never `fully_proven`. See [`pipeline.md`](pipeline.md).
