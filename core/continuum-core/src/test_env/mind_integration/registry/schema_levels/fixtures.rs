#![cfg(test)]
//! Canonical custom-Deserialize coverage through the registered command, without effects.
use serde_json::json;

#[test]
fn code_edit_custom_decoder_accepts_each_advertised_mode_and_refuses_missing_fields() {
    let descriptor = crate::sdk_codegen::command_descriptor("code/edit").expect("registered edit");
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .should_validate_formats(true)
        .build(&descriptor.params_schema)
        .expect("edit schema");
    let modes = [
        (
            json!({"type":"line_range","start_line":1,"end_line":2,"new_content":"replacement"}),
            "new_content",
        ),
        (json!({"type":"search_replace","search":"before","replace":"after","all":true}), "replace"),
        (json!({"type":"insert_at","line":1,"content":"insert"}), "content"),
        (json!({"type":"append","content":"append"}), "content"),
    ];
    for (mode, required) in modes {
        let value = json!({"file_path":"src/example.rs", "edit_mode":mode, "description":"fixture"});
        assert!(validator.is_valid(&value), "fixture not advertised: {value}");
        assert!(descriptor.arguments.check(&value).is_ok(), "custom decoder refused: {value}");
        let mut absent = value;
        absent["edit_mode"].as_object_mut().expect("tagged mode").remove(required);
        assert!(!validator.is_valid(&absent), "missing {required} must leave the schema");
        assert!(descriptor.arguments.check(&absent).is_err(), "missing {required} must refuse");
    }
}

#[test]
fn branch_witnesses_include_optional_values_and_each_union_case() {
    let schema = json!({"type":"object", "properties":{
        "choice":{"anyOf":[{"type":"null"},{"$ref":"#/definitions/choice"}]},
        "entries":{"type":"array","items":{"enum":["first","second"]}}
    }, "definitions":{"choice":{"type":"string","enum":["left","right"]}}});
    let values = super::samples::variants(&schema, &schema, 0).expect("branch fixtures");
    let validator = jsonschema::draft7::new(&schema).expect("fixture schema");
    assert!(values.iter().all(|value| validator.is_valid(value)));
    for expected in [
        json!({}),
        json!({"choice":null}),
        json!({"choice":"left"}),
        json!({"choice":"right"}),
        json!({"entries":["first"]}),
        json!({"entries":["second"]}),
    ] {
        assert!(values.contains(&expected), "uncovered branch {expected}");
    }
}

#[test]
fn every_owner_example_is_validated_and_decoded_not_only_the_first() {
    let schema = json!({"type":"string", "pattern":"^[a-f]{4}$", "examples":["abcd","face"]});
    let values = super::samples::variants(&schema, &schema, 0).expect("owner examples");
    let validator = jsonschema::draft7::new(&schema).expect("fixture schema");
    assert_eq!(values, vec![json!("abcd"), json!("face")]);
    assert!(values.iter().all(|value| validator.is_valid(value)));
    let invalid = json!({"type":"string", "pattern":"^[a-f]{4}$", "examples":["abcd","nope"]});
    let values = super::samples::variants(&invalid, &invalid, 0).expect("examples are candidates");
    assert!(values.iter().any(|value| !validator.is_valid(value)), "the validator must expose a bad example");
}
