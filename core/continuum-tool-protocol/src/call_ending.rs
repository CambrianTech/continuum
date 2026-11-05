//! How a model's tool call ENDED, decided once at the engine boundary (ONE-ACT 6b).
//!
//! The engine is the only seam that knows both facts a call's fate needs: whether its arguments
//! parsed, and why the generation stopped. Two of the four cells are failures, and they are not
//! the same failure:
//!
//! | ended at the output limit | arguments parse | the call is                          |
//! |---------------------------|-----------------|--------------------------------------|
//! | yes                       | no              | [`CallEnding::Cut`]: the limit cut the JSON |
//! | yes                       | yes             | [`CallEnding::Cut`]: complete JSON, but nothing proves its framing closed, so it is never executed |
//! | no                        | no              | [`CallEnding::Malformed`]: she wrote bad JSON |
//! | no                        | yes             | [`CallEnding::Committed`]            |
//!
//! A cut is never "retry": retrying re-cuts at the same limit (83 consecutive cuts on the M5).
//! Its way out is more room or a continuation. A malformed call's way out IS a retry. So the
//! engine carries the cause typed in the call (never a JSON EOF read later as a guess), and the
//! act seam (`contribute.rs`) and her hands (the executor) read this one classifier.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ToolCall;

/// Why the generation that carried a call stopped, as far as a call's fate is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationEnd {
    /// The output limit stopped it (`finish_reason: length`).
    OutputLimit,
    /// Anything else: the model stopped, called a tool, errored, or the backend did not say.
    Finished,
}

/// A call's arguments the engine could not hand on as a params object, with the cause typed.
/// Rides in [`ToolCall::input`] under [`UnparsedArguments::MARKER`], because the call itself still
/// has to reach the act seam to be reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UnparsedArguments {
    /// The output limit ended the generation inside the call's JSON.
    Cut { raw: String },
    /// The generation finished, and the arguments it wrote are not JSON.
    Malformed { parse_error: String, raw: String },
}

impl UnparsedArguments {
    /// The key the marker rides under in a call's input: one owner, written by the engine and
    /// read by [`UnparsedArguments::of`].
    pub const MARKER: &'static str = "__unparsed_tool_arguments";

    /// Parse a call's raw argument text, or say typed why it is not a params object.
    pub fn parse(raw: String, end: GenerationEnd) -> Result<Value, UnparsedArguments> {
        match serde_json::from_str(&raw) {
            Ok(value) => Ok(value),
            Err(_) if end == GenerationEnd::OutputLimit => Err(UnparsedArguments::Cut { raw }),
            Err(error) => Err(UnparsedArguments::Malformed { parse_error: error.to_string(), raw }),
        }
    }

    /// The input a call carries when its arguments did not parse.
    pub fn into_input(self) -> Value {
        let mut input = serde_json::Map::new();
        input.insert(
            Self::MARKER.to_string(),
            serde_json::to_value(self).expect("a pair of strings always serializes"), // JUSTIFIED: two String fields under a unit tag cannot fail to serialize
        );
        Value::Object(input)
    }

    /// The marker in a call's input, if the engine put one there.
    pub fn of(input: &Value) -> Option<UnparsedArguments> {
        input.get(Self::MARKER).and_then(|marker| UnparsedArguments::deserialize(marker).ok())
    }

    /// The raw argument text, whole.
    pub fn raw(&self) -> &str {
        match self {
            UnparsedArguments::Cut { raw } | UnparsedArguments::Malformed { raw, .. } => raw,
        }
    }
}

/// The fate of one call: the one classifier every seam reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallEnding {
    /// Complete arguments, and the generation was not stopped by the limit: it may run.
    Committed,
    /// The output limit ended the generation while the call was being written: it never runs.
    Cut,
    /// The generation finished, and the arguments are not JSON: it never runs; she retries.
    Malformed,
}

impl ToolCall {
    /// How this call ended, given why its generation stopped.
    pub fn ending(&self, end: GenerationEnd) -> CallEnding {
        match (UnparsedArguments::of(&self.input), end) {
            (Some(UnparsedArguments::Malformed { .. }), GenerationEnd::Finished) => CallEnding::Malformed,
            (Some(_), _) | (None, GenerationEnd::OutputLimit) => CallEnding::Cut,
            (None, GenerationEnd::Finished) => CallEnding::Committed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(raw: &str, end: GenerationEnd) -> ToolCall {
        let input = UnparsedArguments::parse(raw.to_string(), end).unwrap_or_else(UnparsedArguments::into_input);
        ToolCall { id: "t".into(), name: "code/edit".into(), input }
    }

    // what this catches: the four cells of a call's fate collapsing into fewer. A cut read as
    // malformed sends her to retry into the same limit; complete JSON at the limit run as if
    // committed executes a call whose framing never closed; bad JSON from a finished generation
    // read as a cut hides her own mistake.
    #[test]
    fn each_cell_of_parse_by_finish_is_its_own_ending() {
        let complete = r#"{"file_path":"a.rs"}"#;
        let broken = r#"{"file_path":"a.rs","content":"fn ma"#;
        assert_eq!(call(broken, GenerationEnd::OutputLimit).ending(GenerationEnd::OutputLimit), CallEnding::Cut);
        assert_eq!(call(complete, GenerationEnd::OutputLimit).ending(GenerationEnd::OutputLimit), CallEnding::Cut);
        assert_eq!(call(broken, GenerationEnd::Finished).ending(GenerationEnd::Finished), CallEnding::Malformed);
        assert_eq!(call(complete, GenerationEnd::Finished).ending(GenerationEnd::Finished), CallEnding::Committed);
    }

    // what this catches: the marker losing its typed cause or its raw text on the way through the
    // call's input, so the reader would have to guess from a parse error string.
    #[test]
    fn the_marker_carries_its_cause_and_the_whole_raw_text() {
        let raw = r#"{"content":"x"#;
        let input = UnparsedArguments::parse(raw.to_string(), GenerationEnd::OutputLimit).unwrap_err().into_input();
        assert_eq!(UnparsedArguments::of(&input), Some(UnparsedArguments::Cut { raw: raw.to_string() }));
        let input = UnparsedArguments::parse(raw.to_string(), GenerationEnd::Finished).unwrap_err().into_input();
        match UnparsedArguments::of(&input) {
            Some(UnparsedArguments::Malformed { parse_error, raw: kept }) => {
                assert!(!parse_error.is_empty());
                assert_eq!(kept, raw);
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
        assert_eq!(UnparsedArguments::of(&serde_json::json!({"file_path": "a.rs"})), None);
    }
}
