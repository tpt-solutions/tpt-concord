// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Strongly-typed identifiers.
//!
//! All identifiers are non-empty strings without whitespace, bounded in length,
//! so they can be embedded in manifests, traces and CLI output without
//! escaping surprises. They serialize as plain strings.

use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const MAX_ID_LEN: usize = 256;

fn validate(s: &str, what: &'static str) -> Result<(), Error> {
    if s.is_empty() {
        return Err(Error::InvalidId(
            s.to_string(),
            format!("{what} must not be empty"),
        ));
    }
    if s.len() > MAX_ID_LEN {
        return Err(Error::InvalidId(
            s.to_string(),
            format!("{what} must be at most {MAX_ID_LEN} bytes"),
        ));
    }
    if s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(Error::InvalidId(
            s.to_string(),
            format!("{what} must not contain whitespace or control characters"),
        ));
    }
    Ok(())
}

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Create a validated identifier.
            pub fn new(s: impl Into<String>) -> Result<Self, Error> {
                let s = s.into();
                validate(&s, stringify!($name))?;
                Ok(Self(s))
            }

            /// The identifier as a string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = Error;
            fn try_from(s: String) -> Result<Self, Error> {
                Self::new(s)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }

        impl FromStr for $name {
            type Err = Error;
            fn from_str(s: &str) -> Result<Self, Error> {
                Self::new(s)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

define_id!(
    /// Identifies a specification (see `tpt-concord-spec`).
    SpecificationID
);
define_id!(
    /// Identifies a claim about a system.
    ClaimID
);
define_id!(
    /// Identifies an obligation that must be established before a claim is accepted.
    ObligationID
);
define_id!(
    /// Identifies a piece of evidence demonstrating an obligation.
    EvidenceID
);
define_id!(
    /// Identifies an executable reference model (see `tpt-concord-model`).
    ModelID
);
define_id!(
    /// Identifies an implementation under conformance checking.
    ImplementationID
);
define_id!(
    /// Identifies a canonical execution trace (see `tpt-concord-trace`).
    TraceID
);
define_id!(
    /// Identifies an explicit assumption a claim depends on.
    AssumptionID
);
define_id!(
    /// Identifies a scope bounding where a claim or assumption holds.
    ScopeID
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_ids() {
        let id = ClaimID::new("wal.recovery/committed-preserved_v1").unwrap();
        assert_eq!(id.as_str(), "wal.recovery/committed-preserved_v1");
        assert_eq!(id.to_string(), "wal.recovery/committed-preserved_v1");
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        assert!(matches!(ClaimID::new(""), Err(Error::InvalidId(_, _))));
        assert!(matches!(
            ClaimID::new("has space"),
            Err(Error::InvalidId(_, _))
        ));
        assert!(matches!(ClaimID::new("tab\t"), Err(Error::InvalidId(_, _))));
    }

    #[test]
    fn rejects_overlong() {
        let long = "a".repeat(257);
        assert!(matches!(ClaimID::new(long), Err(Error::InvalidId(_, _))));
    }

    #[test]
    fn serde_roundtrip_as_plain_string() {
        let id = ModelID::new("wal-model").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"wal-model\"");
        let back: ModelID = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn rejects_invalid_from_serde() {
        let res: Result<TraceID, _> = serde_json::from_str("\"not ok \"");
        assert!(res.is_err());
    }
}
