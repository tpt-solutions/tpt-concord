# tpt-concord-sim

Deterministic simulation: virtual time, controlled scheduling, fault
injection, state-space exploration and replay.

## What it is

Many distributed and storage bugs only show up under a particular ordering of
events or a particular fault. `tpt-concord-sim` makes those orderings and
faults *data*: events fire in virtual time with no wall clock anywhere, faults
are scheduled objects applied at exact steps, and the same inputs always
produce the same log. That makes simulation runs reproducible, replayable and
suitable as evidence.

### Where it sits in the pipeline

In the Concord pipeline (see [`docs/pipeline.md`](../../docs/pipeline.md)),
`explore` contributes to stage 4, "formal proof / model checking": a
breadth-first sweep of a [`tpt-concord-model`](../tpt-concord-model)
deterministic machine that finds bad states with a minimal witness path.
Simulation logs serialize to JSON, so they can be stored as `Simulation`
artifacts in a [`tpt-concord-evidence`](../tpt-concord-evidence) bundle. The
[`tpt-concord-cli`](../tpt-concord-cli) `simulate` subcommand is a thin
wrapper over `explore`. Results are carried into the Assurance Graph from
[`tpt-concord-core`](../tpt-concord-core); a clean exploration is
`model_checked`-category evidence about the *model*, transferred to an
implementation only through conformance checking
([`tpt-concord-check`](../tpt-concord-check)).

## Features

- `Scheduler<P>`: virtual-time event queue ordered by `(time, seq)`; ties
  break by insertion order, so the order is total and replayable.
- `Fault` injection at exact step indices, recorded in the log.
- `run_with_faults`: runs a step function over inputs, producing a
  serializable `SimulationLog`.
- `verify_replay`: runs the same simulation twice and reports divergence.
- `explore` / `explore_bounded`: breadth-first state-space exploration of a
  `DeterministicMachine`, with a visited set, depth bound, optional
  transition budget, bad-state detection and a minimal witness path.

## Installation

```sh
cargo add tpt-concord-sim
```

or in `Cargo.toml`:

```toml
[dependencies]
tpt-concord-sim = "0.1"
tpt-concord-model = "0.1"   # for DeterministicMachine when using explore
```

## Quick start

Fault injection and replay:

```rust
use tpt_concord_sim::{run_with_faults, verify_replay, Fault};

/// A counter where input 0 resets; an injected `reset.lost` fault makes the
/// reset a no-op.
fn counter_step(state: i64, input: &i64, fault: Option<&Fault>) -> Result<i64, String> {
    let faulted = fault.is_some_and(|f| f.kind == "reset.lost");
    Ok(match input {
        0 if !faulted => 0,
        0 => state,
        n => state + n,
    })
}

fn main() {
    let inputs = vec![1, 1, 0, 1];
    let faults = vec![Fault { at_step: 2, kind: "reset.lost".into() }];

    let log = run_with_faults(0, &inputs, &faults, counter_step).unwrap();
    assert_eq!(log.steps[2].after, 2); // the faulted reset was swallowed
    assert_eq!(log.faults_applied, vec!["reset.lost".to_string()]);

    // Same inputs + faults => bit-identical log, or an Err describing the divergence.
    verify_replay(0, &inputs, &faults, counter_step).unwrap();

    // Logs are serde-serializable for use as evidence.
    let _json = serde_json::to_string(&log).unwrap();
}
```

(`serde_json` is only needed if you serialize the log; `SimulationLog`
implements `Serialize`/`Deserialize` for any serializable state and input.)

State-space exploration:

```rust
use tpt_concord_model::{DeterministicMachine, StepOutcome};
use tpt_concord_sim::explore;

/// Increments saturate at 3; any other input resets to 0.
struct Counter;
impl DeterministicMachine for Counter {
    type State = i64;
    type Input = i64;
    type Error = std::convert::Infallible;
    fn initial_state(&self) -> i64 { 0 }
    fn step(&self, s: &i64, i: &i64) -> Result<StepOutcome<i64>, Self::Error> {
        match i {
            1 => Ok(StepOutcome::to((*s + 1).min(3))),
            _ => Ok(StepOutcome::to(0)),
        }
    }
}

fn main() {
    let report = explore(&Counter, &[1, 0], 10, |s| *s == 3).unwrap();
    let (state, witness) = report.bad_state.unwrap();
    assert_eq!(state, 3);
    assert_eq!(witness, vec![1, 1, 1]); // BFS => a shortest path
}
```

Virtual-time scheduling:

```rust
use tpt_concord_sim::Scheduler;

let mut s = Scheduler::new();
s.schedule(10, "late-a");
s.schedule(5, "early");
s.schedule(10, "late-b");

let mut fired = Vec::new();
s.drain_until(100, |e| fired.push((e.at, e.payload)));
assert_eq!(fired, vec![(5, "early"), (10, "late-a"), (10, "late-b")]);
```

From the command line:

```sh
concord simulate model.json --alphabet put,get,crash --depth 6 --bad-state corrupt
```

The command prints a JSON report (`states_reached`, `transitions_explored`,
`max_depth_reached`, `bad_state` with `witness_inputs`) and exits `1` if a bad
state was found.

## API tour

| Item | Purpose |
|---|---|
| `Scheduler<P>` | `new`, `schedule(at, payload) -> seq`, `pop_next(until)`, `drain_until(until, f)`, `len`, `is_empty`, `next_time` |
| `ScheduledEvent<P>` | `at`, `seq`, `payload` |
| `Fault` | `at_step: usize`, `kind: String` (e.g. `"disk.truncate"`, `"net.drop"`) |
| `run_with_faults(initial, inputs, faults, step)` | Runs `step(state, &input, Option<&Fault>)` per input; stops at the first `Err` |
| `SimulationLog<S, I>` / `StepRecord<S, I>` | Per-step input, applied fault, before and after states; `faults_applied` in order |
| `verify_replay(initial, inputs, faults, step)` | Runs twice, compares logs, `Err(String)` on divergence |
| `explore(machine, alphabet, max_depth, is_bad)` | BFS; returns `ExplorationReport` |
| `explore::explore_bounded(..., max_transitions, is_bad)` | As `explore`, plus a global transition budget (not re-exported at the crate root) |
| `ExplorationReport<S, I>` | `states_reached`, `transitions_explored`, `max_depth_reached`, `bad_state: Option<(S, Vec<I>)>`, `found_bad_state()` |

Modules: `scheduler` and `explore`.

## Design notes, guarantees and limits

- **No wall clock.** Nothing in this crate reads real time or randomness.
  Determinism holds only if *your* step function and machine are themselves
  deterministic; `verify_replay` checks this empirically by running twice,
  it cannot prove it.
- **Faults are interpreted by your machine.** A `Fault` is just a step index
  and a kind string. The step function decides what a fault means; if it
  ignores the fault, the log still records it as applied. If two faults
  target the same step, only the first is delivered.
- **Exploration is bounded.** `explore` is exhaustive only for the reachable
  space within `max_depth`, and `explore_bounded` additionally stops when its
  transition budget is exhausted. A clean report on a depth- or
  budget-limited run means "no bad state found within these bounds", not
  "the system is correct". It is evidence about the *model* (and the chosen
  alphabet), not the implementation.
- **Illegal transitions are skipped.** A machine `Err` during exploration
  marks a step as outside the legal space and it is not counted as
  explored; it is not reported as a bad state.
- **Stops at the first bad state.** BFS order gives a minimal-length witness
  path, but only one witness is reported.
- Exploration requires `State: Eq + Hash + Clone` and `Input: Clone`.
- Non-goal: this is not a full distributed-systems simulator or network
  model; it provides the deterministic building blocks.

## Related crates

- [`tpt-concord-model`](../tpt-concord-model): `DeterministicMachine`, the
  machines `explore` runs over.
- [`tpt-concord-check`](../tpt-concord-check): conformance of an
  implementation against the model that was explored.
- [`tpt-concord-fuzz`](../tpt-concord-fuzz): sampling-based exploration of the
  same models (fuzz-tested, never model-checked).
- [`tpt-concord-evidence`](../tpt-concord-evidence): bundles for simulation
  logs.
- [`tpt-concord-cli`](../tpt-concord-cli): `concord simulate`.

## MSRV

Rust 1.75. MSRV bumps are breaking changes (see
[`docs/versioning.md`](../../docs/versioning.md)).

## License

Dual-licensed under **MIT OR Apache-2.0**; see
[LICENSE-MIT](../../LICENSE-MIT) and [LICENSE-APACHE](../../LICENSE-APACHE).
Copyright TPT Solutions.

See [CHANGELOG.md](CHANGELOG.md) for release history.
