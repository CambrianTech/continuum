//! A command whose params are an enum with data (`#[serde(tag = "action")]`: `code/git/merge` start | continue |
//! abort) derives a schema that is a top-level `oneOf` of object variants with no top-level `properties`. A model's
//! native tool contract takes ONE object schema, and lifting only the top-level `properties` turned such a command
//! into a tool with no parameters: on the M5 (2026-11-17) Benchy was shown `code_git_merge` with
//! `{"type":"object","properties":{}}`, called it with `{}` exactly as shown, and was refused for a missing
//! `action` three times. This merges the variants into the one object a model can call: the union of their
//! properties (same-named enums merged), required = what every variant requires, and each variant's description on
//! the property that tells them apart. The command still validates the exact variant and refuses with its own error.

use serde::Deserialize;
use serde_json::{Map, Value};

/// JSON Schema keywords this merge writes (keys of a schema object, not data fields).
const ENUM_KEYWORD: &str = "enum";
const DESCRIPTION_KEYWORD: &str = "description";

/// A schema made of object variants: a `oneOf` and no top-level properties of its own.
#[derive(Deserialize)]
struct Variants {
    #[serde(rename = "oneOf")]
    one_of: Vec<ObjectVariant>,
    #[serde(default)]
    properties: Option<Map<String, Value>>,
}

#[derive(Deserialize)]
struct ObjectVariant {
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    properties: Map<String, Value>,
    #[serde(default)]
    required: Vec<String>,
}

/// A property schema as far as the merge reads it: its `enum` values, everything else kept as it is.
#[derive(Deserialize)]
struct PropertySchema {
    #[serde(rename = "enum", default)]
    values: Option<Vec<Value>>,
    #[serde(flatten)]
    rest: Map<String, Value>,
}

impl PropertySchema {
    fn decode(value: &Value) -> Option<Self> {
        Self::deserialize(value).ok()
    }

    /// This property with `other`'s enum values added (unchanged when either has no enum).
    fn with_enum_of(mut self, other: &Value) -> Value {
        let more = PropertySchema::decode(other).and_then(|other| other.values);
        if let (Some(values), Some(more)) = (self.values.as_mut(), more) {
            for value in more {
                if !values.contains(&value) {
                    values.push(value);
                }
            }
        }
        self.encode()
    }

    fn encode(self) -> Value {
        let mut schema = self.rest;
        if let Some(values) = self.values {
            schema.insert(ENUM_KEYWORD.into(), Value::Array(values));
        }
        Value::Object(schema)
    }
}

/// The one object schema a set of object variants merges into.
pub(super) struct MergedObject {
    /// The union of the variants' properties.
    pub(super) properties: Value,
    /// What every variant requires.
    pub(super) required: Vec<String>,
}

/// The variants of `schema` merged into one object schema. `None` when `schema` is not a `oneOf` of object
/// variants (or also has properties of its own).
pub(super) fn merged_object_variants(schema: &Value) -> Option<MergedObject> {
    let Variants { one_of, properties: None } = Variants::deserialize(schema).ok()? else {
        return None;
    };
    let first = one_of.first()?;
    let mut properties: Map<String, Value> = Map::new();
    for variant in &one_of {
        for (name, property) in &variant.properties {
            let merged = match properties.remove(name) {
                None => property.clone(),
                Some(existing) => match PropertySchema::decode(&existing) {
                    Some(schema) => schema.with_enum_of(property),
                    None => existing,
                },
            };
            properties.insert(name.clone(), merged);
        }
    }
    let required = first
        .required
        .iter()
        .filter(|name| one_of.iter().all(|variant| variant.required.contains(name)))
        .cloned()
        .collect();
    if let Some(tag) = discriminator(&one_of) {
        describe_variants(&mut properties, &tag, &one_of);
    }
    Some(MergedObject { properties: Value::Object(properties), required })
}

/// The property every variant declares with exactly one enum value: what tells the variants apart.
fn discriminator(variants: &[ObjectVariant]) -> Option<String> {
    let first = variants.first()?;
    first
        .properties
        .keys()
        .find(|name| {
            variants.iter().all(|variant| {
                variant
                    .properties
                    .get(name.as_str())
                    .and_then(PropertySchema::decode)
                    .and_then(|property| property.values)
                    .is_some_and(|values| values.len() == 1)
            })
        })
        .cloned()
}

/// Describe each value of the discriminating property with its variant's description ("start: Merge ...").
fn describe_variants(properties: &mut Map<String, Value>, tag: &str, variants: &[ObjectVariant]) {
    let lines: Vec<String> = variants
        .iter()
        .filter_map(|variant| {
            let values = variant.properties.get(tag).and_then(PropertySchema::decode)?.values?;
            let Some(Value::String(value)) = values.into_iter().next() else {
                return None;
            };
            Some(match &variant.description {
                Some(description) => format!("{value}: {description}"),
                None => value,
            })
        })
        .collect();
    if let Some(Value::Object(property)) = properties.get_mut(tag) {
        property.insert(DESCRIPTION_KEYWORD.into(), Value::String(lines.join("; ")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a command with enum-with-data params reaching the model as a tool with NO parameters
    // (Benchy, M5 2026-11-17: `code_git_merge` shown `properties: {}`, called with `{}`, refused three times). The
    // merged schema must offer the discriminating `action` with every variant's value and description, the other
    // properties, and require only what every variant requires.
    #[test]
    fn a_oneof_of_object_variants_reaches_the_model_as_one_callable_object() {
        // `code/git/merge`'s derived params schema, as the core publishes it
        let schema: Value = serde_json::from_str(
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","description":"What to do with the merge.","oneOf":[
                {"description":"Merge `from` (e.g. `origin/canary`) into your branch.","type":"object","required":["action","from"],
                 "properties":{"action":{"type":"string","enum":["start"]},"from":{"type":"string"}}},
                {"description":"After resolving and adding the files.","type":"object","required":["action"],
                 "properties":{"action":{"type":"string","enum":["continue"]}}},
                {"description":"Undo the merge.","type":"object","required":["action"],
                 "properties":{"action":{"type":"string","enum":["abort"]}}}]}"#,
        )
        .expect("test: schema");
        let MergedObject { properties, required } = merged_object_variants(&schema).expect("a oneOf of object variants merges");

        #[derive(Deserialize)]
        struct Tag {
            #[serde(rename = "enum")]
            values: Vec<String>,
            description: String,
        }
        #[derive(Deserialize)]
        struct Merged {
            action: Tag,
            from: PropertyType,
        }
        #[derive(Deserialize)]
        struct PropertyType {
            #[serde(rename = "type")]
            kind: String,
        }
        let merged = Merged::deserialize(&properties).expect("action and from are offered");
        assert_eq!(merged.action.values, ["start", "continue", "abort"], "every variant's action is callable");
        assert_eq!(merged.from.kind, "string");
        assert!(merged.action.description.starts_with("start: Merge `from`"), "{}", merged.action.description);
        assert!(merged.action.description.contains("abort: Undo the merge."));
        assert_eq!(required, ["action"], "only what every variant requires");

        let plain: Value = serde_json::from_str(r#"{"type":"object","properties":{"x":{"type":"string"}}}"#).expect("test: schema");
        assert!(merged_object_variants(&plain).is_none(), "a plain object schema is left to the existing lift");
    }
}
