//! Why a generation failed — as a TYPE, decided once at the adapter seam.
//!
//! # The defect this exists to kill
//!
//! Every inference failure used to reach cognition as a `String`
//! (`Err(format!("… returned 400 Bad Request: {body}"))`). A caller that needed
//! to know *what kind* of failure it was had exactly one option: match on the
//! prose. Nobody did — so `cognition::act_observe::settle` treated EVERY fault
//! as transient and retried it blind.
//!
//! That is correct for a wedged backend (measured under #386: the next
//! generation succeeds ~2/3 of the time). It is catastrophic for a context
//! overflow, which is *deterministic*: the same prompt against the same slot
//! returns the same 400 forever. Measured live on 2026-08-13 — four distinct
//! overflows on one lane, each overshooting by a small margin, each retried to
//! exhaustion, each burning a whole turn:
//!
//! ```text
//! request (15697 tokens) exceeds the available context size (15104 tokens)
//! request (16751 tokens) exceeds the available context size (16384 tokens)
//! ```
//!
//! The server told us the kind (`"type":"exceed_context_size"`) AND both
//! numbers. We `format!`-ed them into prose and threw the structure away, then
//! could not act on what we had been handed.
//!
//! # The rule
//!
//! Parse the backend's error body **once**, here, at the boundary where the
//! wire shape is still known. Downstream matches on a variant. No consumer of
//! this type ever calls `.contains()` on an error message — if you find
//! yourself wanting to, the missing thing is a variant, and it belongs in this
//! file ([[the-same-bug-at-two-sites-is-a-missing-constraint]]).

use std::fmt;

/// A generation failure, classified at the adapter seam.
///
/// `Display` deliberately reproduces the operator-facing prose the untyped
/// `String` used to carry, so probes and receipts read identically after the
/// conversion — the type adds a decision surface without costing legibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceError {
    /// The prompt did not fit the slot the backend actually has.
    ///
    /// PERMANENT for an unchanged prompt: retrying verbatim cannot succeed.
    /// The caller's correct move is to re-render within `available` (minus its
    /// own completion reserve) and try again, or surrender loud if it is
    /// already minimal. `available` is the backend's OWN number and therefore
    /// outranks any window figure cached from a serving plan.
    ContextExceeded { requested: u32, available: u32 },

    /// The backend is not reachable / not up. Retry may succeed once it is.
    Unavailable(String),

    /// A fault with no better classification — the #386 case. Retry is
    /// reasonable and bounded.
    Transient(String),

    /// The backend answered, but not in a shape we can use (bad JSON, missing
    /// field, unexpected schema). Retrying verbatim is unlikely to help; this
    /// is a bug on one side of the wire, so it must stay loud.
    Protocol(String),
}

impl InferenceError {
    /// Is retrying the IDENTICAL request capable of succeeding?
    ///
    /// This is the predicate the settle loop needs and could not previously
    /// express. `ContextExceeded` answers `false` — which is the whole point:
    /// it must be re-rendered, not resent.
    pub fn is_retryable_unchanged(&self) -> bool {
        match self {
            Self::ContextExceeded { .. } => false,
            Self::Protocol(_) => false,
            Self::Unavailable(_) | Self::Transient(_) => true,
        }
    }

    /// Classify an HTTP error response from an OpenAI-compatible backend.
    ///
    /// `body` is the raw response body. llama-server sends
    /// `{"error":{"code":400,"message":"…","type":"exceed_context_size"}}`;
    /// we key on the machine-readable `type` first and fall back to the two
    /// numbers in `message` only when the tag is absent (older builds), so a
    /// backend that labels its errors is never re-parsed out of prose.
    /// Cost note: an overflow is only ever a 400, so the JSON parse is gated on
    /// that status — every other failure classifies without touching the body.
    /// Nothing here runs on the success path.
    pub fn from_http(status: u16, body: &str) -> Self {
        if status == 400 {
            if let Some(parsed) = Self::parse_context_exceeded(body) {
                return parsed;
            }
        }
        if status == 503 || status == 502 || status == 504 {
            return Self::Unavailable(format!("backend returned {status}: {body}"));
        }
        Self::Transient(format!("backend returned {status}: {body}"))
    }

    /// Extract `ContextExceeded` from an error body, or `None` if it is not one.
    fn parse_context_exceeded(body: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(body).ok()?;
        let error = value.get("error")?;
        let message = error.get("message").and_then(|m| m.as_str()).unwrap_or("");

        let tagged = error.get("type").and_then(|t| t.as_str()) == Some("exceed_context_size");
        let (requested, available) = Self::two_numbers(message)?;
        if !tagged && !message.contains("context size") {
            return None;
        }
        Some(Self::ContextExceeded {
            requested,
            available,
        })
    }

    /// Pull the two parenthesised token counts out of llama-server's message:
    /// `request (16751 tokens) exceeds the available context size (16384 tokens)`.
    ///
    /// Used ONLY to recover the numbers once the body has already been
    /// identified as a context error — never to decide *whether* it is one when
    /// the backend tagged it.
    fn two_numbers(message: &str) -> Option<(u32, u32)> {
        let mut found = message
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<u32>().ok());
        let requested = found.next()?;
        let available = found.next()?;
        Some((requested, available))
    }
}

impl fmt::Display for InferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextExceeded {
                requested,
                available,
            } => write!(
                f,
                "request ({requested} tokens) exceeds the slot's available context size ({available} tokens)"
            ),
            Self::Unavailable(detail) => write!(f, "backend unavailable: {detail}"),
            Self::Transient(detail) => write!(f, "{detail}"),
            Self::Protocol(detail) => write!(f, "protocol error: {detail}"),
        }
    }
}

impl std::error::Error for InferenceError {}

/// Adopting an untyped error is always possible and always honest: an
/// unclassified failure IS `Transient` under the #386 policy. This keeps the
/// conversion one-directional — call sites migrate to real variants over time,
/// and none of them regress into string matching in the meantime.
impl From<String> for InferenceError {
    fn from(message: String) -> Self {
        Self::Transient(message)
    }
}

impl From<&str> for InferenceError {
    fn from(message: &str) -> Self {
        Self::Transient(message.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the live 2026-08-13 defect — a tagged llama-server
    // context overflow must classify as ContextExceeded carrying BOTH numbers,
    // so the settle loop can re-render instead of resending forever.
    #[test]
    fn tagged_llama_server_overflow_classifies_with_both_numbers() {
        let body = r#"{"error":{"code":400,"message":"request (16751 tokens) exceeds the available context size (16384 tokens), try increasing it","type":"exceed_context_size"}}"#;
        assert_eq!(
            InferenceError::from_http(400, body),
            InferenceError::ContextExceeded {
                requested: 16751,
                available: 16384
            }
        );
    }

    // what this catches: the reason this type exists at all — a context
    // overflow must NOT be treated as retryable-unchanged, because the same
    // prompt against the same slot 400s forever (it burned whole turns).
    #[test]
    fn context_exceeded_is_not_retryable_unchanged() {
        let err = InferenceError::ContextExceeded {
            requested: 16751,
            available: 16384,
        };
        assert!(!err.is_retryable_unchanged());
        // …while the #386 transient fault, which measurably recovers, still is.
        assert!(InferenceError::Transient("decode stalled".into()).is_retryable_unchanged());
    }

    // what this catches: an UNTAGGED overflow (older llama-server builds) must
    // still classify, or the fix silently stops working after a downgrade.
    #[test]
    fn untagged_overflow_still_classifies_from_the_message() {
        let body = r#"{"error":{"code":400,"message":"request (15697 tokens) exceeds the available context size (15104 tokens)"}}"#;
        assert_eq!(
            InferenceError::from_http(400, body),
            InferenceError::ContextExceeded {
                requested: 15697,
                available: 15104
            }
        );
    }

    // what this catches: over-eager classification. A 400 that is NOT a context
    // problem must stay Transient — misreading one as ContextExceeded would
    // shrink a prompt for no reason and hide the real fault.
    #[test]
    fn unrelated_400_does_not_become_context_exceeded() {
        let body = r#"{"error":{"code":400,"message":"invalid 'temperature': expected number","type":"invalid_request_error"}}"#;
        assert!(matches!(
            InferenceError::from_http(400, body),
            InferenceError::Transient(_)
        ));
    }

    // what this catches: a 503 is a distinct condition from a generic fault —
    // both retry, but conflating them would lose the "backend is down" signal
    // an operator needs on the receipt.
    #[test]
    fn service_unavailable_classifies_as_unavailable() {
        assert!(matches!(
            InferenceError::from_http(503, "upstream not ready"),
            InferenceError::Unavailable(_)
        ));
    }

    // what this catches: Display must preserve the operator-facing prose so
    // probes/receipts stay legible after the String→type migration.
    #[test]
    fn display_keeps_the_numbers_an_operator_needs() {
        let shown = InferenceError::ContextExceeded {
            requested: 16751,
            available: 16384,
        }
        .to_string();
        assert!(shown.contains("16751"), "requested must survive: {shown}");
        assert!(shown.contains("16384"), "available must survive: {shown}");
    }

    // what this catches: a non-JSON body (proxy HTML, empty) must not panic and
    // must degrade to Transient rather than being dropped.
    #[test]
    fn non_json_body_degrades_to_transient_without_panicking() {
        assert!(matches!(
            InferenceError::from_http(400, "<html>502 Bad Gateway</html>"),
            InferenceError::Transient(_)
        ));
    }
}
