// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Assurance levels (spec §6).
//!
//! Concord explicitly distinguishes what kind of evidence exists for a claim.
//! The variants intentionally have **no implicit ordering** as equivalence: a
//! fuzz campaign is never a proof. [`AssuranceLevel::strictness_rank`] exists
//! only as a policy hint (e.g. "at least model-checked") and must never be
//! read as "fuzzing ranks close to proving".

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// What kind of assurance exists for a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssuranceLevel {
    /// A specification exists; nothing beyond that.
    Specified,
    /// The type system constrains the implementation.
    TypeChecked,
    /// An executable model was checked (model checking / exhaustive exploration).
    ModelChecked,
    /// Property-based testing found no violations.
    PropertyTested,
    /// A fuzz campaign found no violations.
    FuzzTested,
    /// A differential comparison found no disagreement.
    DifferentiallyChecked,
    /// Some properties are formally proven; others are not.
    PartiallyProven,
    /// All obligations of the claim are discharged by formal proof.
    FullyProven,
}

/// Coarse family of an assurance level.
///
/// Families keep evidence types honest: empirical testing can never be
/// promoted into [`EvidenceCategory::FormalProof`], and a proof is never
/// downgraded into "just testing".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvidenceCategory {
    /// The behaviour is written down, nothing more.
    Specification,
    /// Constraint obtained by static analysis of the implementation.
    StaticAnalysis,
    /// Exhaustive or systematic exploration of a model.
    ModelChecking,
    /// Sampling-based empirical evidence (property tests, fuzzing, differential).
    Empirical,
    /// Formal proof.
    FormalProof,
}

impl AssuranceLevel {
    /// All levels, in declaration order.
    pub const ALL: [AssuranceLevel; 8] = [
        AssuranceLevel::Specified,
        AssuranceLevel::TypeChecked,
        AssuranceLevel::ModelChecked,
        AssuranceLevel::PropertyTested,
        AssuranceLevel::FuzzTested,
        AssuranceLevel::DifferentiallyChecked,
        AssuranceLevel::PartiallyProven,
        AssuranceLevel::FullyProven,
    ];

    /// Canonical snake_case name (matches the serde representation).
    pub fn as_str(&self) -> &'static str {
        match self {
            AssuranceLevel::Specified => "specified",
            AssuranceLevel::TypeChecked => "type_checked",
            AssuranceLevel::ModelChecked => "model_checked",
            AssuranceLevel::PropertyTested => "property_tested",
            AssuranceLevel::FuzzTested => "fuzz_tested",
            AssuranceLevel::DifferentiallyChecked => "differentially_checked",
            AssuranceLevel::PartiallyProven => "partially_proven",
            AssuranceLevel::FullyProven => "fully_proven",
        }
    }

    /// The evidence family this level belongs to.
    pub fn category(&self) -> EvidenceCategory {
        match self {
            AssuranceLevel::Specified => EvidenceCategory::Specification,
            AssuranceLevel::TypeChecked => EvidenceCategory::StaticAnalysis,
            AssuranceLevel::ModelChecked => EvidenceCategory::ModelChecking,
            AssuranceLevel::PropertyTested
            | AssuranceLevel::FuzzTested
            | AssuranceLevel::DifferentiallyChecked => EvidenceCategory::Empirical,
            AssuranceLevel::PartiallyProven | AssuranceLevel::FullyProven => {
                EvidenceCategory::FormalProof
            }
        }
    }

    /// True only for [`AssuranceLevel::PartiallyProven`] and
    /// [`AssuranceLevel::FullyProven`].
    pub fn is_formal_proof(&self) -> bool {
        self.category() == EvidenceCategory::FormalProof
    }

    /// True for sampling-based empirical levels (property, fuzz, differential).
    pub fn is_empirical(&self) -> bool {
        self.category() == EvidenceCategory::Empirical
    }

    /// A policy-only rank so gates can express "at least X".
    ///
    /// This is a **hint for policy composition, not an equivalence**. Two
    /// levels with adjacent ranks are different kinds of evidence and must
    /// never be presented as substitutes for each other.
    pub fn strictness_rank(&self) -> u8 {
        match self {
            AssuranceLevel::Specified => 0,
            AssuranceLevel::TypeChecked => 1,
            AssuranceLevel::ModelChecked => 2,
            AssuranceLevel::PropertyTested => 3,
            AssuranceLevel::FuzzTested => 4,
            AssuranceLevel::DifferentiallyChecked => 5,
            AssuranceLevel::PartiallyProven => 6,
            AssuranceLevel::FullyProven => 7,
        }
    }

    /// The weakest level a gate should accept when requiring `min`.
    pub fn satisfies(&self, min: AssuranceLevel) -> bool {
        self.strictness_rank() >= min.strictness_rank()
    }
}

impl fmt::Display for AssuranceLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AssuranceLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        AssuranceLevel::ALL
            .iter()
            .find(|l| l.as_str() == s)
            .copied()
            .ok_or_else(|| format!("unknown assurance level {s:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_names() {
        for level in AssuranceLevel::ALL {
            assert_eq!(level.as_str().parse::<AssuranceLevel>().unwrap(), level);
            let json = serde_json::to_string(&level).unwrap();
            assert_eq!(json, format!("\"{}\"", level.as_str()));
            assert_eq!(
                serde_json::from_str::<AssuranceLevel>(&json).unwrap(),
                level
            );
        }
    }

    #[test]
    fn empirical_is_never_proof() {
        assert!(AssuranceLevel::FuzzTested.is_empirical());
        assert!(!AssuranceLevel::FuzzTested.is_formal_proof());
        assert!(AssuranceLevel::FullyProven.is_formal_proof());
        assert!(!AssuranceLevel::FullyProven.is_empirical());
    }

    #[test]
    fn policy_comparison() {
        assert!(AssuranceLevel::FullyProven.satisfies(AssuranceLevel::ModelChecked));
        // On the §6 scale, property testing is stronger than model checking;
        // gates that need the *exact* evidence kind must require it via policy.
        assert!(AssuranceLevel::PropertyTested.satisfies(AssuranceLevel::ModelChecked));
        assert!(!AssuranceLevel::ModelChecked.satisfies(AssuranceLevel::PropertyTested));
        assert!(!AssuranceLevel::FuzzTested.satisfies(AssuranceLevel::PartiallyProven));
    }

    #[test]
    fn unknown_name_is_error() {
        assert!("proof".parse::<AssuranceLevel>().is_err());
    }
}
