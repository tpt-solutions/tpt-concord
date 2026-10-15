// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `concord schema` — JSON Schemas for every file kind the CLI reads.
//!
//! The schemas are generated from the same Rust types the CLI deserializes,
//! so they cannot drift from what the tools accept. Point an editor at them
//! (or let `concord init` set the `$schema` key) and malformed specs,
//! traces and policies are flagged while you type.

use schemars::{schema_for, Schema};
use std::path::Path;
use tpt_concord_core::AssuranceGraph;
use tpt_concord_evidence::EvidenceBundle;
use tpt_concord_model::TransitionTable;
use tpt_concord_release::Policy;
use tpt_concord_spec::Specification;
use tpt_concord_trace::Trace;

/// The JSON files Concord reads, as `concord schema` names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    /// A `Specification` (`concord spec`, `concord check`).
    Spec,
    /// A `TransitionTable` (`concord model`, `simulate`, `fuzz`).
    Model,
    /// A canonical `Trace` (`concord check`, `concord diff`).
    Trace,
    /// An `AssuranceGraph` (`concord gate`, `concord report`).
    Graph,
    /// A release `Policy` (`concord gate`, `concord report`).
    Policy,
    /// An evidence bundle manifest, `manifest.json`.
    Manifest,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Spec,
        Kind::Model,
        Kind::Trace,
        Kind::Graph,
        Kind::Policy,
        Kind::Manifest,
    ];

    /// File name used when writing a schema to disk.
    pub fn file_name(self) -> &'static str {
        match self {
            Kind::Spec => "spec.schema.json",
            Kind::Model => "model.schema.json",
            Kind::Trace => "trace.schema.json",
            Kind::Graph => "graph.schema.json",
            Kind::Policy => "policy.schema.json",
            Kind::Manifest => "manifest.schema.json",
        }
    }

    pub fn schema(self) -> Schema {
        match self {
            Kind::Spec => schema_for!(Specification),
            Kind::Model => schema_for!(TransitionTable<String, String>),
            Kind::Trace => schema_for!(Trace),
            Kind::Graph => schema_for!(AssuranceGraph),
            Kind::Policy => schema_for!(Policy),
            Kind::Manifest => schema_for!(EvidenceBundle),
        }
    }
}

fn pretty(schema: &Schema) -> Result<String, String> {
    serde_json::to_string_pretty(schema).map_err(|e| format!("cannot serialize schema: {e}"))
}

/// `concord schema <kind>` prints one schema; `concord schema --dir <d>`
/// writes all of them.
pub fn schema(kind: Option<Kind>, dir: Option<&Path>) -> i32 {
    let result = match (kind, dir) {
        (_, Some(dir)) => write_all(dir),
        (Some(kind), None) => pretty(&kind.schema()).map(|text| println!("{text}")),
        (None, None) => Err(
            "name a kind (spec, model, trace, graph, policy, manifest) or pass --dir".to_string(),
        ),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("schema: {e}");
            2
        }
    }
}

pub fn write_all(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    for kind in Kind::ALL {
        let path = dir.join(kind.file_name());
        let text = pretty(&kind.schema())?;
        std::fs::write(&path, text + "\n")
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(())
}
