//! Storage-only migration for responses recorded before dispatch receipts existed.
//! Live wire decoding remains strict; missing history cannot establish admission.
use serde::Deserialize;
use super::types::TextGenerationResponse;

pub fn decode(mut value: serde_json::Value) -> Result<TextGenerationResponse, serde_json::Error> {
    if let Some(object) = value.as_object_mut() {
        object.entry("dispatch").or_insert_with(|| serde_json::json!({"kind": "legacy_unrecorded"}));
    }
    serde_json::from_value(value) // Storage boundary: historical captured/training response, never a live adapter reply.
}

pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<TextGenerationResponse, D::Error> {
    decode(serde_json::Value::deserialize(deserializer)?).map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::reply_room::DispatchReceipt;

    fn historical() -> serde_json::Value {
        serde_json::json!({"text":"retained", "finishReason":"stop", "model":"recorded-model",
            "provider":"recorded-provider", "usage":{"inputTokens":27221,"outputTokens":100,"totalTokens":27321},
            "responseTimeMs":42,"requestId":"recorded-id","reasoning":"retained reasoning"})
    }

    #[test]
    fn historical_storage_loads_without_forging_live_admission() {
        let old = historical();
        assert!(serde_json::from_value::<TextGenerationResponse>(old.clone()).is_err());
        let response = decode(old).expect("historical record");
        assert_eq!(response.dispatch, DispatchReceipt::LegacyUnrecorded);
        assert!(response.dispatch.admitted_reply().is_err());
        assert_eq!(response.text, "retained");
        assert_eq!(response.reasoning.as_deref(), Some("retained reasoning"));
        assert_eq!(response.usage.input_tokens, 27221);
    }

    #[test]
    fn explicit_receipts_are_preserved_and_malformed_records_still_fail() {
        let mut value = historical();
        value["dispatch"] = serde_json::json!({"kind":"counted","room":{"prompt":27221,"window":32512,"reply":5290}});
        let response = decode(value.clone()).expect("recorded receipt");
        assert_eq!(response.dispatch.admitted_reply().expect("recorded cap"), 5290);
        value["dispatch"] = serde_json::Value::Null;
        assert!(decode(value.clone()).is_err());
        value.as_object_mut().expect("object").remove("dispatch");
        value.as_object_mut().expect("object").remove("text");
        assert!(decode(value).is_err());
    }
}
