// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Executable reference models (spec §5, `tpt-concord-model`).
//!
//! Two shapes of model:
//! - [`DeterministicMachine`]: the executable reference — same state + input
//!   always produces the same outcome. Runs produce [`RunRecord`]s suitable
//!   for canonical traces.
//! - [`AbstractModel`]: a (possibly nondeterministic) permission structure —
//!   which transitions are allowed — used as the target of refinement
//!   checking in `tpt-concord-check`.
//!
//! [`TransitionTable`] and [`AbstractTable`] are ready-made table-driven
//! implementations; most real models implement the traits directly.

pub mod machine;

pub use machine::{
    run, AbstractModel, AbstractTable, DeterministicMachine, RunRecord, StepOutcome,
    TransitionTable, TransitionTableError,
};
