// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

use tpt_concord_spec::{SpecState, Value, ValueType};

#[derive(SpecState)]
#[spec(id = "spec.wal", version = "1.2.0", description = "WAL state")]
struct Wal {
    committed: u64,
    #[spec(rename = "in_flight")]
    pending: Vec<u64>,
    open: bool,
    #[spec(skip)]
    #[allow(dead_code)]
    scratch: Vec<u8>,
}

#[derive(SpecState)]
struct DoorController {
    locked: bool,
    label: String,
}

#[test]
fn declares_non_skipped_fields_with_spec_types() {
    let vars = Wal::state_vars();
    assert_eq!(vars.len(), 3, "scratch is skipped: {vars:?}");
    assert_eq!(vars["committed"], ValueType::Int);
    assert_eq!(vars["in_flight"], ValueType::Seq(Box::new(ValueType::Int)));
    assert_eq!(vars["open"], ValueType::Bool);
    assert!(
        !vars.contains_key("pending"),
        "renamed field uses its new key"
    );
}

#[test]
fn reads_current_values_into_a_spec_state() {
    let wal = Wal {
        committed: 7,
        pending: vec![8, 9],
        open: true,
        scratch: vec![1, 2, 3],
    };
    let state = wal.to_spec_state();
    assert_eq!(state["committed"], Value::Int(7));
    assert_eq!(
        state["in_flight"],
        Value::Seq(vec![Value::Int(8), Value::Int(9)])
    );
    assert_eq!(state["open"], Value::Bool(true));
    assert_eq!(state.len(), 3);
}

#[test]
fn specification_carries_identity_and_declared_state() {
    let spec = Wal::specification();
    assert_eq!(spec.id.as_str(), "spec.wal");
    assert_eq!(spec.version.to_string(), "1.2.0");
    assert_eq!(spec.description, "WAL state");
    assert_eq!(spec.state_vars, Wal::state_vars());
}

#[test]
fn identity_defaults_from_the_struct_name() {
    let spec = DoorController::specification();
    assert_eq!(spec.id.as_str(), "spec.door_controller");
    assert_eq!(spec.version.to_string(), "0.1.0");
    assert_eq!(spec.name, "DoorController");
    assert_eq!(spec.state_vars.len(), 2);
    assert_eq!(spec.state_vars["label"], ValueType::Str);
}

#[test]
fn generated_state_matches_the_declared_types() {
    // Every value the derive produces must satisfy the type it declares.
    let door = DoorController {
        locked: false,
        label: "front".into(),
    };
    let vars = DoorController::state_vars();
    for (name, value) in door.to_spec_state() {
        assert!(vars[&name].matches(&value), "{name} has the wrong type");
    }
}

#[test]
#[should_panic(expected = "invalid specification id")]
fn invalid_identity_fails_loudly() {
    #[derive(SpecState)]
    #[spec(id = "has a space")]
    #[allow(dead_code)]
    struct Bad {
        x: bool,
    }
    let _ = Bad::specification();
}
