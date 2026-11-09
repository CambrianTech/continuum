//! How a model's tool call ENDED, decided once at the engine boundary (ONE-ACT 6b).
//!
//! The engine is the only seam that knows both facts a call's fate needs: whether its arguments
//! parsed, and why the generation stopped. Only a generation the engine reported FINISHED commits
//! a call; every other end holds it, and each hold is its own failure:
//!
//! | why it stopped (engine)   | arguments parse | the call is                          |
//! |---------------------------|-----------------|--------------------------------------|
//! | finished (stop, tool use) | yes             | [`CallEnding::Committed`]            |
//! | finished                  | no              | [`CallEnding::Malformed`]: she wrote bad JSON; she retries |
//! | the output limit          | either          | [`CallEnding::Cut`]: the limit cut it, or nothing proves its framing closed; never a retry |
//! | the generation errored    | either          | [`CallEnding::Failed`]: no completion receipt |
//! | not said by the engine    | either          | [`CallEnding::Unreported`]: neither cut nor complete can be known |
//!
//! A cut is never "retry": retrying re-cuts at the same limit (83 consecutive cuts on the M5).
//! The engine carries the cause typed in the call (never a JSON EOF read later as a guess), and
//! the disposition travels with the batch (`call_batch::CallBatch`), so nothing downstream runs
//! a call that was not committed.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ToolCall;

/// Why the generation that carried a call stopped, as far as a call's fate is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationEnd {
    /// The output limit stopped it (`finish_reason: length`).
    OutputLimit,
    /// The engine reported a completed generation: the model stopped, or called a tool.
    Finished,
    /// The engine reported an error ending the generation: no completion receipt.
    Failed,
    /// The engine did not say why it stopped. Never read as finished: a backend that drops the
    /// marker on a cut call would otherwise turn the cut into a retry (the re-cut loop) or run
    /// a call whose framing never closed.
    Unreported,
}

/// A call's arguments the engine could not hand on as a params object to run, with the cause
/// typed. Rides in [`ToolCall::input`] under [`UnparsedArguments::MARKER`], because the call itself
/// still has to reach the act seam to be reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UnparsedArguments {
    /// The output limit ended the generation inside the call's JSON.
    Cut { raw: String },
    /// The generation finished, and the arguments it wrote are not JSON.
    Malformed { parse_error: String, raw: String },
    /// The generation errored, so the call has no completion receipt (parsed or not).
    Errored { raw: String },
    /// The engine did not say why the generation stopped, so the call cannot be known complete
    /// (parsed or not): a transport defect, held rather than run.
    Unexplained { raw: String },
}

/// A marker the engine wrote that this build cannot read (a renamed tag, version skew). Never
/// read as "no marker": the call would run with the marker as its params.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableMarker {
    pub parse_error: String,
}

impl UnparsedArguments {
    /// The key the marker rides under in a call's input: one owner, written by the engine and
    /// read by [`UnparsedArguments::of`].
    pub const MARKER: &'static str = "__unparsed_tool_arguments";

    /// Parse a call's raw argument text, or say typed why it is not a params object to run.
    pub fn parse(raw: String, end: GenerationEnd) -> Result<Value, UnparsedArguments> {
        match end {
            GenerationEnd::Unreported => return Err(UnparsedArguments::Unexplained { raw }),
            GenerationEnd::Failed => return Err(UnparsedArguments::Errored { raw }),
            GenerationEnd::OutputLimit | GenerationEnd::Finished => {}
        }
        match serde_json::from_str(&raw) {
            Ok(value) => Ok(value),
            Err(_) if end == GenerationEnd::OutputLimit => Err(UnparsedArguments::Cut { raw }),
            Err(error) => Err(UnparsedArguments::Malformed { parse_error: error.to_string(), raw }),
        }
    }

    /// The input a call carries when its arguments are not a params object to run.
    pub fn into_input(self) -> Value {
        let mut input = serde_json::Map::new();
        input.insert(
            Self::MARKER.to_string(),
            serde_json::to_value(self).expect("a tagged pair of strings always serializes"), // JUSTIFIED: string fields under a unit tag cannot fail to serialize
        );
        Value::Object(input)
    }

    /// The marker in a call's input, if the engine put one there, or why it cannot be read.
    pub fn of(input: &Value) -> Result<Option<UnparsedArguments>, UnreadableMarker> {
        input
            .get(Self::MARKER)
            .map(|marker| UnparsedArguments::deserialize(marker).map_err(|e| UnreadableMarker { parse_error: e.to_string() }))
            .transpose()
    }

    /// The raw argument text, whole.
    pub fn raw(&self) -> &str {
        match self {
            UnparsedArguments::Cut { raw }
            | UnparsedArguments::Malformed { raw, .. }
            | UnparsedArguments::Errored { raw }
            | UnparsedArguments::Unexplained { raw } => raw,
        }
    }
}

/// The fate of one call: the one classifier every seam reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallEnding {
    /// Complete arguments from a generation the engine reported finished: it may run.
    Committed,
    /// The output limit ended the generation while the call was being written: it never runs.
    Cut,
    /// A finished generation wrote arguments that are not JSON (or carry a marker this build
    /// cannot read): it never runs; she retries.
    Malformed,
    /// The generation errored: it never runs.
    Failed,
    /// The engine did not say why the generation stopped: it never runs.
    Unreported,
}

impl ToolCall {
    /// How this call ended, given why its generation stopped.
    pub fn ending(&self, end: GenerationEnd) -> CallEnding {
        match (end, UnparsedArguments::of(&self.input)) {
            (GenerationEnd::OutputLimit, _) | (_, Ok(Some(UnparsedArguments::Cut { .. }))) => CallEnding::Cut,
            (GenerationEnd::Failed, _) | (_, Ok(Some(UnparsedArguments::Errored { .. }))) => CallEnding::Failed,
            (GenerationEnd::Unreported, _) | (_, Ok(Some(UnparsedArguments::Unexplained { .. }))) => CallEnding::Unreported,
            (GenerationEnd::Finished, Ok(Some(UnparsedArguments::Malformed { .. })) | Err(_)) => CallEnding::Malformed,
            (GenerationEnd::Finished, Ok(None)) => CallEnding::Committed,
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

    const COMPLETE: &str = r#"{"file_path":"a.rs"}"#;
    const BROKEN: &str = r#"{"file_path":"a.rs","content":"fn ma"#;

    // what this catches: the cells of a call's fate collapsing into fewer. A cut read as
    // malformed sends her to retry into the same limit; complete JSON at the limit run as if
    // committed executes a call whose framing never closed; bad JSON from a finished generation
    // read as a cut hides her own mistake.
    #[test]
    fn each_cell_of_parse_by_finish_is_its_own_ending() {
        assert_eq!(call(BROKEN, GenerationEnd::OutputLimit).ending(GenerationEnd::OutputLimit), CallEnding::Cut);
        assert_eq!(call(COMPLETE, GenerationEnd::OutputLimit).ending(GenerationEnd::OutputLimit), CallEnding::Cut);
        assert_eq!(call(BROKEN, GenerationEnd::Finished).ending(GenerationEnd::Finished), CallEnding::Malformed);
        assert_eq!(call(COMPLETE, GenerationEnd::Finished).ending(GenerationEnd::Finished), CallEnding::Committed);
    }

    // what this catches: a finish the engine never reported as complete (no marker, or an error)
    // read as finished. Its cut JSON would become Malformed (a retry into the same limit) and its
    // complete-but-unframed call would run. Both hold, parsed or not, with the raw text kept.
    #[test]
    fn an_unreported_or_errored_finish_never_commits_parsed_or_not() {
        for (end, held) in [(GenerationEnd::Unreported, CallEnding::Unreported), (GenerationEnd::Failed, CallEnding::Failed)] {
            for raw in [COMPLETE, BROKEN] {
                let c = call(raw, end);
                assert_eq!(c.ending(end), held, "{end:?} {raw}");
                assert_eq!(UnparsedArguments::of(&c.input).ok().flatten().map(|u| u.raw().to_string()), Some(raw.to_string()));
            }
            // A call another adapter handed on parsed is still held.
            let parsed = ToolCall { id: "t".into(), name: "code/edit".into(), input: serde_json::json!({"file_path": "a.rs"}) };
            assert_eq!(parsed.ending(end), held);
        }
    }

    // what this catches: a marker this build cannot read (a renamed tag) read as no marker, so
    // the call runs with the marker as its params and is refused for the wrong cause.
    #[test]
    fn an_unreadable_marker_is_malformed_never_absent() {
        let input = serde_json::json!({ UnparsedArguments::MARKER: { "kind": "renamed_in_a_later_build", "raw": "{" } });
        assert!(UnparsedArguments::of(&input).is_err());
        let call = ToolCall { id: "t".into(), name: "code/edit".into(), input };
        assert_eq!(call.ending(GenerationEnd::Finished), CallEnding::Malformed);
    }

    // what this catches: the marker losing its typed cause or its raw text on the way through the
    // call's input, so the reader would have to guess from a parse error string.
    #[test]
    fn the_marker_carries_its_cause_and_the_whole_raw_text() {
        let input = UnparsedArguments::parse(BROKEN.to_string(), GenerationEnd::OutputLimit).unwrap_err().into_input();
        assert_eq!(UnparsedArguments::of(&input), Ok(Some(UnparsedArguments::Cut { raw: BROKEN.to_string() })));
        let input = UnparsedArguments::parse(BROKEN.to_string(), GenerationEnd::Finished).unwrap_err().into_input();
        match UnparsedArguments::of(&input) {
            Ok(Some(UnparsedArguments::Malformed { parse_error, raw })) => {
                assert!(!parse_error.is_empty());
                assert_eq!(raw, BROKEN);
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
        assert_eq!(UnparsedArguments::of(&serde_json::json!({"file_path": "a.rs"})), Ok(None));
    }
}
