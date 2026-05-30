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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandRequest<P> {
    /// Command-specific params, deserialized from the same JSON object
    /// as the envelope. Flatten means the wire JSON looks like
    /// `{ ...P fields..., handle?, sessionId?, userId? }`.
    #[serde(flatten)]
    pub params: P,

    /// Handle to existing state from a prior command call. Present
    /// when this command operates on a long-running session (inference,
    /// training, hosting, ORM, etc.) — the producer minted the handle;
    /// this caller passes it back to thread the work.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub handle: Option<HandleRef>,

    /// Calling session — set by the kernel from the request envelope.
    /// Handlers reading this can correlate per-session telemetry, dual
    /// log, etc.
    #[serde(
        rename = "sessionId",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub session_id: Option<Uuid>,

    /// Calling user — set by the kernel from the session. Handlers
    /// reading this can scope per-user state (e.g., per-persona work).
    #[serde(rename = "userId", skip_serializing_if = "Option::is_none", default)]
    pub user_id: Option<Uuid>,
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
        serde_json::from_value(value)
            .map_err(|e| format!("CommandRequest deserialization failed: {e}"))
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
#[derive(Debug, Clone, Serialize)]
pub struct CommandResponse<T> {
    /// Operation succeeded. Default `true`; flipped by
    /// [`CommandResponse::err`].
    pub success: bool,

    /// Command-specific result payload, flattened into the wire JSON
    /// alongside the envelope fields.
    #[serde(flatten)]
    pub data: T,

    /// Handle minted by this command for the caller to use in follow-up
    /// calls — the long-running session pattern.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<HandleRef>,

    /// Operation-level error message. Set when `success == false`.
    #[serde(skip_serializing_if = "Option::is_none")]
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

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
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
            req.handle.is_none() && req.session_id.is_none() && req.user_id.is_none(),
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
        assert_eq!(req.handle.unwrap().id, handle_id);
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

    // ── CommandResponse<T> ───────────────────────────────────────────

    #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
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
        let cr = resp.into_command_result().expect("materialize must succeed");
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
        #[derive(Debug, Clone, Deserialize, Serialize)]
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
        assert_eq!(req.handle.unwrap().id, handle_id);
    }
}
