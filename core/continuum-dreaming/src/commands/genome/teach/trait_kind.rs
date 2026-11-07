//! Validated teaching trait identity at the command boundary.
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct TeachTraitKind(String);

impl TryFrom<String> for TeachTraitKind {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            Err("teaching trait must contain a non-whitespace character")
        } else {
            Ok(Self(value))
        }
    }
}
impl TeachTraitKind {
    pub fn as_str(&self) -> &str { &self.0 }
}
impl<'de> Deserialize<'de> for TeachTraitKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
impl schemars::JsonSchema for TeachTraitKind {
    fn schema_name() -> String { "TeachTraitKind".into() }
    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        let mut schema = String::json_schema(generator).into_object();
        schema.string = Some(Box::new(schemars::schema::StringValidation {
            pattern: Some(r"\S".into()),
            ..Default::default()
        }));
        schemars::schema::Schema::Object(schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preparation_command_cannot_deserialize_without_a_valid_trait() {
        let valid = serde_json::json!({
            "action": "prepareAndSubmit", "candidateId": uuid::Uuid::nil(),
            "recipient": uuid::Uuid::nil().to_string(), "baseModelId": "fixture",
            "traitKind": "code/owner", "heldoutGym": "fixture-gym",
        });
        assert!(serde_json::from_value::<super::super::TeachTrainingAction>(valid.clone()).is_ok());
        for value in [serde_json::Value::Null, serde_json::json!(""), serde_json::json!(" \t"), serde_json::json!(7)] {
            let mut row = valid.clone();
            row["traitKind"] = value;
            assert!(serde_json::from_value::<super::super::TeachTrainingAction>(row).is_err());
        }
        let mut absent = valid;
        absent.as_object_mut().expect("fixture object").remove("traitKind");
        assert!(serde_json::from_value::<super::super::TeachTrainingAction>(absent).is_err());
    }

    #[test]
    fn trait_identity_is_validated_before_a_preparation_can_exist() {
        for value in [serde_json::Value::Null, serde_json::json!(""), serde_json::json!(" \t\n"), serde_json::json!(7)] {
            assert!(serde_json::from_value::<TeachTraitKind>(value).is_err());
        }
        let name = "code/owner";
        let kind: TeachTraitKind = serde_json::from_value(serde_json::json!(name)).expect("valid trait");
        assert_eq!(kind.as_str(), name);
        assert_eq!(serde_json::to_value(&kind).expect("serialize trait"), serde_json::json!(name));
        let schema = serde_json::to_value(schemars::schema_for!(TeachTraitKind)).expect("schema");
        assert_eq!(schema["type"], "string");
        assert_eq!(schema["pattern"], r"\S");
    }
}
