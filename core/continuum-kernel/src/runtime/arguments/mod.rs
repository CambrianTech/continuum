//! A tool call's arguments are checked against the command's own `Params` BEFORE it runs, and a
//! failure is a typed [`ArgumentsRefusal`] naming the field and what it needed (ONE-ACT 6b, item
//! 5). One decoder decides it ([`decode`]): the same one the dispatch uses, so what is refused
//! before dispatch is exactly what dispatch would refuse.
//!
//! Why typed: the refusal used to be serde_json's message string plus a "you sent ..." sentence
//! built by reading "missing field `x`" back out of that string, and her executor told an
//! arguments refusal from any other by the `[arguments]` prefix on the rendered text. Here the
//! field, the path and the problem arrive as data (serde's own `de::Error` calls, through
//! `json_args::ArgError`), and a reader matches variants.
//!
//! The shape is the arguments arm of the one tool refusal (Cormac's `<Tool>Refusal`, card
//! 3d7931b7): its next move is a retry of the same command with the fields fixed, except a
//! [`UnparsedArguments::Cut`], whose way out is never a retry.

pub mod accepted_shape;
mod json_args;
#[cfg(test)]
mod tests;

use airc_core::shown_id::ShownIdError;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub use continuum_tool_protocol::call_ending::{UnparsedArguments, UnreadableMarker};

use json_args::{ArgError, JsonArgs};

/// What a call's arguments did not satisfy. Nothing ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentsRefusal {
    /// The engine could not hand the arguments on as an object at all: the output limit cut them,
    /// or the model wrote text that is not JSON. The raw text is kept whole.
    Unparsed(UnparsedArguments),
    /// The engine marked the call's arguments in a form this build cannot read (a renamed tag,
    /// version skew): refused as malformed, never run with the marker as its params.
    UnreadableMarker(UnreadableMarker),
    /// The arguments are JSON, but not an object of named fields.
    NotAnObject { sent: JsonKind },
    /// One field did not decode as the command's own type for it.
    Field(FieldRefusal),
    /// A field held a whole tool call (`{"tool_call": {"name", "arguments"}}`) naming no command
    /// in her hands. Her next call is that inner call, sent as a call of its own with a real name.
    WrappedCall { property: String, name: String, arguments: String },
}

/// Which field, and what was wrong with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRefusal {
    pub path: FieldPath,
    pub problem: FieldProblem,
}

/// Where a field sits in the arguments: `edit_mode.search`, `files[2]`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FieldPath(pub Vec<Segment>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Key(String),
    Index(usize),
    Variant(String),
}

/// What a field needed that it did not get.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldProblem {
    /// Required and absent. `sent` is every key she sent beside it (at the root, never the
    /// envelope's own); `sent_as` is the one of them that reads as this field under another name
    /// (`path` for `file_path`, `cmd` for `command`). `sent` is `None` when the refuser is not
    /// the decoder and does not know her keys (a handler's own check of an envelope field).
    Missing { sent: Option<Vec<String>>, sent_as: Option<String> },
    /// Present with the wrong JSON type. `expected` is serde's rendering of the field's type.
    WrongType { expected: String, sent: JsonKind },
    /// The right JSON type, outside what the field accepts.
    WrongValue { expected: String, sent: JsonKind },
    /// A list or tuple of the wrong length.
    WrongLength { len: usize, expected: String },
    /// A tag or variant name the field's enum does not have.
    UnknownVariant { variant: String, expected: &'static [&'static str] },
    /// A key the command does not take (only commands that deny unknown keys say this).
    UnknownField { expected: &'static [&'static str] },
    /// The same key twice.
    Duplicate,
    /// An id that names no candidate, or more than one: airc's own typed refusal, with the
    /// candidates she can choose from.
    Id(ShownIdError),
    /// The field's own `Deserialize` refused it through serde's `custom` channel, which carries
    /// only a message (serde's API, not ours). A field type with a typed error belongs in a
    /// variant of its own, like [`FieldProblem::Id`].
    Decoder(DecoderSaid),
}

/// The text a field type's own `Deserialize` gave serde's `custom` channel: rendered to her, never
/// matched (it has no accessor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecoderSaid(String);

/// The JSON type of what she sent, read from the value at the refused path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JsonKind {
    Null,
    Bool,
    Number,
    String,
    Array,
    Object,
    /// Nothing at that path (serde refused a value inside one it buffered, such as an
    /// internally tagged enum, and reported only the outer path).
    Absent,
}

impl JsonKind {
    pub fn of(value: &Value) -> Self {
        match value {
            Value::Null => JsonKind::Null,
            Value::Bool(_) => JsonKind::Bool,
            Value::Number(_) => JsonKind::Number,
            Value::String(_) => JsonKind::String,
            Value::Array(_) => JsonKind::Array,
            Value::Object(_) => JsonKind::Object,
        }
    }

    fn noun(self) -> &'static str {
        match self {
            JsonKind::Null => "null",
            JsonKind::Bool => "a boolean",
            JsonKind::Number => "a number",
            JsonKind::String => "a string",
            JsonKind::Array => "a list",
            JsonKind::Object => "an object",
            JsonKind::Absent => "nothing there",
        }
    }
}

impl ArgumentsRefusal {
    /// A refusal of one top-level field, for a handler that checks a field past its type.
    pub fn field(name: &str, problem: FieldProblem) -> Self {
        ArgumentsRefusal::Field(FieldRefusal { path: FieldPath(vec![Segment::Key(name.to_string())]), problem })
    }

    /// A field that should hold an id of `label`'s kind and does not parse as one: airc's own
    /// typed refusal, so an id field reads the same everywhere it is checked.
    pub fn not_an_id(name: &str, raw: &str, label: &'static str) -> Self {
        Self::id(name, ShownIdError::NotAnId { raw: raw.to_string(), label })
    }

    /// A field whose id named no candidate, or more than one: airc's refusal kept whole, so the
    /// candidates she can choose from stay typed.
    pub fn id(name: &str, error: ShownIdError) -> Self {
        Self::field(name, FieldProblem::Id(error))
    }

    /// Whether her next move is the same command again with the arguments fixed. A cut is not:
    /// the same call cuts at the same limit. Nor is a call whose generation errored or never said
    /// how it ended: the fault is the engine's, and no change to her call answers it.
    pub fn retries(&self) -> bool {
        !matches!(
            self,
            ArgumentsRefusal::Unparsed(UnparsedArguments::Cut { .. } | UnparsedArguments::Errored { .. } | UnparsedArguments::Unexplained { .. })
        )
    }
}

impl FieldPath {
    fn at<'v>(&self, root: &'v Value) -> Option<&'v Value> {
        self.0.iter().try_fold(root, |value, segment| match segment {
            Segment::Key(key) => value.get(key),
            Segment::Index(index) => value.get(index),
            Segment::Variant(_) => Some(value),
        })
    }
}

impl std::fmt::Display for FieldPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for segment in &self.0 {
            match segment {
                Segment::Key(key) if first => write!(f, "{key}")?,
                Segment::Key(key) => write!(f, ".{key}")?,
                Segment::Index(index) => write!(f, "[{index}]")?,
                Segment::Variant(_) => continue,
            }
            first = false;
        }
        Ok(())
    }
}

impl std::fmt::Display for ArgumentsRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgumentsRefusal::Unparsed(UnparsedArguments::Cut { raw }) => write!(
                f,
                "the output limit cut this call while its arguments were being written ({} bytes), \
                 so nothing ran. Sending the same call again cuts at the same place: commit a \
                 smaller call (only the change, never the whole file), or split it.",
                raw.len()
            ),
            ArgumentsRefusal::Unparsed(UnparsedArguments::Malformed { parse_error, raw }) => write!(
                f,
                "your arguments are not valid JSON ({parse_error}), so nothing ran. What you sent \
                 ({} bytes):\n{raw}",
                raw.len()
            ),
            ArgumentsRefusal::Unparsed(UnparsedArguments::Unexplained { raw }) => write!(
                f,
                "the engine did not say why your generation stopped, so this call ({} bytes) cannot \
                 be known complete, and nothing ran. This is a fault in the engine's transport, not \
                 in your call.",
                raw.len()
            ),
            ArgumentsRefusal::Unparsed(UnparsedArguments::Errored { raw }) => write!(
                f,
                "the engine reported an error ending your generation, so this call ({} bytes) has no \
                 completion receipt, and nothing ran. The fault is the engine's, not your call's.",
                raw.len()
            ),
            ArgumentsRefusal::UnreadableMarker(UnreadableMarker { parse_error }) => write!(
                f,
                "the engine marked this call's arguments as unparsed in a form this build cannot read \
                 ({parse_error}), so nothing ran. Send the call again."
            ),
            ArgumentsRefusal::NotAnObject { sent } => {
                write!(f, "the arguments must be an object of named fields; you sent {}", sent.noun())
            }
            ArgumentsRefusal::Field(refusal) => write!(f, "{refusal}"),
            ArgumentsRefusal::WrappedCall { property, name, arguments } => write!(
                f,
                "`{property}` holds a whole tool call to `{name}`, and no command named `{name}` is in your \
                 hands, so nothing ran. A tool call is never an argument: your next call is {name}({arguments}), \
                 sent as a call of its own under a name from your tool list."
            ),
        }
    }
}

impl std::fmt::Display for FieldRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = &self.path;
        match &self.problem {
            FieldProblem::Missing { sent_as: Some(near), .. } => write!(
                f,
                "`{path}` is required. You sent `{near}`: this command calls that field `{path}`, not `{near}`. Re-send it as `{path}`."
            ),
            FieldProblem::Missing { sent: None, sent_as: None } => write!(f, "`{path}` is required and was not sent."),
            FieldProblem::Missing { sent: Some(sent), sent_as: None } if sent.is_empty() => {
                write!(f, "`{path}` is required, and you sent no parameters at all.")
            }
            FieldProblem::Missing { sent: Some(sent), sent_as: None } => {
                let sent: Vec<&str> = sent.iter().map(String::as_str).collect();
                write!(f, "`{path}` is required. You sent {}, but not `{path}`.", quoted(&sent))
            }
            FieldProblem::WrongType { expected, sent } => write!(f, "`{path}` must be {expected}; you sent {}.", sent.noun()),
            FieldProblem::WrongValue { expected, sent } => {
                write!(f, "`{path}` must be {expected}; the {} you sent is not one.", sent.noun().trim_start_matches("a ").trim_start_matches("an "))
            }
            FieldProblem::WrongLength { len, expected } => write!(f, "`{path}` must be {expected}; you sent {len} items."),
            FieldProblem::UnknownVariant { variant, expected } => {
                write!(f, "`{path}` is `{variant}`, which is not one of: {}.", quoted(expected))
            }
            FieldProblem::UnknownField { expected } => {
                write!(f, "this command takes no field `{path}`. Its fields are: {}.", quoted(expected))
            }
            FieldProblem::Duplicate => write!(f, "`{path}` was sent twice."),
            FieldProblem::Id(error) => write!(f, "`{path}`: {error}"),
            FieldProblem::Decoder(DecoderSaid(said)) => write!(f, "`{path}` did not decode: {said}."),
        }
    }
}

/// Callers that still speak `Result<_, String>` (a `ServiceModule` arm) `?` a refusal straight
/// through; its rendering is the text they carry.
impl From<ArgumentsRefusal> for String {
    fn from(refusal: ArgumentsRefusal) -> String {
        refusal.to_string()
    }
}

fn quoted(names: &[&str]) -> String {
    names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
}

/// Decode `T` from `value`, reading only the root keys `keep` admits, or refuse typed.
pub fn decode<T: DeserializeOwned>(value: &Value, keep: fn(&str) -> bool) -> Result<T, ArgumentsRefusal> {
    serde_path_to_error::deserialize(JsonArgs::keeping(value, keep)).map_err(|error| {
        let mut path = FieldPath(
            error
                .path()
                .iter()
                .filter_map(|segment| match segment {
                    serde_path_to_error::Segment::Map { key } => Some(Segment::Key(key.clone())),
                    serde_path_to_error::Segment::Seq { index } => Some(Segment::Index(*index)),
                    serde_path_to_error::Segment::Enum { variant } => Some(Segment::Variant(variant.clone())),
                    serde_path_to_error::Segment::Unknown => None,
                })
                .collect(),
        );
        let sent = |path: &FieldPath| path.at(value).map_or(JsonKind::Absent, JsonKind::of);
        let problem = match error.into_inner() {
            ArgError::Missing(field) => {
                let sent: Vec<String> = path
                    .at(value)
                    .and_then(Value::as_object)
                    .map(|holder| holder.keys().filter(|k| !path.0.is_empty() || keep(k)).cloned().collect())
                    .unwrap_or_default(); // unwrap_or_default: a holder that is not an object sent no keys
                let sent_as = sent_as(field, &sent);
                path.0.push(Segment::Key(field.to_string()));
                FieldProblem::Missing { sent: Some(sent), sent_as }
            }
            ArgError::WrongType { expected } => FieldProblem::WrongType { expected, sent: sent(&path) },
            ArgError::WrongValue { expected } => FieldProblem::WrongValue { expected, sent: sent(&path) },
            ArgError::WrongLength { len, expected } => FieldProblem::WrongLength { len, expected },
            ArgError::UnknownVariant { variant, expected } => FieldProblem::UnknownVariant { variant, expected },
            ArgError::UnknownField { field, expected } => {
                path.0.push(Segment::Key(field));
                FieldProblem::UnknownField { expected }
            }
            ArgError::Duplicate(field) => {
                path.0.push(Segment::Key(field.to_string()));
                FieldProblem::Duplicate
            }
            ArgError::Decoder(said) => FieldProblem::Decoder(DecoderSaid(said)),
        };
        ArgumentsRefusal::Field(FieldRefusal { path, problem })
    })
}

/// The key she sent that most plausibly MEANT `wanted`.
///
/// Two kinships, because the real pairs come in two shapes: a QUALIFIER (`path` inside
/// `file_path`, a substring) and an ABBREVIATION (`cmd` in `command`, the letters in order). Both
/// stay tight enough to refuse unrelated names: a wrong "you meant X" would send her to rename
/// the wrong field.
fn sent_as(wanted: &str, sent: &[String]) -> Option<String> {
    let w = wanted.to_lowercase();
    sent.iter()
        .find(|k| {
            let k = k.to_lowercase();
            k.contains(&w) || w.contains(&k) || is_subsequence(&k, &w) || is_subsequence(&w, &k)
        })
        .cloned()
}

/// Is `short` an in-order subsequence of `long`: the abbreviation relation. At least 3
/// characters, so a 1-2 letter key never matches half the alphabet.
fn is_subsequence(short: &str, long: &str) -> bool {
    if short.len() < 3 || short.len() >= long.len() {
        return false;
    }
    let mut chars = long.chars();
    short.chars().all(|c| chars.any(|l| l == c))
}
