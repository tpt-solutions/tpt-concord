// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Versions of specifications, models, implementations and evidence.
//!
//! A version is an opaque, validated string (e.g. `1.2.0`, a content hash, or
//! a git SHA). Concord never interprets it; it records and compares it.

use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// An opaque version string for any Concord object.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Version(String);

impl Version {
    /// Create a validated version string (non-empty, no whitespace, ≤256 bytes).
    pub fn new(s: impl Into<String>) -> Result<Self, Error> {
        let s = s.into();
        if s.is_empty() {
            return Err(Error::InvalidId(s, "version must not be empty".to_string()));
        }
        if s.len() > 256 {
            return Err(Error::InvalidId(
                s,
                "version must be at most 256 bytes".to_string(),
            ));
        }
        if s.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(Error::InvalidId(
                s,
                "version must not contain whitespace or control characters".to_string(),
            ));
        }
        Ok(Self(s))
    }

    /// The version as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Version {
    type Error = Error;
    fn try_from(s: String) -> Result<Self, Error> {
        Self::new(s)
    }
}

impl From<Version> for String {
    fn from(v: Version) -> String {
        v.0
    }
}

impl FromStr for Version {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        Self::new(s)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Version {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_common_shapes() {
        for v in ["1.2.3", "git-58546e7", "sha256:abc123", "2026.10.05"] {
            assert!(Version::new(v).is_ok(), "should accept {v}");
        }
    }

    #[test]
    fn rejects_invalid() {
        assert!(Version::new("").is_err());
        assert!(Version::new("1 2 3").is_err());
    }
}
