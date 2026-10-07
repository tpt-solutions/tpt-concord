// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `concord` — the TPT Concord command line (spec §5, `tpt-concord-cli`).
//!
//! Nine conceptual commands over machine-readable JSON objects:
//! `spec`, `model`, `check`, `prove`, `simulate`, `fuzz`, `diff`,
//! `evidence`, `gate`.
//!
//! Exit codes: `0` success/approval, `1` verification failure/rejection,
//! `2` usage or I/O error.

mod cmd;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "concord",
    version,
    about = "Specifications, claims, evidence, conformance and assurance as first-class objects",
    after_help = "All objects are JSON; see the crate docs for schemas."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate and inspect a specification JSON file.
    Spec {
        /// Path to a `Specification` JSON file.
        file: PathBuf,
    },
    /// Run a table-driven model over a sequence of inputs.
    Model {
        /// Path to a `TransitionTable<String,String>` JSON file.
        file: PathBuf,
        /// Path to a JSON file containing an array of input strings.
        #[arg(long)]
        inputs: PathBuf,
    },
    /// Conformance-check a canonical trace against a specification.
    Check {
        /// Path to a `Specification` JSON file.
        spec: PathBuf,
        /// Path to a `Trace` JSON file whose events carry step payloads.
        trace: PathBuf,
    },
    /// Render a proof obligation into a prover artifact.
    Prove {
        /// Prover adapter: lean4, telos, eidos, or `name:ext:command-template`.
        #[arg(long)]
        adapter: String,
        /// Obligation ID.
        #[arg(long)]
        obligation: String,
        /// The precise statement to prove.
        #[arg(long)]
        statement: String,
        /// Output path for the rendered artifact.
        #[arg(long)]
        out: PathBuf,
    },
    /// Explore the state space of a table-driven model.
    Simulate {
        /// Path to a `TransitionTable<String,String>` JSON file.
        file: PathBuf,
        /// Comma-separated input alphabet.
        #[arg(long)]
        alphabet: String,
        /// Maximum exploration depth.
        #[arg(long)]
        #[arg(default_value_t = 6)]
        depth: usize,
        /// Stop exploration at any state containing this substring.
        #[arg(long)]
        bad_state: Option<String>,
    },
    /// Fuzz a table-driven model with stateful input generation.
    Fuzz {
        /// Path to a `TransitionTable<String,String>` JSON file.
        file: PathBuf,
        /// Comma-separated input alphabet.
        #[arg(long)]
        alphabet: String,
        /// Base seed (campaigns are deterministic per seed).
        #[arg(long)]
        #[arg(default_value_t = 1)]
        seed: u64,
        /// Number of fuzz cycles.
        #[arg(long)]
        #[arg(default_value_t = 100)]
        cycles: usize,
        /// Length of each generated input sequence.
        #[arg(long)]
        #[arg(default_value_t = 12)]
        seq_len: usize,
    },
    /// Compare two canonical traces.
    Diff {
        /// First trace JSON file (reference).
        left: PathBuf,
        /// Second trace JSON file (candidate).
        right: PathBuf,
    },
    /// Verify an evidence bundle directory.
    Evidence {
        #[command(subcommand)]
        command: EvidenceCommand,
    },
    /// Evaluate a release policy against an assurance graph.
    Gate {
        /// Path to an `AssuranceGraph` JSON file.
        graph: PathBuf,
        /// Path to a `Policy` JSON file.
        policy: PathBuf,
    },
}

#[derive(Subcommand)]
enum EvidenceCommand {
    /// Verify every artifact hash in a bundle directory.
    Verify {
        /// Path to the bundle directory (must contain manifest.json).
        dir: PathBuf,
    },
    /// Print the bundle manifest.
    Show {
        /// Path to the bundle directory.
        dir: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Spec { file } => cmd::spec(&file),
        Command::Model { file, inputs } => cmd::model(&file, &inputs),
        Command::Check { spec, trace } => cmd::check(&spec, &trace),
        Command::Prove {
            adapter,
            obligation,
            statement,
            out,
        } => cmd::prove(&adapter, &obligation, &statement, &out),
        Command::Simulate {
            file,
            alphabet,
            depth,
            bad_state,
        } => cmd::simulate(&file, &alphabet, depth, bad_state.as_deref()),
        Command::Fuzz {
            file,
            alphabet,
            seed,
            cycles,
            seq_len,
        } => cmd::fuzz(&file, &alphabet, seed, cycles, seq_len),
        Command::Diff { left, right } => cmd::diff(&left, &right),
        Command::Evidence { command } => match command {
            EvidenceCommand::Verify { dir } => cmd::evidence_verify(&dir),
            EvidenceCommand::Show { dir } => cmd::evidence_show(&dir),
        },
        Command::Gate { graph, policy } => cmd::gate(&graph, &policy),
    };
    std::process::exit(code);
}
