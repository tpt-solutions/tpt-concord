// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `tpt-concord-diff` — differential verification (spec §5).
//!
//! Compares:
//! - implementation vs model,
//! - implementation vs implementation,
//! - version vs version.
//!
//! All three reduce to the same primitive — canonical trace comparison —
//! wrapped with the identities and versions involved so the result is a
//! self-describing, evidence-ready object. A differential result is
//! `differentially_checked` evidence; it is never a proof.

use serde::{Deserialize, Serialize};
use tpt_concord_core::{ImplementationID, TraceID, Version};
use tpt_concord_trace::{compare, Divergence, Trace};

/// The two sides of a differential comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffSide {
    /// The executable reference model.
    Model {
        model: tpt_concord_core::ModelID,
        model_version: Option<Version>,
    },
    /// An implementation.
    Implementation {
        implementation: ImplementationID,
        version: Option<Version>,
    },
}

impl DiffSide {
    /// Human-readable label for reports.
    pub fn label(&self) -> String {
        match self {
            DiffSide::Model {
                model,
                model_version,
            } => match model_version {
                Some(v) => format!("model:{model}@{v}"),
                None => format!("model:{model}"),
            },
            DiffSide::Implementation {
                implementation,
                version,
            } => match version {
                Some(v) => format!("impl:{implementation}@{v}"),
                None => format!("impl:{implementation}"),
            },
        }
    }
}

/// A self-describing differential comparison result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffReport {
    pub left: DiffSide,
    pub right: DiffSide,
    pub left_trace: TraceID,
    pub right_trace: TraceID,
    /// All divergences, positional.
    pub divergences: Vec<Divergence>,
}

impl DiffReport {
    /// True iff the traces are byte-identical under canonical comparison.
    pub fn agree(&self) -> bool {
        self.divergences.is_empty()
    }

    /// One-line verdict for logs and gate reasons.
    pub fn verdict(&self) -> String {
        if self.agree() {
            format!(
                "{} and {} agree (traces {} / {})",
                self.left.label(),
                self.right.label(),
                &self.left_trace.as_str()[..12],
                &self.right_trace.as_str()[..12]
            )
        } else {
            format!(
                "{} and {} disagree at {} point(s), first at index {}",
                self.left.label(),
                self.right.label(),
                self.divergences.len(),
                self.divergences[0].index
            )
        }
    }
}

/// Compare two traces with their provenance attached.
pub fn diff(left: (&Trace, DiffSide), right: (&Trace, DiffSide)) -> DiffReport {
    let divergences = compare(left.0, right.0);
    DiffReport {
        left: left.1,
        right: right.1,
        left_trace: left.0.id.clone(),
        right_trace: right.0.id.clone(),
        divergences,
    }
}

/// Convenience: implementation vs model.
pub fn impl_vs_model(
    impl_trace: (&Trace, ImplementationID, Option<Version>),
    model_trace: (&Trace, tpt_concord_core::ModelID, Option<Version>),
) -> DiffReport {
    diff(
        (
            impl_trace.0,
            DiffSide::Implementation {
                implementation: impl_trace.1,
                version: impl_trace.2,
            },
        ),
        (
            model_trace.0,
            DiffSide::Model {
                model: model_trace.1,
                model_version: model_trace.2,
            },
        ),
    )
}

/// Convenience: version vs version of the same implementation.
pub fn version_vs_version(
    old: (&Trace, ImplementationID, Version),
    new: (&Trace, ImplementationID, Version),
) -> DiffReport {
    diff(
        (
            old.0,
            DiffSide::Implementation {
                implementation: old.1,
                version: Some(old.2),
            },
        ),
        (
            new.0,
            DiffSide::Implementation {
                implementation: new.1,
                version: Some(new.2),
            },
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tpt_concord_trace::TraceBuilder;

    fn trace_with(values: &[i64]) -> Trace {
        let mut b = TraceBuilder::new();
        for v in values {
            b.event("step", json!({"v": v}));
        }
        b.finish()
    }

    fn side(label: &str) -> DiffSide {
        DiffSide::Implementation {
            implementation: ImplementationID::new(label).unwrap(),
            version: None,
        }
    }

    #[test]
    fn agreeing_traces_report_agreement() {
        let a = trace_with(&[1, 2, 3]);
        let b = trace_with(&[1, 2, 3]);
        let report = diff((&a, side("impl.a")), (&b, side("impl.b")));
        assert!(report.agree());
        assert!(report.verdict().contains("agree"));
    }

    #[test]
    fn disagreeing_traces_report_divergences() {
        let a = trace_with(&[1, 2, 3]);
        let b = trace_with(&[1, 5, 3]);
        let report = diff((&a, side("impl.a")), (&b, side("impl.b")));
        assert!(!report.agree());
        assert_eq!(report.divergences.len(), 1);
        assert_eq!(report.divergences[0].index, 1);
        assert!(report.verdict().contains("disagree"));
    }

    #[test]
    fn impl_vs_model_labels_sides() {
        let impl_t = trace_with(&[1]);
        let model_t = trace_with(&[1]);
        let report = impl_vs_model(
            (&impl_t, ImplementationID::new("i").unwrap(), None),
            (&model_t, tpt_concord_core::ModelID::new("m").unwrap(), None),
        );
        assert!(report.agree());
        assert!(report.left.label().starts_with("impl:"));
        assert!(report.right.label().starts_with("model:"));
    }

    #[test]
    fn version_vs_version_detects_regression() {
        let v1 = trace_with(&[1, 2, 3]);
        let v2 = trace_with(&[1, 2]);
        let report = version_vs_version(
            (
                &v1,
                ImplementationID::new("wal").unwrap(),
                Version::new("1.0.0").unwrap(),
            ),
            (
                &v2,
                ImplementationID::new("wal").unwrap(),
                Version::new("1.1.0").unwrap(),
            ),
        );
        assert!(!report.agree());
        assert!(report.left.label().contains("1.0.0"));
        assert!(report.right.label().contains("1.1.0"));
    }

    #[test]
    fn report_serializes() {
        let a = trace_with(&[1]);
        let b = trace_with(&[2]);
        let report = diff((&a, side("x")), (&b, side("y")));
        let json = serde_json::to_string(&report).unwrap();
        let back: DiffReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back, report);
    }
}
