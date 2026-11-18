#![cfg(test)]

//! Shared test boundary for ORM integration and load fixtures.
use super::*;

/// A scalar projection carries its requested values plus the record envelope's four fields.
fn assert_projection(row: &DataRecord, selected: serde_json::Map<String, serde_json::Value>) {
    let mut expected = selected;
    expected.insert("id".into(), serde_json::json!(row.id));
    expected.insert("createdAt".into(), serde_json::json!(row.metadata.created_at));
    expected.insert("updatedAt".into(), serde_json::json!(row.metadata.updated_at));
    expected.insert("version".into(), serde_json::json!(row.metadata.version));
    assert_eq!(row.data, serde_json::Value::Object(expected));
}

#[path = "derive_test.rs"]
mod derive_test;
#[path = "relational_derived_tests.rs"]
mod relational_derived_tests;
#[path = "relational_tests.rs"]
mod relational_tests;
#[path = "binary_tests.rs"]
mod binary_tests;
#[path = "binary_load_tests.rs"]
mod binary_load_tests;
