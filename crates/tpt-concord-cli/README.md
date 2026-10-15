# tpt-concord-cli

The `concord` command line for [TPT Concord](https://github.com/tpt-solutions/tpt-concord): specifications,
claims, evidence, conformance and release gating as machine-readable JSON objects.

## Install

```
cargo install tpt-concord-cli --locked
concord completions bash > ~/.local/share/bash-completion/completions/concord   # or zsh, fish, powershell
```

## Try it in 60 seconds

```
concord init demo && cd demo
concord check spec.json trace-bad.json            # exit 1: invariant violated, with the step
concord fuzz model.json --alphabet open,close,lock,unlock --impl impl-buggy.json
                                                  # exit 1: minimal reproducing sequence
concord report graph.json --policy policy.json --bundle evidence
                                                  # markdown: claimed / why / assumptions / unknown
concord gate graph.json policy.json --bundle evidence   # exit 0: approved
```

## Commands

| Command | Purpose |
|---|---|
| `init [dir]` | Scaffold a runnable starter project with a hash-verified evidence bundle |
| `spec` | Validate a specification |
| `model` | Run a table-driven model over inputs |
| `check` | Conformance-check a trace against a specification |
| `simulate` | Explore a model's state space; minimal witness to a bad state |
| `fuzz [--impl]` | Stateful fuzzing; with `--impl`, compares an implementation to the model |
| `diff` | Compare two traces |
| `prove` | Render a proof obligation for Lean, Telos, Eidos or an external prover |
| `evidence verify\|show` | Re-verify or print an evidence bundle |
| `gate [--bundle]` | Release decision; with `--bundle`, evidence must re-hash or it is rejected |
| `report [--policy] [--bundle]` | Plain-markdown answer to "what do we know, and what don't we?" |
| `completions <shell>` | Shell completion script |

Exit codes: `0` success/approved, `1` verification failure/rejected, `2` usage or I/O error — so every
command drops straight into CI.

## Honesty rules

- A graph file is just JSON, so `gate` without `--bundle` takes its `"validated"` status on trust.
  **Use `--bundle` in CI**: validated evidence must then name an artifact that re-hashes correctly.
- `fuzz` without `--impl` only runs the model against itself and says so; it is never evidence.
- A fuzz campaign is `fuzz_tested`, never proof. `report` keeps evidence families separate.

## License

Dual-licensed under **MIT OR Apache-2.0**. Copyright TPT Solutions.
