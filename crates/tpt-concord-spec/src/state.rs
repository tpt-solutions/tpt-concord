// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Mapping Rust state onto specification state.
//!
//! [`SpecValue`] maps a Rust value to a spec [`Value`] and its [`ValueType`].
//! [`SpecState`] maps a whole struct to a spec [`State`]. With the `derive`
//! feature, `#[derive(SpecState)]` implements it for a struct and generates a
//! specification skeleton with its state variables already declared, so an
//! implementation's state and the spec cannot silently disagree on names or
//! types.

use crate::expr::{State, Value};
use crate::spec::{Specification, ValueType};
use std::collections::BTreeMap;
use tpt_concord_core::Version;

/// A Rust type that has a specification representation.
pub trait SpecValue {
    /// The declared spec type of this value.
    fn value_type() -> ValueType;
    /// The spec value of this instance.
    fn to_spec_value(&self) -> Value;
}

/// A struct whose fields are specification state variables.
pub trait SpecState {
    /// Declared state variables, by field name.
    fn state_vars() -> BTreeMap<String, ValueType>;
    /// The current state, as a spec state.
    fn to_spec_state(&self) -> State;

    /// A specification skeleton: identity, version and declared state.
    /// Invariants, transitions and bounds are left for the author to add.
    fn specification() -> Specification {
        let mut spec = skeleton("spec.skeleton", "0.1.0", "skeleton", "");
        spec.state_vars = Self::state_vars();
        spec
    }
}

/// Build an empty specification with the given identity.
///
/// Panics if `id` or `version` is not a valid Concord identifier, so a bad
/// `#[spec(...)]` attribute fails at the first call rather than being saved.
pub fn skeleton(id: &str, version: &str, name: &str, description: &str) -> Specification {
    let id = tpt_concord_core::SpecificationID::new(id)
        .unwrap_or_else(|e| panic!("invalid specification id {id:?}: {e}"));
    let version = Version::new(version)
        .unwrap_or_else(|e| panic!("invalid specification version {version:?}: {e}"));
    let mut spec = Specification::new(id, version, name);
    spec.description = description.to_string();
    spec
}

impl SpecValue for bool {
    fn value_type() -> ValueType {
        ValueType::Bool
    }
    fn to_spec_value(&self) -> Value {
        Value::Bool(*self)
    }
}

impl SpecValue for String {
    fn value_type() -> ValueType {
        ValueType::Str
    }
    fn to_spec_value(&self) -> Value {
        Value::Str(self.clone())
    }
}

macro_rules! int_values {
    ($($t:ty),*) => {$(
        impl SpecValue for $t {
            fn value_type() -> ValueType {
                ValueType::Int
            }
            fn to_spec_value(&self) -> Value {
                // Every listed type fits in i128, so this cannot truncate.
                Value::Int(i128::from(*self))
            }
        }
    )*};
}
int_values!(i8, i16, i32, i64, u8, u16, u32, u64);

impl SpecValue for usize {
    fn value_type() -> ValueType {
        ValueType::Int
    }
    fn to_spec_value(&self) -> Value {
        // Platform width is at most 64 bits, so this cannot truncate.
        Value::Int(*self as i128)
    }
}

impl SpecValue for isize {
    fn value_type() -> ValueType {
        ValueType::Int
    }
    fn to_spec_value(&self) -> Value {
        Value::Int(*self as i128)
    }
}

impl<T: SpecValue> SpecValue for Vec<T> {
    fn value_type() -> ValueType {
        ValueType::Seq(Box::new(T::value_type()))
    }
    fn to_spec_value(&self) -> Value {
        Value::Seq(self.iter().map(SpecValue::to_spec_value).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_map_to_their_spec_types() {
        assert_eq!(bool::value_type(), ValueType::Bool);
        assert_eq!(u64::MAX.to_spec_value(), Value::Int(u64::MAX as i128));
        assert_eq!(i64::MIN.to_spec_value(), Value::Int(i64::MIN as i128));
        assert_eq!(
            Vec::<u8>::value_type(),
            ValueType::Seq(Box::new(ValueType::Int))
        );
        assert_eq!(
            vec![1u8, 2].to_spec_value(),
            Value::Seq(vec![Value::Int(1), Value::Int(2)])
        );
    }
}
