//! One owned, strict decode at the storage boundary.
//!
//! SQL NULL is a value, not evidence that a row predates a field. Schema changes
//! must migrate historical values explicitly; runtime decoding never removes NULLs
//! or retries with different semantics. Consuming the JSON tree lets serde move
//! owned strings and containers into the entity instead of cloning them.
use serde::de::DeserializeOwned;
use serde_json::Value;
use super::types::DataRecord;

impl DataRecord {
    pub fn decode<T: DeserializeOwned>(self) -> Result<T, serde_json::Error> {
        decode_row(self.data)
    }
}

pub fn decode_row<T: DeserializeOwned>(data: Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(data)
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

    #[test]
    fn null_is_not_reinterpreted_as_a_missing_column() {
        let invalid = serde_json::json!({"sequence": 7, "learned": null, "note": null, "payload": 1});
        assert!(decode_row::<Row>(invalid).is_err());
        let invalid_identity = serde_json::json!({"sequence": null, "learned": 2, "note": null, "payload": 1});
        assert!(decode_row::<Row>(invalid_identity).is_err());
        let missing = serde_json::json!({"sequence": 7, "note": null, "payload": 1});
        assert_eq!(decode_row::<Row>(missing).expect("test: declared default"),
            Row { sequence: 7, learned: 0, note: None, payload: Value::from(1) });
        let written = serde_json::json!({"sequence": u64::MAX, "learned": 2, "note": "null", "payload": null});
        assert_eq!(decode_row::<Row>(written).expect("test: exact values"),
            Row { sequence: u64::MAX, learned: 2, note: Some("null".into()), payload: Value::Null });
    }

    #[test]
    fn owned_string_storage_moves_into_the_entity() {
        #[derive(serde::Deserialize)]
        struct Text { body: String }
        let body = "stored payload".repeat(1024);
        let allocation = body.as_ptr();
        let mut fields = serde_json::Map::new();
        fields.insert("body".into(), Value::String(body));
        let decoded: Text = decode_row(Value::Object(fields)).expect("test: owned text");
        assert_eq!(decoded.body.as_ptr(), allocation, "decoding must not clone the payload");
    }
}
