//! A refused command as it crosses a seam: in-process from the executor to the client a mind
//! calls through, and over the grid on [`AircCommandResponse::Error`](crate::AircCommandResponse).
//!
//! The kernel builds a refusal only with its next moves (`sdk_codegen::Refusal`, each call from a
//! verb in her hands with that verb's own params) and projects it onto [`CommandRefusal`] here,
//! which is plain data: the client below the kernel and a peer on another node both read it, and
//! the mind's renderer turns each [`SuggestedCall`] into a call on the surface of her act.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What kind of refusal it is: the classification a client or log reads without parsing prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/transport/RefusalCategory.ts")]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// The call decoded but failed a check of the request or a precondition of the domain.
    Invalid,
    /// What the call names (an output, a file, a card) does not exist.
    NotFound,
    /// The caller may not do this.
    Denied,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Invalid => "invalid",
            Category::NotFound => "not_found",
            Category::Denied => "denied",
        }
    }
}

/// Why a refusal has no call that would make progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/transport/NoMove.ts")]
#[serde(rename_all = "snake_case")]
pub enum NoMove {
    /// What she asked for no longer exists anywhere: what she was shown of it is all there is.
    NothingMoreExists,
    /// Nothing of hers is saved yet, so there is nothing to reopen.
    NothingSaved,
}

impl std::fmt::Display for NoMove {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoMove::NothingMoreExists => f.write_str("no call recovers it; what you were shown is all there is"),
            NoMove::NothingSaved => f.write_str("none of your acts has a saved output yet"),
        }
    }
}

/// A call that would make progress: the command by its canonical name (her renderer resolves it
/// through the command's aliases to the name her tool list offers), its arguments as an object of
/// the command's own params, and why it would help.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/transport/SuggestedCall.ts")]
pub struct SuggestedCall {
    pub command: String,
    #[ts(type = "Record<string, unknown>")]
    pub args: serde_json::Map<String, serde_json::Value>,
    pub why: String,
}

/// What she can do after a refusal: one or more calls, or the typed reason there is none. An
/// empty list of calls is not a value of this type: it fails to decode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/transport/NextMoves.ts")]
#[serde(rename_all = "snake_case", try_from = "NextMovesWire")]
pub enum NextMoves {
    Calls(Vec<SuggestedCall>),
    None(NoMove),
}

impl NextMoves {
    /// At least one call; `None` for an empty list, which says nothing she can do.
    pub fn calls(first: SuggestedCall, more: Vec<SuggestedCall>) -> Self {
        let mut calls = Vec::with_capacity(1 + more.len());
        calls.push(first);
        calls.extend(more);
        NextMoves::Calls(calls)
    }
}

/// The decoded shape before the non-empty check.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum NextMovesWire {
    Calls(Vec<SuggestedCall>),
    None(NoMove),
}

/// A decoded list of next calls that was empty.
#[derive(Debug)]
pub struct NoCallsListed;

impl std::fmt::Display for NoCallsListed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a refusal's next calls must name at least one call; with none it says no_move")
    }
}

impl TryFrom<NextMovesWire> for NextMoves {
    type Error = NoCallsListed;
    fn try_from(wire: NextMovesWire) -> Result<Self, Self::Error> {
        match wire {
            NextMovesWire::Calls(calls) if calls.is_empty() => Err(NoCallsListed),
            NextMovesWire::Calls(calls) => Ok(NextMoves::Calls(calls)),
            NextMovesWire::None(none) => Ok(NextMoves::None(none)),
        }
    }
}

/// A refused command: its category, its cause and the causes beneath it (outermost first), and
/// her next moves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/transport/CommandRefusal.ts")]
pub struct CommandRefusal {
    pub category: Category,
    pub cause: Vec<String>,
    pub next: NextMoves,
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a refusal arriving over the grid with an empty call list, which would
    // reach her as "do one of these:" followed by nothing. It fails to decode instead.
    #[test]
    fn an_empty_list_of_next_calls_does_not_decode() {
        let empty = serde_json::json!({ "category": "not_found", "cause": ["gone"], "next": { "calls": [] } });
        assert!(serde_json::from_value::<CommandRefusal>(empty).is_err());
        let none = serde_json::json!({ "category": "not_found", "cause": ["gone"], "next": { "none": "nothing_more_exists" } });
        assert!(matches!(
            serde_json::from_value::<CommandRefusal>(none),
            Ok(CommandRefusal { next: NextMoves::None(NoMove::NothingMoreExists), .. })
        ));
    }
}
