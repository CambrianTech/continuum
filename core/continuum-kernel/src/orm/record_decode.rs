//! A stored row read as its entity: the one decode every reader of a `DataRecord` goes through.
//!
//! A column an entity gained after a row was written is NULL in that row, and the store cannot tell
//! it from a field written as null. Decoded strictly, a NULL the type can hold (an `Option`, a
//! `serde_json::Value`) reads as written; only when the strict decode fails and the row holds NULLs
//! are they read as columns the row predates, absent, so the entity's `#[serde(default)]` answers
//! them and a required field missing names itself. Before this every reader called
//! `serde_json::from_value(row.data)` itself (15 production sites), and a `#[serde(default)]` field
//! added later failed on every older row: on the M5 (2026-11-10) the training trigger's recovery
//! died every 30 s on 380 submissions with `learned` NULL ("invalid type: null, expected u32"), so
//! no resident's in-place learning ran.

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use super::types::DataRecord;

impl DataRecord {
    /// The entity this row holds (module doc).
    pub fn decode<T: DeserializeOwned>(self) -> Result<T, serde_json::Error> {
        decode_row(self.data)
    }
}

/// A row's data read as its entity (module doc).
pub fn decode_row<T: DeserializeOwned>(data: Value) -> Result<T, serde_json::Error> {
    let strict = T::deserialize(&data);
    match (strict, data) {
        (Ok(entity), _) => Ok(entity),
        (Err(error), Value::Object(mut fields)) if fields.values().any(Value::is_null) => {
            fields.retain(|_, value| !value.is_null());
            serde_json::from_value(Value::Object(fields)).map_err(|_| error) // map_err: the strict error is the one to report when reading NULLs as absent does not help
        }
        (Err(error), _) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct Row {
        sequence: u64,
        #[serde(default)]
        learned: u32,
        note: Option<String>,
        payload: Value,
    }

    // what this catches (the M5, 2026-11-10: training recovery dead on 380 rows): a field added
    // later with `#[serde(default)]` reads NULL on an older row as its default; a value written as
    // null still reads as null; a required field that is NULL still refuses, naming the type error.
    #[test]
    fn a_column_the_row_predates_reads_as_absent_and_a_written_null_as_null() {
        let older = serde_json::json!({ "sequence": 7, "learned": null, "note": null, "payload": null });
        assert_eq!(
            decode_row::<Row>(older).expect("test: an older row decodes"), // JUSTIFIED: test fixture
            Row { sequence: 7, learned: 0, note: None, payload: Value::Null },
            "learned defaults; the written null payload stays null"
        );
        let required = serde_json::json!({ "sequence": null, "learned": 3, "note": null, "payload": 1 });
        assert!(decode_row::<Row>(required).is_err(), "a required field NULL is still refused");
    }
}
