//! Deterministic schema witnesses, not runtime defaults or a universal schema solver.
//! Unsupported constraints fail the conformance test explicitly, never count as coverage.
use serde_json::{json, Map, Value};

pub(super) fn from_schema(schema: &Value, root: &Value, depth: usize) -> Result<Value, String> {
    if unconstrained(schema) {
        return Ok(Value::Null);
    }
    if depth > 32 {
        return Err("recursive required schema needs a finite declared example".into());
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let pointer = reference.strip_prefix('#').ok_or_else(|| format!("external schema reference {reference}"))?;
        let target = root.pointer(pointer).ok_or_else(|| format!("unresolved reference {reference}"))?;
        return from_schema(target, root, depth + 1);
    }
    if let Some(value) = schema.get("const") {
        return Ok(value.clone());
    }
    if let Some(value) = schema.get("enum").and_then(Value::as_array).and_then(|v| v.first()) {
        return Ok(value.clone());
    }
    if let Some(value) = schema.get("examples").and_then(Value::as_array).and_then(|v| v.first()) {
        return Ok(value.clone());
    }
    for keyword in ["oneOf", "anyOf", "allOf"] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            if keyword == "allOf" && branches.len() != 1 {
                return Err("multi-branch allOf needs a constraint-intersection witness".into());
            }
            let mut errors = Vec::new();
            for branch in branches {
                match from_schema(branch, root, depth + 1) {
                    Ok(value) => return Ok(value),
                    Err(error) => errors.push(error),
                }
            }
            return Err(format!("{keyword} has no generated witness: {errors:?}"));
        }
    }
    let kind = schema
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| schema.get("type").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str));
    match kind {
        Some("object") => {
            let mut value = Map::new();
            if let Some(required) = schema.get("required").and_then(Value::as_array) {
                for field in required {
                    let name = field.as_str().ok_or("non-string required property")?;
                    let property = schema
                        .get("properties")
                        .and_then(|p| p.get(name))
                        .ok_or_else(|| format!("required {name} has no property schema"))?;
                    value.insert(name.into(), from_schema(property, root, depth + 1).map_err(|e| format!("{name}: {e}"))?);
                }
            }
            Ok(Value::Object(value))
        }
        Some("array") => {
            let count = schema.get("minItems").and_then(Value::as_u64).unwrap_or(0);
            if count > 64 {
                return Err("large minItems needs an explicit witness".into());
            }
            let mut values = Vec::new();
            for _ in 0..count {
                values.push(from_schema(schema.get("items").ok_or("array items absent")?, root, depth + 1)?);
            }
            Ok(Value::Array(values))
        }
        Some("string") => {
            if schema.get("pattern").is_some() {
                return Err("pattern requires a declared example or a pattern witness generator".into());
            }
            let text = match schema.get("format").and_then(Value::as_str) {
                Some("uuid") => "00000000-0000-4000-8000-000000000001",
                Some("date-time") => "2026-11-07T00:00:00Z",
                Some("date") => "2026-11-07",
                Some("uri") | Some("url") => "https://example.test/",
                Some(other) => return Err(format!("unsupported format {other}")),
                None => "sample",
            };
            let min = schema.get("minLength").and_then(Value::as_u64).unwrap_or(0) as usize;
            let max = schema.get("maxLength").and_then(Value::as_u64).map(|n| n as usize);
            if min > text.len() || max.is_some_and(|max| text.len() > max) {
                return Err("string length constraint needs a bounded witness".into());
            }
            Ok(json!(text))
        }
        Some("integer") | Some("number") => {
            if schema.get("exclusiveMinimum").is_some() || schema.get("exclusiveMaximum").is_some() || schema.get("multipleOf").is_some() {
                return Err("numeric constraint requires a boundary witness".into());
            }
            let lower = schema.get("minimum").and_then(Value::as_f64).unwrap_or(0.0);
            if kind == Some("integer") {
                // schemars writes bounds as f64, but JSON integer witnesses must
                // have integer storage for serde's integer deserializers.
                if lower >= 0.0 && lower.ceil() < u64::MAX as f64 {
                    Ok(json!(lower.ceil() as u64))
                } else if lower.ceil() >= i64::MIN as f64 && lower.ceil() < i64::MAX as f64 {
                    Ok(json!(lower.ceil() as i64))
                } else {
                    Err("integer minimum outside witness range".into())
                }
            } else {
                Ok(json!(lower))
            }
        }
        Some("boolean") => Ok(json!(false)),
        Some("null") => Ok(Value::Null),
        _ => Err("untyped schema requires an explicit witness".into()),
    }
}

/// Cover each enum/union alternative and each property (including optional ones)
/// through the real decoder. This is branch coverage, not a Cartesian proof.
pub(super) fn variants(schema: &Value, root: &Value, depth: usize) -> Result<Vec<Value>, String> {
    let mut values = branch_variants(schema, root, depth)?;
    if let Some(examples) = schema.get("examples").and_then(Value::as_array) {
        for example in examples {
            if !values.contains(example) {
                values.push(example.clone());
            }
        }
    }
    Ok(values)
}

fn branch_variants(schema: &Value, root: &Value, depth: usize) -> Result<Vec<Value>, String> {
    if unconstrained(schema) {
        return Ok(vec![Value::Null, json!(false), json!(0), json!("sample"), json!([]), json!({})]);
    }
    if depth > 32 {
        return Err("recursive schema needs explicit finite branch fixtures".into());
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let pointer = reference.strip_prefix('#').ok_or("external reference")?;
        return variants(root.pointer(pointer).ok_or("unresolved reference")?, root, depth + 1);
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return Ok(values.clone());
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(branches) = schema.get(key).and_then(Value::as_array) {
            let mut values = Vec::new();
            for branch in branches {
                values.extend(variants(branch, root, depth + 1)?);
            }
            return Ok(values);
        }
    }
    if let Some(types) = schema.get("type").and_then(Value::as_array) {
        let mut values = Vec::new();
        for kind in types {
            let mut branch = schema.clone();
            branch["type"] = kind.clone();
            values.extend(variants(&branch, root, depth + 1)?);
        }
        return Ok(values);
    }
    let base = from_schema(schema, root, depth)?;
    let mut values = vec![base.clone()];
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => {
            if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                for (name, property) in properties {
                    for sample in variants(property, root, depth + 1).map_err(|e| format!("{name}: {e}"))? {
                        let mut value = base.clone();
                        value[name] = sample;
                        if !values.contains(&value) {
                            values.push(value);
                        }
                    }
                }
            }
        }
        Some("array") => {
            let minimum = schema.get("minItems").and_then(Value::as_u64).unwrap_or(0);
            let count = minimum.max(1);
            if count > 64 {
                return Err("large array needs explicit branch fixtures".into());
            }
            if schema.get("maxItems").and_then(Value::as_u64).is_none_or(|max| max >= count) {
                for sample in variants(schema.get("items").ok_or("array items absent")?, root, depth + 1)? {
                    let value = Value::Array(vec![sample; count as usize]);
                    if !values.contains(&value) {
                        values.push(value);
                    }
                }
            }
        }
        _ => {}
    }
    Ok(values)
}

// Draft-07 true and an object containing annotations alone accept every JSON value.
// Unknown validation keywords remain errors, never become unconstrained witnesses.
fn unconstrained(schema: &Value) -> bool {
    schema == &Value::Bool(true)
        || schema.as_object().is_some_and(|fields| {
            fields
                .keys()
                .all(|key| matches!(key.as_str(), "title" | "description" | "$comment" | "default" | "examples"))
        })
}
