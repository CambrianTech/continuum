//! REFUSAL IS THE LAST RESORT (Joel, 2026-11-07: "Refusal is insane"; card 9ed04d88). A call whose
//! arguments do not decode is read once more against the command's OWN declared params schema
//! (the one its offer is generated from), and every reading that schema makes unambiguous is
//! taken before the decoder's refusal stands. One seam for every command, never per tool.
//!
//! Why: 67 argument refusals in one Windows window. Sahar sent `cmd: {"cmd": "Get-Content …"}`
//! to `code/shell` and `card_id: {…}` to `work/get` because the collapsed offer showed `{}` for
//! the field; Aiko had 32 refused results in 33 acts on the M5. Each meaning was plain.
//!
//! What is read, per top-level property whose declared type is ONE scalar (string, integer,
//! number or boolean; `Option` of one counts):
//! - an object of exactly one key, that key the property's own name: unwrapped (and its value
//!   read again as a scalar, below);
//! - an object of exactly one key under another name whose value already IS the declared kind:
//!   unwrapped;
//! - a string that is exactly a declared integer, number or boolean's canonical text: converted;
//! - a number or boolean where a string is declared: rendered.
//!
//! Anything with more than one plausible reading still refuses with the decoder's typed refusal:
//! an object of two or more keys, a one-key object holding an object or list, a string that only
//! loosely reads as a number (`"5 lines"`, `"+5"`, `"1e3"`), a number where a boolean is declared
//! (0/1 is a guess), a declared union of scalar types, a nested field, a list where a scalar is
//! declared, or a string holding JSON where an object is declared.
//!
//! A call that decodes as sent is never touched: the reading happens only after the decoder
//! refused. Each accepted reading is recorded (probe `dispatch.call.accepted_shape` carrying its
//! running count per (command, property, reading, sent kind)) so the offer is fixed at its source.

#[cfg(test)]
mod tests;
pub mod wrapped_call;

use std::borrow::Cow;
use std::sync::LazyLock;

use dashmap::DashMap;
use serde_json::{Map, Value};

use super::{ArgumentsRefusal, JsonKind};

/// The JSON scalar a property's schema declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scalar {
    String,
    Integer,
    Number,
    Boolean,
}

impl Scalar {
    fn of_type_name(name: &str) -> Option<Self> {
        match name {
            "string" => Some(Scalar::String),
            "integer" => Some(Scalar::Integer),
            "number" => Some(Scalar::Number),
            "boolean" => Some(Scalar::Boolean),
            _ => None,
        }
    }

    fn holds(self, value: &Value) -> bool {
        match (self, value) {
            (Scalar::String, Value::String(_)) | (Scalar::Boolean, Value::Bool(_)) | (Scalar::Number, Value::Number(_)) => true,
            (Scalar::Integer, Value::Number(n)) => n.is_i64() || n.is_u64(),
            _ => false,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Scalar::String => "string",
            Scalar::Integer => "integer",
            Scalar::Number => "number",
            Scalar::Boolean => "boolean",
        }
    }
}

/// How one property was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reading {
    /// `{"cmd": {"cmd": "ls"}}`: the value wrapped in an object keyed by its own name.
    WrappedInOwnName,
    /// `{"card_id": {"id": "08ece9e8"}}`: one key under another name, its value the declared kind.
    WrappedInOneKey,
    /// `"30000"` where an integer or number is declared, `"true"` where a boolean is.
    TextAsScalar,
    /// `5` or `true` where a string is declared.
    ScalarAsText,
}

impl Reading {
    pub fn as_str(self) -> &'static str {
        match self {
            Reading::WrappedInOwnName => "wrapped_in_own_name",
            Reading::WrappedInOneKey => "wrapped_in_one_key",
            Reading::TextAsScalar => "text_as_scalar",
            Reading::ScalarAsText => "scalar_as_text",
        }
    }
}

/// One accepted reading: which property, what she sent, what it was read as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedShape {
    pub property: String,
    pub reading: Reading,
    pub from: JsonKind,
    pub to: Scalar,
}

/// The arguments as read, and every reading taken to get there.
#[derive(Debug, Clone, PartialEq)]
pub struct Accepted {
    pub value: Value,
    pub shapes: Vec<AcceptedShape>,
}

/// Read `arguments` against `schema` (a command's declared params schema). `None` when no
/// property has an unambiguous reading. Pure.
pub fn read(schema: &Value, arguments: &Value) -> Option<Accepted> {
    let properties = schema.get("properties")?.as_object()?;
    let sent = arguments.as_object()?;
    let mut value: Option<Map<String, Value>> = None;
    let mut shapes = Vec::new();
    for (name, property_schema) in properties {
        let Some(scalar) = declared_scalar(property_schema, schema, false) else { continue };
        let Some(original) = sent.get(name) else { continue };
        let Some((read_as, mut read_shapes)) = read_property(name, scalar, original) else { continue };
        shapes.append(&mut read_shapes);
        value.get_or_insert_with(|| sent.clone()).insert(name.clone(), read_as);
    }
    value.map(|map| Accepted { value: Value::Object(map), shapes })
}

/// The one scalar a property schema declares, through `$ref`, a `["T", "null"]` type list, or an
/// `anyOf`/`oneOf` of one schema beside `null`. `None` for anything else (an object, a list, a
/// union of scalars): no reading is unambiguous there. One `$ref` hop is followed (schemars
/// emits one for a newtype); a reference to a reference is not a declared scalar, so a cycle ends.
fn declared_scalar(property: &Value, root: &Value, followed_ref: bool) -> Option<Scalar> {
    if let Some(reference) = property.get("$ref").and_then(Value::as_str) {
        if followed_ref {
            return None;
        }
        let name = reference.strip_prefix("#/definitions/").or_else(|| reference.strip_prefix("#/$defs/"))?;
        let target = root
            .get("definitions")
            .and_then(|d| d.get(name))
            .or_else(|| root.get("$defs").and_then(|d| d.get(name)))?;
        return declared_scalar(target, root, true);
    }
    match property.get("type") {
        Some(Value::String(name)) => return Scalar::of_type_name(name),
        Some(Value::Array(names)) => {
            let mut not_null = names.iter().filter_map(Value::as_str).filter(|n| *n != "null");
            return match (not_null.next(), not_null.next()) {
                (Some(only), None) => Scalar::of_type_name(only),
                _ => None,
            };
        }
        Some(_) => return None,
        None => {}
    }
    let union = property.get("anyOf").or_else(|| property.get("oneOf"))?.as_array()?;
    let mut not_null = union.iter().filter(|s| s.get("type").and_then(Value::as_str) != Some("null"));
    match (not_null.next(), not_null.next()) {
        (Some(only), None) => declared_scalar(only, root, followed_ref),
        _ => None,
    }
}

/// One property's reading, or `None` when it has none or more than one.
fn read_property(name: &str, scalar: Scalar, sent: &Value) -> Option<(Value, Vec<AcceptedShape>)> {
    if scalar.holds(sent) {
        return None;
    }
    let shape = |reading, from| AcceptedShape { property: name.to_string(), reading, from, to: scalar };
    if let Value::Object(wrapper) = sent {
        let (key, inner) = match (wrapper.len(), wrapper.iter().next()) {
            (1, Some(entry)) => entry,
            _ => return None,
        };
        let own_name = key == name;
        let unwrapped = shape(if own_name { Reading::WrappedInOwnName } else { Reading::WrappedInOneKey }, JsonKind::Object);
        if scalar.holds(inner) {
            return Some((inner.clone(), vec![unwrapped]));
        }
        // Under another name only the declared kind itself is unambiguous; under her own name the
        // value is hers, and a scalar inside reads exactly as one sent bare.
        if !own_name {
            return None;
        }
        let converted = convert(scalar, inner)?;
        return Some((converted, vec![unwrapped, shape(reading_of(scalar), JsonKind::of(inner))]));
    }
    let converted = convert(scalar, sent)?;
    Some((converted, vec![shape(reading_of(scalar), JsonKind::of(sent))]))
}

fn reading_of(scalar: Scalar) -> Reading {
    match scalar {
        Scalar::String => Reading::ScalarAsText,
        Scalar::Integer | Scalar::Number | Scalar::Boolean => Reading::TextAsScalar,
    }
}

/// A scalar of another kind read as `scalar`, only when exact: the canonical text of an integer,
/// a JSON number, or `true`/`false`; or a number or boolean rendered as text.
fn convert(scalar: Scalar, sent: &Value) -> Option<Value> {
    match (scalar, sent) {
        (Scalar::String, Value::Number(n)) => Some(Value::String(n.to_string())),
        (Scalar::String, Value::Bool(b)) => Some(Value::String(b.to_string())),
        (Scalar::Integer, Value::String(text)) => integer_text(text),
        (Scalar::Number, Value::String(text)) => number_text(text),
        (Scalar::Boolean, Value::String(text)) => match text.as_str() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        _ => None,
    }
}

/// `text` as an integer only when it is that integer's own rendering (no sign, padding or space).
fn integer_text(text: &str) -> Option<Value> {
    let value = match text.parse::<u64>() {
        Ok(unsigned) => Value::from(unsigned),
        Err(_) => Value::from(text.parse::<i64>().ok()?),
    };
    (value.to_string() == text).then_some(value)
}

/// `text` as a number only when it is a JSON number whose rendering is `text` itself.
fn number_text(text: &str) -> Option<Value> {
    match serde_json::from_str::<Value>(text) {
        Ok(value @ Value::Number(_)) if value.to_string() == text => Some(value),
        _ => None,
    }
}

/// Decode `arguments` with `decode`; when it refuses, take the readings `schema` makes
/// unambiguous and decode once more. A call that decodes as sent is returned borrowed and
/// untouched. A read call that still does not decode is refused with the refusal of what was
/// read: the field it names is the one still wrong, her next move.
pub fn decode_accepting<'a, T>(
    command: &'static str,
    schema: &Value,
    arguments: &'a Value,
    decode: impl Fn(&Value) -> Result<T, ArgumentsRefusal>,
) -> Result<(T, Cow<'a, Value>), ArgumentsRefusal> {
    let refusal = match decode(arguments) {
        Ok(decoded) => return Ok((decoded, Cow::Borrowed(arguments))),
        Err(refusal) => refusal,
    };
    let Some(accepted) = read(schema, arguments) else { return Err(refusal) };
    let decoded = decode(&accepted.value)?;
    record(command, &accepted.shapes);
    Ok((decoded, Cow::Owned(accepted.value)))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CountKey {
    command: &'static str,
    property: String,
    reading: Reading,
    from: JsonKind,
}

/// Accepted readings since boot, per (command, property, reading, sent kind): the number that
/// says which offer to fix first, carried on every probe row.
static ACCEPTED: LazyLock<DashMap<CountKey, u64>> = LazyLock::new(DashMap::new);

fn record(command: &'static str, shapes: &[AcceptedShape]) {
    for shape in shapes {
        let key = CountKey { command, property: shape.property.clone(), reading: shape.reading, from: shape.from };
        let count = {
            let mut entry = ACCEPTED.entry(key).or_default();
            *entry += 1;
            *entry
        };
        continuum_base::probe!(
            class = "dispatch.call.accepted_shape",
            command = command,
            property = shape.property.as_str(),
            reading = shape.reading.as_str(),
            from = shape.from.noun(),
            to = shape.to.as_str(),
            count,
            "a call's argument had one plain reading against the command's declared type: accepted, never refused"
        );
    }
}
