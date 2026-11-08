#![cfg(test)]
//! All registered callable schemas keep their contract as documentation collapses.
//! Decoder-value coverage is separate: this gate cannot prove custom Deserialize rules.

use crate::cognition::persona_tools::ai_safe_tool_specs;
use crate::cognition::tool_surface::{Floor, VerbLevels};

mod fixtures;
mod samples;

#[test]
fn every_registered_tool_decodes_a_schema_generated_call() {
    use crate::sdk_codegen::ext::command_registry_live;
    use crate::sdk_codegen::ArgumentsContract;
    let descriptors = command_registry_live();
    let specs = ai_safe_tool_specs();
    assert!(!specs.is_empty());
    let mut failures = Vec::new();
    for spec in specs {
        let descriptor = descriptors.iter().find(|d| d.name == spec.name).expect("offered command registered");
        if matches!(descriptor.arguments, ArgumentsContract::External) {
            failures.push(format!("{}: external decoder requires an explicit conformance adapter", spec.name));
            continue;
        }
        let levels = VerbLevels::of(&spec, &spec.name, "commands_help", Floor::Pointer);
        for level in 0..levels.len() {
            let schema = serde_json::to_value(&levels.spec(level).input_schema).expect("schema serialization");
            let validator = jsonschema::options()
                .with_draft(jsonschema::Draft::Draft7)
                .should_validate_formats(true)
                .build(&schema)
                .expect("advertised schema must compile independently");
            match samples::variants(&schema, &schema, 0) {
                Ok(values) => {
                    for value in values {
                        if !validator.is_valid(&value) {
                            failures.push(format!(
                                "{} level {level}: invalid generated witness {value}: {:?}",
                                spec.name,
                                validator.iter_errors(&value).map(|e| e.to_string()).collect::<Vec<_>>()
                            ));
                            continue;
                        }
                        if let Err(error) = descriptor.arguments.check(&value) {
                            failures.push(format!("{} level {level}: {error:?}; generated {value}", spec.name));
                        }
                    }
                }
                Err(error) => failures.push(format!("{} level {level}: {error}", spec.name)),
            }
        }
    }
    assert!(failures.is_empty(), "schema/decoder coverage gaps:\n{}", failures.join("\n"));
}

#[test]
fn every_registered_tool_keeps_its_callable_schema_at_every_level() {
    let specs = ai_safe_tool_specs();
    assert!(!specs.is_empty(), "the real linked command registry must be present");
    let mut levels_checked = 0;
    for spec in specs {
        let mut expected = spec.input_schema.clone();
        if let Some(properties) = expected.properties.as_object_mut() {
            for schema in properties.values_mut() {
                if let Some(fields) = schema.as_object_mut() {
                    fields.remove("description");
                }
            }
        }
        let levels = VerbLevels::of(&spec, &spec.name, "commands_help", Floor::Pointer);
        for level in 0..levels.len() {
            let mut actual = levels.spec(level).input_schema.clone();
            if let Some(properties) = actual.properties.as_object_mut() {
                for schema in properties.values_mut() {
                    if let Some(fields) = schema.as_object_mut() {
                        fields.remove("description");
                    }
                }
            }
            assert_eq!(
                serde_json::to_value(&actual).expect("schema serialization"),
                serde_json::to_value(&expected).expect("schema serialization"),
                "{} callable level {level} changed its value contract",
                spec.name
            );
            levels_checked += 1;
        }
    }
    assert!(levels_checked > 0);
}

#[test]
fn bare_inference_preflight_retains_the_handlers_request_id() {
    use crate::inference::llm_module::InferenceRequest;
    use crate::sdk_codegen::{command_descriptor, WireShape};
    let descriptor = command_descriptor("inference/llm/request").expect("real inference command");
    assert_eq!(descriptor.wire, WireShape::Bare);
    let schema = &descriptor.params_schema;
    let value = samples::from_schema(schema, schema, 0).expect("typed request witness");
    // The manual service's handle_request uses precisely this Params decoder.
    let decoded: InferenceRequest = serde_json::from_value(value.clone()).expect("actual bare decoder");
    assert_eq!(value["requestId"], serde_json::to_value(decoded.request_id).expect("request id"));
    assert!(descriptor.arguments.check(&value).is_ok(), "preflight must not strip requestId");
    let mut absent = value;
    absent.as_object_mut().expect("object request").remove("requestId");
    assert!(serde_json::from_value::<InferenceRequest>(absent.clone()).is_err());
    assert!(descriptor.arguments.check(&absent).is_err());
}

#[test]
fn offered_schema_preserves_the_registered_parameter_validation_contract() {
    fn validation_only(schema: &mut serde_json::Value) {
        match schema {
            serde_json::Value::Object(fields) => {
                for annotation in ["$schema", "title", "description", "default", "examples", "$comment"] {
                    fields.remove(annotation);
                }
                for optional in ["required", "definitions"] {
                    if fields.get(optional).is_some_and(serde_json::Value::is_null) {
                        fields.remove(optional);
                    }
                }
                // Empty properties imposes no constraints, just like an absent properties keyword.
                if fields.get("properties").and_then(serde_json::Value::as_object).is_some_and(|p| p.is_empty()) {
                    fields.remove("properties");
                }
                // Property/definition names are data; only their schema values recurse.
                for (key, child) in fields.iter_mut() {
                    if matches!(key.as_str(), "properties" | "definitions" | "$defs" | "patternProperties") {
                        if let Some(named) = child.as_object_mut() {
                            for value in named.values_mut() {
                                validation_only(value);
                            }
                        }
                    } else if matches!(
                        key.as_str(),
                        "additionalProperties"
                            | "additionalItems"
                            | "contains"
                            | "propertyNames"
                            | "not"
                            | "if"
                            | "then"
                            | "else"
                            | "items"
                            | "allOf"
                            | "anyOf"
                            | "oneOf"
                    ) {
                        validation_only(child);
                    } else if key == "dependencies" {
                        if let Some(dependencies) = child.as_object_mut() {
                            for value in dependencies.values_mut().filter(|v| !v.is_array()) {
                                validation_only(value);
                            }
                        }
                    }
                }
            }
            serde_json::Value::Array(items) => {
                for child in items {
                    validation_only(child);
                }
            }
            serde_json::Value::Bool(true) => *schema = serde_json::json!({}),
            _ => {}
        }
    }
    // Boolean validation keywords and literal values are not boolean schemas.
    let mut guard = serde_json::json!({"type":"array", "uniqueItems":true,
        "items":{"const":true}, "additionalItems":true,
        "properties":{"title":{"type":"boolean", "const":true}}});
    validation_only(&mut guard);
    assert_eq!(guard["uniqueItems"], true);
    assert_eq!(guard["items"]["const"], true);
    assert_eq!(guard["properties"]["title"]["const"], true);
    assert_eq!(guard["additionalItems"], serde_json::json!({}));
    for spec in ai_safe_tool_specs() {
        let descriptor = crate::sdk_codegen::command_descriptor(&spec.name).expect("real descriptor");
        let mut expected = descriptor.params_schema.clone();
        let mut offered = serde_json::to_value(spec.input_schema).expect("tool schema");
        validation_only(&mut expected);
        validation_only(&mut offered);
        assert_eq!(offered, expected, "{} projection changed validation", spec.name);
    }
}
