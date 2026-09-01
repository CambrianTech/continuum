//! Command envelopes — typed wrappers around the cross-cutting params
//! and result fields every command shares.
//!
//! # The pattern
//!
//! Per Joel 2026-05-30: *"Some things are used so much should just be
//! part of command result and params, handle for example. Find the
//! patterns and simplify. The better the pattern, the easier to use
//! the command or to reduce code size."*
//!
//! Right. A handle for long-running work, a session ID, a user ID,
//! a success flag, an optional error message — these are cross-cutting
//! concerns every command touches in some combination. Today's
//! `ServiceModule::handle_command(command, params: Value) ->
//! Result<CommandResult, String>` shovels everything through raw JSON;
//! handlers re-parse the cross-cutting bits themselves and rebuild the
//! same envelope at every return point.
//!
//! This module gives that pattern a name:
//!
//! - **`CommandRequest<P>`** — typed envelope around an inbound command:
//!   the command-specific params `P` flattened with `handle`, `sessionId`,
//!   `userId`. Parsers + accessors live here so handlers don't re-roll
//!   the wheel.
//!
//! - **`CommandResponse<T>`** — typed envelope around the outbound
//!   result: the command-specific data `T` flattened with `success`,
//!   `error`, optional `handle` for follow-up calls. Builder-style API
//!   so producing both data AND a handle is one fluent expression.
//!
//! Existing handlers keep their `Value`-based signatures (back-compat
//! for the 300+ surface). New handlers opt into the typed shape via
//! `CommandRequest::<P>::from_value(params)?` at the entry +
//! `.into_command_result()?` at the exit. Same `ServiceModule` trait,
//! tighter internal pattern.
//!
//! # What this collapses
//!
//! Before:
//!
//! ```ignore
//! async fn handle_inference_start(
//!     &self,
//!     params: Value,
//! ) -> Result<CommandResult, String> {
//!     let p: InferenceStartParams =
//!         serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
//!     let session_id = params
//!         .get("sessionId")
//!         .and_then(|v| v.as_str())
//!         .and_then(|s| Uuid::parse_str(s).ok());
//!     let id = Uuid::new_v4();
//!     self.sessions.insert(id, InferenceSession::new(p));
//!     Ok(CommandResult::Json(serde_json::json!({
//!         "success": true,
//!         "firstToken": first_token,
//!         "handle": HandleRef::with_id("ai/inference", id, "ai::InferenceSession"),
//!     })))
//! }
//! ```
//!
//! After:
//!
//! ```ignore
//! async fn handle_inference_start(
//!     &self,
//!     params: Value,
//! ) -> Result<CommandResult, String> {
//!     let req = CommandRequest::<InferenceStartParams>::from_value(params)?;
//!     let id = Uuid::new_v4();
//!     self.sessions.insert(id, InferenceSession::new(req.params));
//!     CommandResponse::ok(InferenceStartData { first_token })
//!         .with_handle("ai/inference", id, "ai::InferenceSession")
//!         .into_command_result()
//! }
//! ```
//!
//! The cross-cutting fields stop being something handlers have to know
//! about. They become free.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use super::cell_shapes::HandleRef;
use super::CommandResult;

/// Typed envelope around an inbound command's params.
///
/// Wraps the command-specific `P` with the cross-cutting fields every
/// command can carry:
///
/// - `handle` — a [`HandleRef`] from a previous call. Present when this
///   command is operating on existing state owned by another command
///   (e.g., `inference/poll` carries the handle minted by
///   `inference/start`).
/// - `session_id` — the calling session. Threaded by the kernel for
///   dual logging + accountability.
/// - `user_id` — the calling user. Threaded by the kernel for
///   per-user scoping (e.g., per-persona work).
///
/// `P` is flattened into the JSON envelope at deserialize time, so
/// the wire shape stays flat (same as today's untyped commands). The
/// type machinery is purely a Rust-side convenience.
///
/// # Construction
///
/// Handlers parse a `CommandRequest<P>` from the raw `Value` they
/// receive via `ServiceModule::handle_command` using
/// [`CommandRequest::from_value`]. The parser yields a typed struct
/// where the command-specific fields live in `params` and the
/// cross-cutting fields live at the top.
///
/// Tests + one-off callsites can construct directly via the public
/// fields.
///
/// # ts-rs / SDK
///
/// This is ALSO the single source of the generated TS envelope generic
/// (`sdk_codegen`): `#[derive(TS)]` exports it to
/// `protocol/typescript/runtime/CommandRequest.ts` as
/// `export type CommandRequest<P> = P & { handle?, sessionId?, userId?, contextId? }`.
/// An `Executed` command's generated `CommandMap` entry wraps its params
/// `P` in this generic — so the typed surface cannot drift from the Rust
/// envelope. `P` is flattened (`#[ts(flatten)]`) to match the flat wire JSON.
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/CommandRequest.ts"
)]
pub struct CommandRequest<P> {
    /// Command-specific params, deserialized from the same JSON object
    /// as the envelope. Flatten means the wire JSON looks like
    /// `{ ...P fields..., handle?, sessionId?, userId? }`.
    #[serde(flatten)]
    #[ts(flatten)]
    pub params: P,

    /// Handle to existing state from a prior command call. Present
    /// when this command operates on a long-running session (inference,
    /// training, hosting, ORM, etc.) — the producer minted the handle;
    /// this caller passes it back to thread the work.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub handle: Option<HandleRef>,

    /// Calling session — set by the kernel from the request envelope.
    /// Handlers reading this can correlate per-session telemetry, dual
    /// log, etc.
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    #[ts(optional, type = "string")]
    pub session_id: Option<Uuid>,

    /// Calling user — set by the kernel from the session. Handlers
    /// reading this can scope per-user state (e.g., per-persona work).
    #[serde(rename = "userId", skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    #[ts(optional, type = "string")]
    pub user_id: Option<Uuid>,

    /// Conversation / room scope this command operates within — the THIRD
    /// ID tier (userId > sessionId > contextId, CLAUDE.md ID hierarchy).
    ///
    /// UNLIKE `session_id`/`user_id` — which the kernel injects from the
    /// connection (the substrate knows WHO you are from the airc pairing) —
    /// `context_id` is CLIENT-SUPPLIED: the caller scopes a sequence of ops
    /// to a conversation via the SDK's scoped client (`Continuum.scoped(ctx)`),
    /// and it rides as an envelope sibling so handlers scope per-context state
    /// (per-room memory, per-thread recall) without it polluting command
    /// params. First-class for every citizen — a persona servicing a room is
    /// a citizen scoped to that room's contextId, the same shape a browser tab
    /// uses (this is what fills the persona cognition's tool_context).
    /// The calling process's CLAIMED actor kind — `"agent"` when an AI agent
    /// session (Claude Code, Codex…) drives the CLI, stamped by the CLI from
    /// its own environment. A CLAIM for local attribution (the caller-less
    /// sender chain resolves to the AGENT self-peer instead of the human
    /// operator — Joel, 2026-09-01: "the chat history is clearly attributing
    /// shit you did to me"), never an authentication: authenticated identity
    /// stays `ctx.caller` (the airc gate).
    #[serde(rename = "actorKind", skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub actor_kind: Option<String>,

    #[serde(rename = "contextId", skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    #[ts(optional, type = "string")]
    pub context_id: Option<Uuid>,
}

/// Turn serde's one-sided "missing field `cmd`" into a two-sided diagnosis.
///
/// Every caller — persona, CLI, SDK — sends a params object, and the single most common
/// failure is a NAME mismatch, not a missing intent: `command` for `cmd`, `path` for
/// `file_path`. serde reports only what it WANTED, so the message reads as "you forgot
/// something" when the truth is "you called it something else, and here is the something".
/// Measured on myself: two different commands, two mismatches, inside two minutes — a model
/// reaching for the industry-standard name hits this constantly and has nothing to correct
/// toward.
///
/// One seam, every command: this is the only place a params object is decoded.
///
/// Naming what was SENT is the fix. The near-miss suggestion is a bonus and is deliberately
/// conservative — when nothing is close, listing the sent keys is still strictly better than
/// naming the wanted one alone.
fn param_mismatch_message(serde_error: &str, sent: &[String]) -> String {
    let base = format!("CommandRequest deserialization failed: {serde_error}");
    // serde renders this as: missing field `cmd`
    let Some(wanted) = serde_error
        .split_once("missing field `")
        .and_then(|(_, rest)| rest.split_once('`'))
        .map(|(name, _)| name)
    else {
        return base;
    };
    // Params the caller controls — the envelope's own fields are not the mismatch.
    let candidates: Vec<&String> = sent
        .iter()
        // Envelope + transport fields are the kernel's, never her params — parading them back
        // as "you sent" points at things she never typed.
        //
        // `command` is deliberately NOT filtered even though the CLI adds it: for `code/shell`
        // it is the single likeliest mis-name (`command` for `cmd`), and suppressing it would
        // break the exact case this function exists for. A little CLI noise beats losing the
        // real diagnosis — caught by this function's own test.
        .filter(|k| {
            !matches!(
                k.as_str(),
                "handle" | "sessionId" | "userId" | "contextId" | "requestId"
            )
        })
        .collect();
    if candidates.is_empty() {
        return format!(
            "{base}. You sent no parameters at all — this command requires `{wanted}`."
        );
    }
    let sent_list = candidates
        .iter()
        .map(|k| format!("`{k}`"))
        .collect::<Vec<_>>()
        .join(", ");
    if let Some(near) = closest_param(wanted, &candidates) {
        return format!(
            "{base}. You sent {sent_list} — this command calls that parameter `{wanted}`, not `{near}`. Re-send with `{wanted}`."
        );
    }
    format!("{base}. You sent {sent_list}, but this command requires `{wanted}`.")
}

/// The sent key most plausibly MEANT as `wanted`.
///
/// Two kinships, because the real pairs come in two shapes and the obvious check only catches
/// one — my first version used substring alone and its own test caught that `cmd` is NOT a
/// substring of `command` (the letters are not contiguous), i.e. it failed on the exact case
/// that motivated it:
///
/// - QUALIFIER: `path` ⊂ `file_path` — substring.
/// - ABBREVIATION: `cmd` ⊆ `command` — subsequence, the letters in order.
///
/// Both stay tight enough to refuse unrelated names: `cmd` is not a subsequence of `colour`
/// (no `m`), and a wrong "you meant X" would send her to rename the wrong field, which is
/// worse than a plain listing.
fn closest_param<'a>(wanted: &str, sent: &[&'a String]) -> Option<&'a str> {
    let w = wanted.to_lowercase();
    sent.iter()
        .find(|k| {
            let k = k.to_lowercase();
            k.contains(&w) || w.contains(&k) || is_subsequence(&k, &w) || is_subsequence(&w, &k)
        })
        .map(|k| k.as_str())
}

/// Is `short` an in-order subsequence of `long` — the abbreviation relation? Requires at least
/// 3 characters so a 1-2 letter param never matches half the alphabet.
fn is_subsequence(short: &str, long: &str) -> bool {
    if short.len() < 3 || short.len() >= long.len() {
        return false;
    }
    let mut chars = long.chars();
    short.chars().all(|c| chars.any(|l| l == c))
}

impl<P> CommandRequest<P>
where
    P: serde::de::DeserializeOwned,
{
    /// Parse a `CommandRequest<P>` from a raw `Value`. The
    /// command-specific fields go into `params`; `handle`, `sessionId`,
    /// `userId` are pulled from the top level of the same object.
    ///
    /// Error is a String describing the failure, matching the existing
    /// `ServiceModule::handle_command` error type so handlers can `?`
    /// the result directly.
    pub fn from_value(value: Value) -> Result<Self, String> {
        // Keep what the caller actually sent — serde's error names only what it WANTED, and
        // the gap between those two is the whole diagnosis.
        let sent: Vec<String> = value
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        serde_json::from_value(value).map_err(|e| param_mismatch_message(&e.to_string(), &sent))
    }
}

impl<P> CommandRequest<P> {
    /// Construct a request envelope for tests or programmatic callsites
    /// where the params are already in-hand. The cross-cutting fields
    /// default to `None`; chain `with_handle`/`with_session`/`with_user`
    /// to populate them.
    pub fn new(params: P) -> Self {
        Self {
            params,
            handle: None,
            session_id: None,
            user_id: None,
            context_id: None,
            actor_kind: None,
        }
    }

    pub fn with_handle(mut self, handle: HandleRef) -> Self {
        self.handle = Some(handle);
        self
    }

    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn with_user(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Scope this request to a conversation/room (the third ID tier). The SDK's
    /// scoped client stamps it; handlers read it for per-context state.
    pub fn with_context(mut self, context_id: Uuid) -> Self {
        self.context_id = Some(context_id);
        self
    }

    /// Resolve a resource id during migration from string-typed ids to
    /// typed [`HandleRef`]s, returning the id as a string.
    ///
    /// Walks two possible shapes in priority order:
    ///
    /// 1. **Envelope `handle`** (the new canonical shape). When
    ///    present, validates against the expected `owner` and
    ///    `type_tag` via [`HandleRef::expect_owned_by`]; a failure
    ///    here surfaces with the `command` name prepended so the
    ///    consumer's error names the offending surface, the failure
    ///    mode, and the expected values in one breath.
    ///
    /// 2. **Legacy string field** (the back-compat shape). Returned
    ///    as-is. The historical wire contract pre-dates UUID typing,
    ///    so legacy callers may send anything — if the string fails
    ///    the consumer's downstream lookup, the consumer's own
    ///    "not found" error names it.
    ///
    /// 3. **Neither present** — typed error naming BOTH supported
    ///    shapes so the caller knows what to add.
    ///
    /// This is the single primitive shared by every additive
    /// migration of a stringly-typed id to a typed handle. See
    /// `data.rs`'s `handle_query_next` / `handle_query_close` for the
    /// canonical consumer; other migrations should reach for this
    /// rather than reimplementing the resolver.
    ///
    /// # Why does it return `String`?
    ///
    /// Two callers consume the same id today:
    /// - the envelope path produces a `Uuid` (typed)
    /// - the legacy path produces a string (predates UUID typing)
    ///
    /// To present a unified resolved-id type to the consumer, we
    /// collapse to `String` — the historical wire format that every
    /// consumer's existing state map is already keyed on. Future
    /// modules whose state maps are keyed on `Uuid` can `Uuid::parse_str`
    /// the result; the parse failure mode for legacy strings is fine
    /// because handle-only consumers (post-migration) won't have a
    /// legacy field to fall back to anyway.
    ///
    /// # Usage
    ///
    /// ```ignore
    /// let cursor_id = req.handle_id_or_legacy(
    ///     "data",                   // expected owner
    ///     "data::QueryCursor",      // expected type_tag
    ///     "queryId",                // legacy field name (for error)
    ///     &req.params.query_id,     // legacy field value (Option<String>)
    ///     "data/query-next",        // command name (for error prefix)
    /// )?;
    /// ```
    pub fn handle_id_or_legacy(
        &self,
        expected_owner: &str,
        expected_type_tag: &str,
        legacy_field_name: &str,
        legacy_field: &Option<String>,
        command: &str,
    ) -> Result<String, String> {
        if let Some(h) = &self.handle {
            return h
                .expect_owned_by(expected_owner, expected_type_tag)
                .map(|uuid| uuid.to_string())
                .map_err(|e| format!("{command}: {e}"));
        }
        if let Some(id) = legacy_field {
            return Ok(id.clone());
        }
        Err(format!(
            "{command}: neither `handle` (envelope field) nor `{legacy_field_name}` \
             (legacy params field) was provided. Pass the resource id via either shape."
        ))
    }
}

/// Typed envelope around an outbound command's result.
///
/// Wraps the command-specific `T` with the cross-cutting fields every
/// command can produce:
///
/// - `success` — operation-level success flag, mirrored in the JSON
///   envelope. Stays `true` until something fails; an error-returning
///   handler should construct via [`CommandResponse::err`] which sets
///   it to `false`.
/// - `error` — operation-level error message. `None` when success.
/// - `handle` — a [`HandleRef`] minted by this command for the caller
///   to use in follow-up calls. The "first call returns a handle"
///   pattern Joel called out for inference / training / hosting /
///   ORM lives here.
///
/// `T` is flattened into the JSON envelope at serialize time so the
/// wire shape stays flat. A handler producing `{ firstToken: "..." }`
/// + a handle for follow-up materializes as
/// `{ success: true, firstToken: "...", handle: {...} }` — same
/// flat shape callers already know.
///
/// # Construction (builder)
///
/// `CommandResponse::ok(data)` for the happy path, then chain
/// `.with_handle(...)` for the long-running case. `CommandResponse::err
/// (msg)` for failure when `T: Default` (callers without a default just
/// build the struct directly).
///
/// Materialize as a `CommandResult` (the ServiceModule return shape)
/// via [`CommandResponse::into_command_result`]: serialize-flatten +
/// wrap as `CommandResult::Json`. One method call to bridge the typed
/// envelope into the existing kernel surface.
///
/// # ts-rs / SDK
///
/// The single source of the generated TS result-envelope generic: exports
/// to `protocol/typescript/runtime/CommandResponse.ts` as
/// `export type CommandResponse<T> = T & { success, handle?, error? }`. An
/// `Executed` command's generated `CommandMap` entry wraps its result `T`
/// in this generic, so a caller always sees the cross-cutting
/// success/error/handle alongside the command-specific payload.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/CommandResponse.ts"
)]
pub struct CommandResponse<T> {
    /// Operation succeeded. Default `true`; flipped by
    /// [`CommandResponse::err`].
    pub success: bool,

    /// Command-specific result payload, flattened into the wire JSON
    /// alongside the envelope fields.
    #[serde(flatten)]
    #[ts(flatten)]
    pub data: T,

    /// Handle minted by this command for the caller to use in follow-up
    /// calls — the long-running session pattern.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<HandleRef>,

    /// Operation-level error message. Set when `success == false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

impl<T> CommandResponse<T> {
    /// Construct a successful response with the given payload. Use
    /// `.with_handle(...)` to attach a handle for follow-up.
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data,
            handle: None,
            error: None,
        }
    }

    /// Attach a handle to this response. Producer typically minted a
    /// UUID, stored state under it, and now returns the handle for the
    /// caller's subsequent operations.
    pub fn with_handle(
        mut self,
        owner: impl Into<String>,
        id: Uuid,
        type_tag: impl Into<String>,
    ) -> Self {
        self.handle = Some(HandleRef::with_id(owner, id, type_tag));
        self
    }

    /// Attach a pre-built [`HandleRef`]. Use when the caller already
    /// has a handle struct (e.g., echoing a downstream module's handle).
    pub fn with_handle_ref(mut self, handle: HandleRef) -> Self {
        self.handle = Some(handle);
        self
    }
}

impl<T: Default> CommandResponse<T> {
    /// Construct a failure response with an error message. Requires
    /// `T: Default` so the data field has a value; callers whose `T`
    /// doesn't default should construct directly.
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: T::default(),
            handle: None,
            error: Some(message.into()),
        }
    }
}

impl<T: Serialize> CommandResponse<T> {
    /// Materialize this typed envelope as a `CommandResult::Json`
    /// suitable for the `ServiceModule::handle_command` return.
    ///
    /// Serializes the whole envelope (with `T` flattened) to a JSON
    /// value and wraps. The Result error is the serialization failure,
    /// matching the canonical `ServiceModule` error string type.
    pub fn into_command_result(self) -> Result<CommandResult, String> {
        serde_json::to_value(&self)
            .map(CommandResult::Json)
            .map_err(|e| format!("CommandResponse serialization failed: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── CommandRequest<P> ────────────────────────────────────────────

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)]
    struct StartParams {
        model: String,
        max_tokens: u32,
    }

    #[test]
    fn request_parses_flat_params_no_envelope_fields() {
        // Wire JSON without any envelope fields — pure command params.
        let value = json!({ "model": "qwen", "max_tokens": 512 });
        let req = CommandRequest::<StartParams>::from_value(value).expect("parse must succeed");
        assert_eq!(req.params.model, "qwen");
        assert_eq!(req.params.max_tokens, 512);
        assert!(
            req.handle.is_none()
                && req.session_id.is_none()
                && req.user_id.is_none()
                && req.context_id.is_none(),
            "envelope fields default to None when absent in the wire JSON"
        );
    }

    #[test]
    fn request_parses_envelope_fields_flat() {
        let session_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let handle_id = Uuid::new_v4();
        let value = json!({
            "model": "qwen",
            "max_tokens": 256,
            "sessionId": session_id.to_string(),
            "userId": user_id.to_string(),
            "handle": {
                "owner": "ai/inference",
                "id": handle_id.to_string(),
                "type_tag": "ai::InferenceSession",
                "created_at_ms": 1_700_000_000_000_u64
            }
        });
        let req = CommandRequest::<StartParams>::from_value(value).expect("parse must succeed");
        assert_eq!(req.params.model, "qwen");
        assert_eq!(req.session_id, Some(session_id));
        assert_eq!(req.user_id, Some(user_id));
        assert_eq!(req.handle.unwrap().id.as_uuid(), handle_id);
    }

    #[test]
    fn request_parse_error_carries_diagnostic() {
        // Wrong types — `max_tokens` is a string. Parser must surface
        // a String error, not panic.
        let value = json!({ "model": "qwen", "max_tokens": "not-a-number" });
        let err = CommandRequest::<StartParams>::from_value(value)
            .expect_err("type mismatch must surface as Err, not panic");
        assert!(
            err.contains("CommandRequest deserialization failed"),
            "error must name the envelope so the caller knows which layer failed: {err}"
        );
    }

    // what this catches: the most common tool-use failure there is — a NAME mismatch, reported
    // one-sidedly. serde says "missing field `cmd`" and never mentions that the caller sent
    // `command`, so the message reads as "you forgot something" when the truth is "you called
    // it something else". Measured on myself: two commands, two mismatches, two minutes. ONE
    // seam fixes every command, because this is the only place a params object is decoded.
    #[test]
    fn a_param_name_mismatch_names_what_was_sent_not_just_what_was_wanted() {
        let msg = param_mismatch_message(
            "missing field `cmd`",
            &["command".to_string(), "timeout".to_string()],
        );
        assert!(
            msg.contains("You sent `command`"),
            "must name what she sent: {msg}"
        );
        assert!(
            msg.contains("calls that parameter `cmd`, not `command`"),
            "must state the correspondence, not just the wanted name: {msg}"
        );

        // Envelope fields are the kernel's, never the caller's mistake — they must not be
        // paraded back as if she had mis-named something.
        let msg = param_mismatch_message(
            "missing field `file_path`",
            &[
                "path".to_string(),
                "sessionId".to_string(),
                "userId".to_string(),
            ],
        );
        assert!(msg.contains("`path`"), "the real candidate survives: {msg}");
        assert!(
            !msg.contains("sessionId"),
            "envelope fields are not param candidates: {msg}"
        );
    }

    // what this catches: a confident WRONG rename is worse than none — it sends her to change
    // the wrong field. Unrelated names must degrade to a plain listing, and no params at all
    // must say so plainly rather than implying a rename.
    #[test]
    fn an_unrelated_param_gets_a_listing_never_an_invented_rename() {
        let msg = param_mismatch_message("missing field `cmd`", &["colour".to_string()]);
        assert!(
            !msg.contains("not `colour`"),
            "must not invent a correspondence between unrelated names: {msg}"
        );
        assert!(
            msg.contains("You sent `colour`") && msg.contains("requires `cmd`"),
            "but must still show both sides: {msg}"
        );

        let msg = param_mismatch_message("missing field `cmd`", &[]);
        assert!(
            msg.contains("no parameters at all"),
            "an empty call deserves its own sentence: {msg}"
        );

        // A non-missing-field error is passed through untouched.
        let msg = param_mismatch_message("invalid type: string, expected u32", &["n".to_string()]);
        assert!(
            msg.ends_with("expected u32"),
            "unrelated errors are not rewritten: {msg}"
        );
    }

    #[test]
    fn request_builder_attaches_envelope_fields() {
        let handle = HandleRef::mint("ai/inference", "ai::InferenceSession");
        let session_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let req = CommandRequest::new(StartParams {
            model: "qwen".into(),
            max_tokens: 100,
        })
        .with_handle(handle.clone())
        .with_session(session_id)
        .with_user(user_id);
        assert_eq!(req.handle, Some(handle));
        assert_eq!(req.session_id, Some(session_id));
        assert_eq!(req.user_id, Some(user_id));
    }

    // what this catches: the THIRD ID tier (contextId) parses from the flat wire
    // envelope as a sibling of sessionId/userId, and the builder attaches it.
    // contextId is CLIENT-supplied (the SDK's scoped client stamps it), so unlike
    // session/user it must survive a round-trip from the caller's JSON. A drift
    // that drops it would silently un-scope every per-context handler (per-room
    // memory, per-thread recall, a persona's tool_context).
    #[test]
    fn request_parses_and_builds_context_id_third_tier() {
        let context_id = Uuid::new_v4();
        // From flat wire JSON (the caller stamped contextId alongside params).
        let value = json!({
            "model": "qwen",
            "max_tokens": 256,
            "contextId": context_id.to_string(),
        });
        let req = CommandRequest::<StartParams>::from_value(value).expect("parse must succeed");
        assert_eq!(req.context_id, Some(context_id));
        // Round-trips back out under the camelCase wire key.
        let back = serde_json::to_value(&req).expect("serialize");
        assert_eq!(back["contextId"], json!(context_id.to_string()));
        // And the builder attaches it.
        let built = CommandRequest::new(StartParams {
            model: "q".into(),
            max_tokens: 1,
        })
        .with_context(context_id);
        assert_eq!(built.context_id, Some(context_id));
    }

    // ── CommandResponse<T> ───────────────────────────────────────────

    #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
    struct StartData {
        first_token: String,
        tokens_emitted: u32,
    }

    #[test]
    fn response_ok_serializes_flat_with_success_true() {
        let resp = CommandResponse::ok(StartData {
            first_token: "Hello".into(),
            tokens_emitted: 1,
        });
        let json = serde_json::to_value(&resp).expect("serialize must succeed");
        assert_eq!(json["success"], true);
        assert_eq!(json["first_token"], "Hello");
        assert_eq!(json["tokens_emitted"], 1);
        assert!(
            json.get("handle").is_none(),
            "handle is omitted when None — clean wire shape"
        );
        assert!(json.get("error").is_none(), "error is omitted when None");
    }

    #[test]
    fn response_with_handle_attaches_handle_at_top_level() {
        let id = Uuid::new_v4();
        let resp = CommandResponse::ok(StartData {
            first_token: "Hi".into(),
            tokens_emitted: 1,
        })
        .with_handle("ai/inference", id, "ai::InferenceSession");
        let json = serde_json::to_value(&resp).expect("serialize must succeed");
        assert_eq!(json["success"], true);
        assert_eq!(json["handle"]["owner"], "ai/inference");
        assert_eq!(json["handle"]["id"], id.to_string());
        assert_eq!(json["handle"]["type_tag"], "ai::InferenceSession");
        // Data fields stay flat alongside the handle.
        assert_eq!(json["first_token"], "Hi");
    }

    #[test]
    fn response_err_serializes_with_success_false_and_message() {
        let resp = CommandResponse::<StartData>::err("model not found: 'qwen-99'");
        let json = serde_json::to_value(&resp).expect("serialize must succeed");
        assert_eq!(json["success"], false);
        assert_eq!(json["error"], "model not found: 'qwen-99'");
        // Default data fields still present (empty strings, 0 counts).
        assert_eq!(json["first_token"], "");
        assert_eq!(json["tokens_emitted"], 0);
    }

    #[test]
    fn response_into_command_result_yields_json_variant() {
        let resp = CommandResponse::ok(StartData {
            first_token: "Hi".into(),
            tokens_emitted: 1,
        })
        .with_handle("ai/inference", Uuid::new_v4(), "ai::InferenceSession");
        let cr = resp
            .into_command_result()
            .expect("materialize must succeed");
        match cr {
            CommandResult::Json(v) => {
                assert_eq!(v["success"], true);
                assert_eq!(v["first_token"], "Hi");
                assert!(v["handle"].is_object());
            }
            other => panic!("expected CommandResult::Json, got {other:?}"),
        }
    }

    #[test]
    fn round_trip_through_wire_preserves_envelope_fields() {
        // End-to-end: typed handler returns Response → serialize as
        // CommandResult → echo as string → deserialize on a "caller"
        // side. The caller-side gets a CommandRequest envelope back
        // (treating the result as the next call's input) — handle,
        // session, user all survive.
        let session_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let handle_id = Uuid::new_v4();

        // Build a response carrying a handle (the producer minted it).
        let resp = CommandResponse::ok(StartData {
            first_token: "Hi".into(),
            tokens_emitted: 1,
        })
        .with_handle("ai/inference", handle_id, "ai::InferenceSession");
        let wire_json = serde_json::to_value(&resp).unwrap();

        // Caller takes the result, builds a new request envelope using
        // the returned handle (+ their own session/user). The new
        // request's params type is a "poll" shape.
        #[derive(Debug, Clone, Deserialize, Serialize, TS)]
        struct PollParams {
            max_tokens: u32,
        }

        let mut next_call = json!({ "max_tokens": 64 });
        next_call["handle"] = wire_json["handle"].clone();
        next_call["sessionId"] = json!(session_id.to_string());
        next_call["userId"] = json!(user_id.to_string());

        let req = CommandRequest::<PollParams>::from_value(next_call)
            .expect("caller round-trips envelope cleanly");
        assert_eq!(req.params.max_tokens, 64);
        assert_eq!(req.session_id, Some(session_id));
        assert_eq!(req.user_id, Some(user_id));
        assert_eq!(req.handle.unwrap().id.as_uuid(), handle_id);
    }

    // ── CommandRequest::handle_id_or_legacy ─────────────────────────
    //
    // The single primitive shared by every additive migration of a
    // stringly-typed id to a typed handle. Distilled from data
    // module's first real consumer (PR #1490) so future migrations
    // don't reimplement the resolver. Each kink the data migration
    // discovered is pinned by a test here so the substrate
    // guarantees them centrally.

    #[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
    #[serde(rename_all = "camelCase")]
    struct CursorParams {
        #[serde(default)]
        query_id: Option<String>,
    }

    fn cursor_handle(id: Uuid) -> HandleRef {
        HandleRef::with_id("data", id, "data::QueryCursor")
    }

    #[test]
    fn handle_id_or_legacy_prefers_envelope_handle_when_both_present() {
        // When the envelope carries a handle AND a legacy field is
        // also present, the typed handle wins. Otherwise consumers
        // mid-migration would diverge from new consumers about which
        // id the resolver sees.
        let h_id = Uuid::new_v4();
        let req = CommandRequest::new(CursorParams {
            query_id: Some(Uuid::new_v4().to_string()), // legacy populated
        })
        .with_handle(cursor_handle(h_id));

        let resolved = req
            .handle_id_or_legacy(
                "data",
                "data::QueryCursor",
                "queryId",
                &req.params.query_id,
                "data/query-next",
            )
            .expect("envelope handle must win");
        assert_eq!(
            resolved,
            h_id.to_string(),
            "envelope handle MUST win when both shapes are present"
        );
    }

    #[test]
    fn handle_id_or_legacy_falls_back_to_legacy_string_when_no_handle() {
        let legacy = "11111111-2222-3333-4444-555555555555".to_string();
        let req = CommandRequest::new(CursorParams {
            query_id: Some(legacy.clone()),
        });

        let resolved = req
            .handle_id_or_legacy(
                "data",
                "data::QueryCursor",
                "queryId",
                &req.params.query_id,
                "data/query-next",
            )
            .expect("legacy fallback must succeed");
        assert_eq!(resolved, legacy, "legacy string returned as-is");
    }

    #[test]
    fn handle_id_or_legacy_errors_loud_when_neither_shape_provided() {
        let req = CommandRequest::new(CursorParams::default());
        let err = req
            .handle_id_or_legacy(
                "data",
                "data::QueryCursor",
                "queryId",
                &req.params.query_id,
                "data/query-next",
            )
            .expect_err("empty request must Err");
        assert!(
            err.contains("data/query-next"),
            "error must name the failing command surface: {err}"
        );
        assert!(
            err.contains("`handle`") && err.contains("`queryId`"),
            "error must name BOTH supported shapes so caller knows what to add: {err}"
        );
    }

    #[test]
    fn handle_id_or_legacy_prepends_command_name_to_handle_validation_errors() {
        // Critical for diagnostics: when a wrong-owner handle reaches
        // this resolver, the error must name BOTH the failing command
        // (so the caller knows which surface) AND the
        // HandleRef-level mismatch (so the caller knows what to fix).
        let req = CommandRequest::new(CursorParams::default())
            .with_handle(HandleRef::mint("chat", "chat::MessageHandle"));

        let err = req
            .handle_id_or_legacy(
                "data",
                "data::QueryCursor",
                "queryId",
                &req.params.query_id,
                "data/query-next",
            )
            .expect_err("wrong-owner handle must Err");
        assert!(
            err.starts_with("data/query-next:"),
            "command name must prefix the error: {err}"
        );
        assert!(
            err.contains("owner mismatch"),
            "HandleRef's failure mode must propagate: {err}"
        );
        assert!(
            err.contains("\"chat\"") && err.contains("\"data\""),
            "both offender and expected named: {err}"
        );
    }

    #[test]
    fn handle_id_or_legacy_propagates_type_mismatch_with_command_name() {
        let req = CommandRequest::new(CursorParams::default())
            .with_handle(HandleRef::mint("data", "data::Migration"));

        let err = req
            .handle_id_or_legacy(
                "data",
                "data::QueryCursor",
                "queryId",
                &req.params.query_id,
                "data/query-close",
            )
            .expect_err("wrong-type handle must Err");
        assert!(
            err.starts_with("data/query-close:"),
            "command prefix: {err}"
        );
        assert!(
            err.contains("type mismatch"),
            "type mismatch propagates: {err}"
        );
        assert!(
            err.contains("data::Migration") && err.contains("data::QueryCursor"),
            "both offender and expected named: {err}"
        );
    }

    #[test]
    fn handle_id_or_legacy_uses_canonical_uuid_string_for_handle_path() {
        // The envelope path must produce the UUID's canonical string
        // form (not some other rendering), so downstream consumers
        // can use the resolved string as a stable cache key with
        // legacy-path values from the same migration window.
        let id = Uuid::new_v4();
        let req = CommandRequest::new(CursorParams::default()).with_handle(cursor_handle(id));
        let resolved = req
            .handle_id_or_legacy(
                "data",
                "data::QueryCursor",
                "queryId",
                &req.params.query_id,
                "data/query-next",
            )
            .unwrap();
        assert_eq!(
            resolved,
            id.to_string(),
            "canonical UUID string is the bridge format between handle and legacy paths"
        );
    }
}
