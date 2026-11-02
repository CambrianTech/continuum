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
    /// The wire's request counter (the IPC framing's `requestId`, an integer). Typed
    /// here so the ENVELOPE consumes it and a command's own `requestId: String` never
    /// sees an integer (measured 2026-09-06: the airc hop refused every well-formed
    /// peer-addressed generate with "invalid type: integer `1`, expected a string").
    ///
    /// AND THE INVERSE (measured 2026-09-17, the first hour cross-grid inference was
    /// addressable at all): a command's own `requestId: String` — `TextGenerationRequest`
    /// stamps a uuid on every persona turn — lands on the RESPONDER in this same flat
    /// namespace, and an `Option<u64>` refused it: "invalid type: string
    /// `05acabe7-…`, expected u64", five of six remote turns dead on arrival, 0 ms.
    /// Two ids share one key. The counter is the framing's (an integer); anything
    /// else under the key is the command's, and the envelope has no counter on that
    /// hop. The command's id does not survive the wire — it is per-hop correlation
    /// and the responder stamps its own — which is the price of the flat namespace,
    /// paid here once rather than by every command that carries a `requestId`.
    #[serde(
        rename = "requestId",
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "wire_request_counter"
    )]
    #[ts(optional, type = "number")]
    pub request_id: Option<u64>,
}

/// The framing's integer counter, or nothing: a string under `requestId` is a
/// command's own id sharing the key (see `CommandRequest::request_id`).
fn wire_request_counter<'de, D>(d: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<Value>::deserialize(d)?;
    Ok(match raw {
        Some(Value::Number(n)) => n.as_u64(),
        _ => None,
    })
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
        .filter(|k| !is_envelope_field(k.as_str()))
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

/// The base fields every wire request may carry beside its command params.
///
/// `actorKind` belongs here for the same reason as the rest, and its absence cost two
/// citizens a retry loop on 2026-09-05: they sent NO parameters, so `candidates` in
/// `param_mismatch_message` should have been empty and the dedicated "you sent no
/// parameters at all" branch should have fired. Instead actorKind made it non-empty and
/// they were told to fix a field the KERNEL had added for them — unremovable, so the
/// identical call was resent and the identical message came back. Observed six-plus times
/// in minutes, including a repeat the substrate had to collapse as near-identical.
///
/// The list is load-bearing beyond that message: anything naming a kernel field back to a
/// caller as something she "sent" points her at something she never typed. One place.
pub fn is_envelope_field(key: &str) -> bool {
    ENVELOPE_FIELDS.contains(&key)
}

/// Every wire name [`CommandRequest`] owns beside the command's params. ONE list, read
/// by [`is_envelope_field`] (the you-sent filter) and by the schema guard in this file's
/// tests: `params` is `#[serde(flatten)]`ed into the SAME object, so a params field
/// spelled like one of these is claimed by the envelope and never reaches the handler
/// — silently `None` for an option, "invalid type … expected struct HandleRef" for a
/// string. Five collisions before the guard (card ea28d2f6): actorKind (#3781),
/// requestId (#4178), `handle` on the staging path (#3924 — why `dbPath` exists), and
/// `jobHandle` (#4326), where a command whose input was named `handle` could not be
/// called through the envelope at all.
pub const ENVELOPE_FIELDS: &[&str] =
    &["handle", "sessionId", "userId", "actorKind", "contextId", "requestId"];

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
            request_id: None,
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
mod tests;
