//! Serialization round-trips and shapes, gated behind the `serde` feature.
//!
//! Run with: `cargo test --features serde`
#![cfg(feature = "serde")]

use sequitur::{
    Components, ExecutionResult, FileOperation, FileSequence, Item, OperationPlan, Planned,
    SequenceError,
};
use std::path::PathBuf;

#[test]
fn components_round_trip() {
    let components = Components::new()
        .prefix("render")
        .delimiter("_")
        .padding(4)
        .suffix("_final")
        .extension("exr")
        .frame_number(42);

    let json = serde_json::to_string(&components).unwrap();
    let back: Components = serde_json::from_str(&json).unwrap();

    assert_eq!(components, back);
}

#[test]
fn components_deserialize_from_partial_json() {
    // A Tauri command might send only the fields the user changed.
    let json = r#"{ "prefix": "shot", "padding": 3 }"#;
    let components: Components = serde_json::from_str(json).unwrap();

    assert_eq!(components.prefix.as_deref(), Some("shot"));
    assert_eq!(components.padding, Some(3));
    assert_eq!(components.delimiter, None);
    assert_eq!(components.frame_number, None);
}

#[test]
fn item_serializes_with_named_fields() {
    let item = Item::from_filename("render_0042_final.exr", None).unwrap();
    let value: serde_json::Value = serde_json::to_value(&item).unwrap();

    assert_eq!(value["prefix"], "render");
    assert_eq!(value["delimiter"], "_");
    assert_eq!(value["frame_string"], "0042");
    assert_eq!(value["suffix"], "_final");
    assert_eq!(value["extension"], "exr");
}

#[test]
fn file_operation_serializes_as_tagged_enum() {
    let op = FileOperation::Rename {
        source: PathBuf::from("a.0001.exr"),
        destination: PathBuf::from("b.0001.exr"),
    };
    let value: serde_json::Value = serde_json::to_value(&op).unwrap();

    assert_eq!(value["Rename"]["source"], "a.0001.exr");
    assert_eq!(value["Rename"]["destination"], "b.0001.exr");
}

#[test]
fn parse_result_and_sequence_serialize() {
    let names = ["render_001.exr", "render_002.exr", "stray.txt"];
    let result = FileSequence::parse_filenames(&names, 1, Some(PathBuf::from("/proj")));

    let value: serde_json::Value = serde_json::to_value(&result).unwrap();
    assert!(value["sequences"].is_array());
    assert_eq!(value["sequences"][0]["items"][0]["frame_string"], "001");
    assert!(value["rogues"].is_array());
}

#[test]
fn planned_and_operation_plan_serialize() {
    let item = Item::from_filename("a_001.exr", Some(PathBuf::from("/proj"))).unwrap();
    let planned: Planned<Item> = item.rename(Components::new().prefix("b"));

    let value: serde_json::Value = serde_json::to_value(&planned).unwrap();
    assert_eq!(value["proposed"]["prefix"], "b");
    assert!(value["plan"]["operations"].is_array());

    // OperationPlan on its own also serializes.
    let plan: &OperationPlan = &planned.plan;
    let plan_json = serde_json::to_string(plan).unwrap();
    assert!(plan_json.contains("operations"));
}

#[test]
fn execution_result_flattens_io_errors() {
    let op = FileOperation::Delete {
        source: PathBuf::from("/nope"),
    };
    let result = ExecutionResult {
        executed: vec![],
        failed: vec![(op, std::io::Error::new(std::io::ErrorKind::NotFound, "boom"))],
    };

    let value: serde_json::Value = serde_json::to_value(&result).unwrap();
    assert_eq!(value["failed"][0]["operation"]["Delete"]["source"], "/nope");
    assert_eq!(value["failed"][0]["error"], "boom");
}

#[test]
fn sequence_error_serializes_as_message() {
    let err = SequenceError::Conflict(vec![PathBuf::from("a"), PathBuf::from("b")]);
    let value: serde_json::Value = serde_json::to_value(&err).unwrap();

    assert!(value.is_string());
    assert_eq!(value, "operation would overwrite 2 existing file(s)");
}
