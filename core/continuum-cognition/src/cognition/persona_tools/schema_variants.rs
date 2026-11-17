//! A command whose params are an enum with data (`#[serde(tag = "action")]`: `code/git/merge` start | continue |
//! abort) derives a schema that is a top-level `oneOf` of object variants with no top-level `properties`. A model's
//! native tool contract takes ONE object schema, and lifting only the top-level `properties` turned such a command
//! into a tool with no parameters: on the M5 (2026-11-17) Benchy was shown `code_git_merge` with
//! `{"type":"object","properties":{}}`, called it with `{}` exactly as shown, and was refused for a missing
//! `action` three times. This merges the variants into the one object a model can call, losing no valid input:
//! - a property the variants declare alike is kept once; a single-value enum per variant (the tag) becomes the enum
//!   of all of them, each value described by its variant;
//! - a property the variants declare differently (another type, ref or constraint) is the `anyOf` of each distinct
//!   declaration, never the first one alone;
//! - required is what every variant requires (a field one variant requires is optional in the merge; the command
//!   validates the exact variant and refuses with its own error).
//!
//! A `oneOf` that is not all typed object variants (a `$ref` variant, a scalar, a schema with its own properties
//! too) is [`VariantsMerge::Unsupported`], named, never flattened to nothing.

use serde::Deserialize;
use serde_json::{Map, Value};

/// JSON Schema keywords this merge writes (keys of a schema object, not data fields).
const ENUM_KEYWORD: &str = "enum";
const ANY_OF_KEYWORD: &str = "anyOf";
const DESCRIPTION_KEYWORD: &str = "description";
const OBJECT_TYPE: &str = "object";

/// A schema with a top-level `oneOf`.
#[derive(Deserialize)]
struct OneOf {
    #[serde(rename = "oneOf")]
    one_of: Vec<Value>,
    #[serde(default)]
    properties: Option<Map<String, Value>>,
}

/// One `oneOf` branch, as far as the merge reads it: a typed object and its fields.
#[derive(Deserialize)]
struct ObjectVariant {
    #[serde(rename = "type")]
    kind: String,
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

    fn encode(self) -> Value {
        let mut schema = self.rest;
        if let Some(values) = self.values {
            schema.insert(ENUM_KEYWORD.into(), Value::Array(values));
        }
        Value::Object(schema)
    }
}

/// The one object schema a set of object variants merges into.
#[derive(Debug)]
pub(super) struct MergedObject {
    /// Every variant's properties, each accepting what any variant accepts.
    pub(super) properties: Value,
    /// What every variant requires.
    pub(super) required: Vec<String>,
}

/// What merging a schema's variants found.
#[derive(Debug)]
pub(super) enum VariantsMerge {
    /// No top-level `oneOf`: the plain object lift applies.
    NotVariants,
    /// The variants as one object.
    Merged(MergedObject),
    /// A `oneOf` this merge cannot express as one object without losing inputs.
    Unsupported(UnsupportedVariants),
}

/// Why a `oneOf` cannot become one object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnsupportedVariants {
    /// A branch is not a typed object (a `$ref`, a scalar, an untyped schema).
    NotAnObject,
    /// The schema declares properties of its own beside the `oneOf`.
    PropertiesBesideVariants,
    /// The `oneOf` is empty.
    NoVariants,
}

impl UnsupportedVariants {
    pub(super) fn reason(self) -> &'static str {
        match self {
            Self::NotAnObject => "a oneOf branch is not a typed object",
            Self::PropertiesBesideVariants => "the schema has properties of its own beside its oneOf",
            Self::NoVariants => "the oneOf is empty",
        }
    }
}

/// The variants of `schema` merged into one object schema.
pub(super) fn merged_object_variants(schema: &Value) -> VariantsMerge {
    let Ok(OneOf { one_of, properties }) = OneOf::deserialize(schema) else {
        return VariantsMerge::NotVariants;
    };
    if properties.is_some() {
        return VariantsMerge::Unsupported(UnsupportedVariants::PropertiesBesideVariants);
    }
    let variants: Option<Vec<ObjectVariant>> = one_of
        .iter()
        .map(|branch| ObjectVariant::deserialize(branch).ok().filter(|v| v.kind == OBJECT_TYPE))
        .collect();
    let Some(variants) = variants else {
        return VariantsMerge::Unsupported(UnsupportedVariants::NotAnObject);
    };
    let Some(first) = variants.first() else {
        return VariantsMerge::Unsupported(UnsupportedVariants::NoVariants);
    };
    let tag = discriminator(&variants);
    let mut properties = Map::new();
    for name in property_names(&variants) {
        let declarations: Vec<&Value> = variants.iter().filter_map(|v| v.properties.get(name)).collect();
        let merged = match tag.as_deref() == Some(name) {
            true => tag_enum(&declarations),
            false => any_of_distinct(&declarations),
        };
        properties.insert(name.to_owned(), merged);
    }
    let required = first
        .required
        .iter()
        .filter(|name| variants.iter().all(|variant| variant.required.contains(name)))
        .cloned()
        .collect();
    if let Some(tag) = tag {
        describe_variants(&mut properties, &tag, &variants);
    }
    VariantsMerge::Merged(MergedObject { properties: Value::Object(properties), required })
}

/// Every property any variant declares, in first-seen order.
fn property_names(variants: &[ObjectVariant]) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    for name in variants.iter().flat_map(|v| v.properties.keys()) {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
    names
}

/// One declaration when every variant declares the property alike; otherwise the `anyOf` of each distinct one.
fn any_of_distinct(declarations: &[&Value]) -> Value {
    let mut distinct: Vec<&Value> = Vec::new();
    for declaration in declarations {
        if !distinct.contains(declaration) {
            distinct.push(declaration);
        }
    }
    if let [only] = distinct.as_slice() {
        return (*only).clone();
    }
    let mut any_of = Map::new();
    any_of.insert(ANY_OF_KEYWORD.into(), Value::Array(distinct.into_iter().cloned().collect()));
    Value::Object(any_of)
}

/// The tag's declarations (each the same schema but for its one enum value) as one enum of every value.
fn tag_enum(declarations: &[&Value]) -> Value {
    let mut decoded = declarations.iter().filter_map(|d| PropertySchema::decode(d));
    let Some(mut merged) = decoded.next() else {
        return any_of_distinct(declarations);
    };
    let values = merged.values.get_or_insert_with(Vec::new);
    for value in decoded.flat_map(|d| d.values.into_iter().flatten()) {
        if !values.contains(&value) {
            values.push(value);
        }
    }
    merged.encode()
}

/// The property that tells the variants apart: every variant declares it with exactly one enum value, and the
/// declarations differ in nothing else.
fn discriminator(variants: &[ObjectVariant]) -> Option<String> {
    let first = variants.first()?;
    first
        .properties
        .keys()
        .find(|name| {
            let decoded: Option<Vec<PropertySchema>> = variants
                .iter()
                .map(|v| v.properties.get(name.as_str()).and_then(PropertySchema::decode))
                .collect();
            decoded.is_some_and(|decoded| {
                decoded.iter().all(|d| d.values.as_ref().is_some_and(|values| values.len() == 1))
                    && decoded.iter().all(|d| d.rest == decoded[0].rest)
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
    use schemars::JsonSchema;

    fn merged(schema: &Value) -> MergedObject {
        match merged_object_variants(schema) {
            VariantsMerge::Merged(merged) => merged,
            other => panic!("expected a merge, got {other:?}"),
        }
    }

    /// Mirrors `code/git/merge`'s params (`continuum-hands` `GitMergeParams`): the same serde tagging and
    /// variant shapes, derived by the same schemars, so the test reads a derived schema, not a hand-written one.
    #[allow(dead_code)] // test: only its derived schema is read
    #[derive(JsonSchema, Deserialize)]
    #[serde(tag = "action", rename_all = "snake_case")]
    enum DerivedMerge {
        /// Merge `from` (e.g. `origin/canary`) into your branch.
        Start { from: String },
        /// After resolving and adding the files.
        Continue,
        /// Undo the merge.
        Abort,
    }

    #[derive(Deserialize)]
    struct Tag {
        #[serde(rename = "enum")]
        values: Vec<String>,
        description: String,
    }

    // what this catches: a command with enum-with-data params reaching the model as a tool with NO parameters
    // (Benchy, M5 2026-11-17: `code_git_merge` shown `properties: {}`, called with `{}`, refused three times). The
    // merge of the DERIVED schema offers the tag with every value and description, `from`, and requires only `action`
    // (`from` is required by `start` alone: conditional, so optional in the merge).
    #[test]
    fn a_derived_tagged_enum_reaches_the_model_as_one_callable_object() {
        let schema = serde_json::to_value(schemars::schema_for!(DerivedMerge)).expect("test: schema"); // test: the derived schema as published
        let MergedObject { properties, required } = merged(&schema);

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
    }

    // what this catches: first-wins on a property the variants declare differently (Codex's #6101 review), which
    // drops every input only a later variant accepts. Differing types and refs stay reachable as an anyOf.
    #[test]
    fn a_property_declared_differently_keeps_every_declaration() {
        let schema: Value = serde_json::from_str(
            r##"{"oneOf":[
                {"type":"object","required":["kind","size"],"properties":{"kind":{"type":"string","enum":["a"]},"size":{"type":"string"},"at":{"$ref":"#/definitions/Point"}}},
                {"type":"object","required":["kind","size"],"properties":{"kind":{"type":"string","enum":["b"]},"size":{"type":"integer","minimum":1},"at":{"$ref":"#/definitions/Area"}}}]}"##,
        )
        .expect("test: schema");
        let MergedObject { properties, required } = merged(&schema);

        #[derive(Deserialize)]
        struct AnyOf {
            #[serde(rename = "anyOf")]
            any_of: Vec<Value>,
        }
        #[derive(Deserialize)]
        struct Merged {
            size: AnyOf,
            at: AnyOf,
        }
        let merged = Merged::deserialize(&properties).expect("size and at are offered as anyOf");
        let size_declarations: Value = serde_json::from_str(r#"[{"type":"string"},{"type":"integer","minimum":1}]"#).expect("test: json");
        let at_declarations: Value = serde_json::from_str(r##"[{"$ref":"#/definitions/Point"},{"$ref":"#/definitions/Area"}]"##).expect("test: json");
        assert_eq!(Value::Array(merged.size.any_of), size_declarations, "both size types accepted");
        assert_eq!(Value::Array(merged.at.any_of), at_declarations, "both refs accepted");
        assert_eq!(required, ["kind", "size"], "required by every variant stays required");
    }

    // what this catches: a oneOf the merge cannot express silently becoming an empty object.
    #[test]
    fn a_oneof_that_is_not_typed_objects_is_refused_by_name() {
        let parse = |text: &str| -> Value { serde_json::from_str(text).expect("test: schema") };
        let refused = |schema: Value| match merged_object_variants(&schema) {
            VariantsMerge::Unsupported(why) => why,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert_eq!(
            refused(parse(r##"{"oneOf":[{"$ref":"#/definitions/A"},{"type":"object","properties":{}}]}"##)),
            UnsupportedVariants::NotAnObject
        );
        assert_eq!(refused(parse(r#"{"oneOf":[{"type":"string"}]}"#)), UnsupportedVariants::NotAnObject);
        assert_eq!(
            refused(parse(r#"{"properties":{"x":{"type":"string"}},"oneOf":[{"type":"object"}]}"#)),
            UnsupportedVariants::PropertiesBesideVariants
        );
        assert_eq!(refused(parse(r#"{"oneOf":[]}"#)), UnsupportedVariants::NoVariants);
        assert!(matches!(
            merged_object_variants(&parse(r#"{"type":"object","properties":{"x":{"type":"string"}}}"#)),
            VariantsMerge::NotVariants
        ));
    }
}
