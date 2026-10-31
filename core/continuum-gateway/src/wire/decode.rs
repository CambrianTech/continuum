//! The checked decoders every format shares. A field the agent sent is read whole or the call
//! is refused with why, before anything is selected or generated: a malformed tool never
//! disappears from the list, a token limit is never wrapped into range, a stop sequence is
//! never dropped. Absent (or `null`) means not sent, which each API defines; each format's
//! adapter holds only where its API really differs (field names, tool shapes, ranges).

use super::BadRequest;
use continuum_engine::ai::{NativeToolSpec, TextGenerationRequest, ToolChoice, ToolInputSchema};
use serde_json::{Map, Value};

/// The field `key`, when the agent sent it: absent and `null` are both not sent.
pub(crate) fn present<'a>(body: &'a Value, key: &str) -> Option<&'a Value> {
    body.get(key).filter(|value| !value.is_null())
}

/// The JSON type of a value, for a refusal (the value itself can be a whole schema).
pub(crate) fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// `key` as a whole count of at least `min`, refused past what the engine's `u32` holds.
pub(crate) fn count(body: &Value, key: &str, min: u32) -> Result<Option<u32>, BadRequest> {
    let Some(value) = present(body, key) else {
        return Ok(None);
    };
    let wide = value
        .as_u64()
        .ok_or_else(|| BadRequest(format!("`{key}` must be a whole number, got {value}")))?;
    let count = u32::try_from(wide).map_err(|_| {
        BadRequest(format!(
            "`{key}` = {wide} is past the largest this engine accepts ({})",
            u32::MAX
        ))
    })?;
    if count < min {
        return Err(BadRequest(format!(
            "`{key}` must be at least {min}, got {count}"
        )));
    }
    Ok(Some(count))
}

/// The answer's token limit from whichever of `keys` the agent sent (the APIs name it
/// differently and some accept an older name). Every key sent is checked; the first wins.
/// A limit is at least one token.
pub(crate) fn token_limit(body: &Value, keys: &[&str]) -> Result<Option<u32>, BadRequest> {
    let mut limit = None;
    for key in keys {
        let checked = count(body, key, 1)?;
        limit = limit.or(checked);
    }
    Ok(limit)
}

/// Whether the agent asked to stream. Not sent means no: every API's default.
pub(crate) fn streamed(body: &Value) -> Result<bool, BadRequest> {
    match present(body, "stream") {
        None => Ok(false),
        Some(Value::Bool(stream)) => Ok(*stream),
        Some(other) => Err(BadRequest(format!(
            "`stream` must be a boolean, got {}",
            kind(other)
        ))),
    }
}

/// A real number in `range`, narrowed to the engine's `f32`.
fn real(
    body: &Value,
    key: &str,
    range: std::ops::RangeInclusive<f64>,
) -> Result<Option<f32>, BadRequest> {
    let Some(value) = present(body, key) else {
        return Ok(None);
    };
    let number = value
        .as_f64()
        .ok_or_else(|| BadRequest(format!("`{key}` must be a number, got {}", kind(value))))?;
    if !range.contains(&number) {
        return Err(BadRequest(format!(
            "`{key}` must be within {}..={}, got {number}",
            range.start(),
            range.end()
        )));
    }
    Ok(Some(number as f32))
}

/// The sampling fields every format names the same way: temperature (whose ceiling is the
/// format's: 2 for OpenAI, 1 for Anthropic), top_p, and the stop sequences under `stop` or
/// `stop_sequences`, one string or an array of strings.
pub(crate) fn sampling(
    body: &Value,
    request: &mut TextGenerationRequest,
    temperature_max: f64,
) -> Result<(), BadRequest> {
    request.temperature = real(body, "temperature", 0.0..=temperature_max)?;
    request.top_p = real(body, "top_p", 0.0..=1.0)?;
    let key = if present(body, "stop").is_some() {
        "stop"
    } else {
        "stop_sequences"
    };
    request.stop_sequences = match present(body, key) {
        None => None,
        Some(Value::String(stop)) => Some(vec![stop.clone()]),
        Some(Value::Array(stops)) => Some(
            stops
                .iter()
                .enumerate()
                .map(|(at, stop)| {
                    stop.as_str().map(str::to_owned).ok_or_else(|| {
                        BadRequest(format!(
                            "`{key}[{at}]` must be a string, got {}",
                            kind(stop)
                        ))
                    })
                })
                .collect::<Result<_, _>>()?,
        ),
        Some(other) => {
            return Err(BadRequest(format!(
                "`{key}` must be a string or an array of strings, got {}",
                kind(other)
            )))
        }
    };
    Ok(())
}

/// The agent's `tools`, each entry read by the format's `one`. Any entry it cannot read (a
/// malformed one, or a type this gateway does not serve) refuses the whole call, naming the
/// entry: a tool the agent declared and the model never sees is a silent wrong answer.
pub(crate) fn tools(
    body: &Value,
    one: impl Fn(&Value) -> Result<NativeToolSpec, String>,
) -> Result<Option<Vec<NativeToolSpec>>, BadRequest> {
    let Some(value) = present(body, "tools") else {
        return Ok(None);
    };
    let Value::Array(entries) = value else {
        return Err(BadRequest(format!(
            "`tools` must be an array, got {}",
            kind(value)
        )));
    };
    entries
        .iter()
        .enumerate()
        .map(|(at, entry)| one(entry).map_err(|why| BadRequest(format!("`tools[{at}]` {why}"))))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// A tool's type, which a format's adapter matches against the types it serves.
pub(crate) fn tool_type(entry: &Value) -> Result<Option<&str>, String> {
    match entry.get("type") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(kind)) => Ok(Some(kind.as_str())),
        Some(other) => Err(format!(
            "has a `type` that is {}, not a string",
            kind(other)
        )),
    }
}

/// The refusal for a tool type this gateway does not serve: its tools are the agent's own
/// functions, never a provider's built-ins (web search, a hosted shell).
pub(crate) fn unserved(kind: &str, served: &str) -> String {
    format!("has type `{kind}`, which this gateway does not serve (it serves {served} tools: the agent's own functions)")
}

/// One tool from its parts as every format names them: a name, an optional description,
/// and an optional JSON-schema object for its input (no schema is a tool with no input).
pub(crate) fn tool_spec(
    name: Option<&Value>,
    description: Option<&Value>,
    schema: Option<&Value>,
) -> Result<NativeToolSpec, String> {
    let name = match name {
        Some(Value::String(name)) if !name.is_empty() => name.clone(),
        Some(Value::String(_)) => return Err("has an empty `name`".into()),
        Some(other) if !other.is_null() => {
            return Err(format!(
                "has a `name` that is {}, not a string",
                kind(other)
            ))
        }
        _ => return Err("has no `name`".into()),
    };
    let description = match description {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(description)) => description.clone(),
        Some(other) => {
            return Err(format!(
                "has a `description` that is {}, not a string",
                kind(other)
            ))
        }
    };
    let input_schema = match schema {
        None | Some(Value::Null) => input_schema(&Map::new())?,
        Some(Value::Object(schema)) => input_schema(schema)?,
        Some(other) => {
            return Err(format!(
                "has an input schema that is {}, not an object",
                kind(other)
            ))
        }
    };
    Ok(NativeToolSpec {
        name,
        description,
        input_schema,
    })
}

/// A JSON-schema object as the engine's tool input schema, which is always an object.
fn input_schema(schema: &Map<String, Value>) -> Result<ToolInputSchema, String> {
    match schema.get("type") {
        None | Some(Value::Null) => {}
        Some(Value::String(kind)) if kind == "object" => {}
        Some(other) => {
            return Err(format!(
                "has an input schema of type {other}; a tool's input is an object"
            ))
        }
    }
    let properties = match schema.get("properties") {
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(properties @ Value::Object(_)) => properties.clone(),
        Some(other) => {
            return Err(format!(
                "has schema `properties` that are {}, not an object",
                kind(other)
            ))
        }
    };
    let required = match schema.get("required") {
        None | Some(Value::Null) => None,
        Some(Value::Array(names)) => Some(
            names
                .iter()
                .map(|name| {
                    name.as_str().map(str::to_owned).ok_or_else(|| {
                        format!(
                            "has a schema `required` entry that is {}, not a name",
                            kind(name)
                        )
                    })
                })
                .collect::<Result<_, _>>()?,
        ),
        Some(other) => {
            return Err(format!(
                "has schema `required` that is {}, not an array",
                kind(other)
            ))
        }
    };
    Ok(ToolInputSchema {
        schema_type: "object".into(),
        properties,
        required,
        definitions: schema
            .get("$defs")
            .or_else(|| schema.get("definitions"))
            .cloned(),
    })
}

/// A tool choice the agent named: a mode from `modes` (each API's own vocabulary, mapped to
/// the engine's), or one tool by `name`. Anything else refuses the call.
pub(crate) fn choice_mode(mode: &str, modes: &[(&str, &str)]) -> Result<ToolChoice, BadRequest> {
    modes
        .iter()
        .find(|(said, _)| *said == mode)
        .map(|(_, engine)| ToolChoice::Mode((*engine).to_owned()))
        .ok_or_else(|| {
            let known: Vec<&str> = modes.iter().map(|(said, _)| *said).collect();
            BadRequest(format!(
                "`tool_choice` `{mode}` is not one of {}",
                known.join(", ")
            ))
        })
}

/// A tool choice that names one tool.
pub(crate) fn choice_named(name: Option<&Value>) -> Result<ToolChoice, BadRequest> {
    match name {
        Some(Value::String(name)) if !name.is_empty() => {
            Ok(ToolChoice::Specific { name: name.clone() })
        }
        _ => Err(BadRequest(
            "`tool_choice` names a tool without a `name`".into(),
        )),
    }
}
