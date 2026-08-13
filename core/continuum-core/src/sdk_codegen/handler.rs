//! The command AUTHORING surface — the write-side twin of the generated
//! call-side accessors.
//!
//! # The goal (scales to all ~260 commands)
//!
//! A command author writes ONE typed method and nothing else:
//!
//! ```ignore
//! #[async_trait]
//! impl CommandHandler for GenerateCommand {
//!     type Spec = GenerateCommandSpec;          // ties NAME/WIRE/Params/Result
//!     async fn execute(&self, ctx: &Ctx, p: GenerateParams)
//!         -> Result<Outcome<GenerateResult>, CommandError>
//!     {
//!         let handle = ctx.handle()?;            // typed envelope accessor
//!         Ok(self.generate(handle, p.request).await?.into())   // `?`, no envelope
//!     }
//! }
//! ```
//!
//! No `serde_json::from_value`, no `match` on the command name, no manual
//! `CommandRequest`/`CommandResponse`, no `into_command_result`, no
//! `success/error` bookkeeping, no `try/catch`. The framework's [`dispatch`]
//! owns all of it: parse the envelope into a typed [`Ctx`] + the typed `Params`,
//! call the author's `execute`, then wrap the result per the command's
//! [`WireShape`] and map any [`CommandError`] onto the transport's refusal
//! channel (which surfaces to the caller as a rejected promise — never a
//! `success:false` field).
//!
//! # One source feeds both sides
//!
//! `CommandHandler::Spec` is the SAME [`CommandSpec`] the generator walks for the
//! typed accessors. So a command is declared once (NAME + WIRE + Params + Result)
//! and that one declaration drives BOTH the generated call-side surface in every
//! language AND the runtime dispatch here — they cannot drift. Change a `Params`
//! type in Rust and both the generated `api.xxx(params)` signature and this
//! handler's `execute` parameter change together.

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use crate::runtime::cell_shapes::HandleRef;
use crate::runtime::command_envelope::{CommandRequest, CommandResponse};
use crate::runtime::CommandResult;

use super::{CommandSpec, WireShape};

/// A typed error from a command handler. `?`-friendly so authors propagate with
/// `?` and never hand-build an error envelope. The framework maps it to the
/// `ServiceModule` refusal string, which the executor turns into
/// `ClientError::Refused` — i.e. a rejected promise on the caller side. The
/// category is preserved as a prefix so logs/clients can classify without
/// parsing prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// Params failed validation or a domain precondition.
    Invalid(String),
    /// A referenced resource (handle, id, session) was not found.
    NotFound(String),
    /// The caller is not permitted to run this command.
    Denied(String),
    /// Anything else — an internal failure.
    Internal(String),
}

impl CommandError {
    fn category(&self) -> &'static str {
        match self {
            CommandError::Invalid(_) => "invalid",
            CommandError::NotFound(_) => "not_found",
            CommandError::Denied(_) => "denied",
            CommandError::Internal(_) => "internal",
        }
    }

    fn message(&self) -> &str {
        match self {
            CommandError::Invalid(m)
            | CommandError::NotFound(m)
            | CommandError::Denied(m)
            | CommandError::Internal(m) => m,
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.category(), self.message())
    }
}

impl std::error::Error for CommandError {}

/// Existing module helpers return `Result<T, String>`; `From<String>` lets an
/// author `?` them straight into a handler without restating the error. A bare
/// string is treated as `Internal` (the author can map to a sharper category
/// when they care).
impl From<String> for CommandError {
    fn from(s: String) -> Self {
        CommandError::Internal(s)
    }
}

impl From<&str> for CommandError {
    fn from(s: &str) -> Self {
        CommandError::Internal(s.to_string())
    }
}

/// The cross-cutting envelope fields a handler may read, typed — never
/// re-parsed by hand. The framework fills this from the inbound
/// [`CommandRequest`] regardless of the command's [`WireShape`], so even a
/// `Bare` command can reach `context_id` (the scoped client stamps it onto the
/// params) without the author parsing JSON.
#[derive(Debug, Clone, Default)]
pub struct Ctx {
    /// A handle threaded from a prior call (long-running session). `None` unless
    /// the caller passed one.
    pub handle: Option<HandleRef>,
    /// Calling session (kernel-injected).
    pub session_id: Option<Uuid>,
    /// Calling user (kernel-injected).
    pub user_id: Option<Uuid>,
    /// Conversation/room scope (client-stamped).
    pub context_id: Option<Uuid>,
    /// WHO is calling — the authenticated caller identity the executor's gate
    /// already saw (`None` = substrate's own local code = owner; `Some(airc …)` =
    /// a persona or a cross-grid peer, verified by airc). Threaded so a handler can
    /// gate/scope BY identity (e.g. `commands/list` showing only what THIS caller
    /// may run) and propagate it when composing other commands — the same identity
    /// that crossed the grid keeps flowing, never escalating.
    pub caller: Option<crate::routing::CallerIdentity>,
}

impl Ctx {
    /// Require a handle (for handle-consuming commands), or a typed `Invalid`
    /// error naming what's missing — so the author writes `ctx.handle()?`
    /// instead of an `ok_or_else(...)` per command.
    pub fn handle(&self) -> Result<HandleRef, CommandError> {
        self.handle
            .clone()
            .ok_or_else(|| CommandError::Invalid("missing required `handle` on the request".into()))
    }
}

/// What a handler returns: the typed output plus an OPTIONAL minted handle. Most
/// commands return `output.into()` (no handle); a handle-MINTING command returns
/// [`Outcome::with_handle`]. Keeping the mint here — rather than a side channel —
/// means the framework, not the author, decides where the handle lands on the
/// wire (the `CommandResponse` envelope's `handle` field).
pub struct Outcome<T> {
    pub output: T,
    pub handle: Option<HandleRef>,
}

impl<T> Outcome<T> {
    /// Output with a freshly minted handle (the long-running-session pattern).
    pub fn with_handle(output: T, handle: HandleRef) -> Self {
        Self {
            output,
            handle: Some(handle),
        }
    }
}

/// Plain output, no handle — the common case, so `Ok(value.into())` just works.
impl<T> From<T> for Outcome<T> {
    fn from(output: T) -> Self {
        Self {
            output,
            handle: None,
        }
    }
}

/// The ONE method a command author writes. Implemented on a deps-holding struct
/// (it borrows whatever shared module state the command needs). `Spec` ties this
/// handler to its [`CommandSpec`] — the same declaration the generator emits the
/// typed accessor from — so the call side and the write side share one source.
#[async_trait]
pub trait CommandHandler: Send + Sync {
    /// The command's declaration (NAME + WIRE + Params + Result).
    type Spec: CommandSpec;

    /// Run the command. Typed params in, typed output out; errors via `?`. The
    /// framework ([`dispatch`]) handles parsing, the envelope, and error mapping.
    async fn execute(
        &self,
        ctx: &Ctx,
        params: <Self::Spec as CommandSpec>::Params,
    ) -> Result<Outcome<<Self::Spec as CommandSpec>::Result>, CommandError>;
}

/// The framework dispatch a module's `handle_command` delegates to — the inverse
/// of the generated accessor. It parses the inbound JSON envelope into a typed
/// [`Ctx`] + the handler's typed `Params`, runs the author's `execute`, then
/// shapes the reply per the command's [`WireShape`]:
///
/// - `Enveloped` → `CommandResponse::ok(output)` (+ minted handle) flattened —
///   the `{success, ...output, handle?}` the caller's `CommandResponse<T>` type
///   expects.
/// - `Bare` / `Provided` → the bare `output`. (A `Bare` command that minted a
///   handle is an authoring bug — there's no envelope to carry it — so it
///   surfaces a loud `Internal` error rather than silently dropping the handle.)
///
/// Params are parsed via [`CommandRequest`] for EVERY shape (it flattens `P` and
/// extracts the envelope siblings), so the envelope fields reach `Ctx` even for
/// `Bare` commands. A [`CommandError`] becomes the `ServiceModule` refusal
/// string (→ `ClientError::Refused` → rejected promise).
pub async fn dispatch<H>(handler: &H, params: serde_json::Value) -> Result<CommandResult, String>
where
    H: CommandHandler,
    <H::Spec as CommandSpec>::Params: DeserializeOwned,
    <H::Spec as CommandSpec>::Result: Serialize,
{
    dispatch_with_caller(handler, params, None).await
}

/// Like [`dispatch`], but threads the authenticated `caller` into [`Ctx`] so the
/// handler can gate/scope by identity and propagate it through composition. The
/// executor passes the caller it already gated on (persona / cross-grid airc
/// sender); local in-process / IPC dispatches pass `None` (owner).
pub async fn dispatch_with_caller<H>(
    handler: &H,
    params: serde_json::Value,
    caller: Option<crate::routing::CallerIdentity>,
) -> Result<CommandResult, String>
where
    H: CommandHandler,
    <H::Spec as CommandSpec>::Params: DeserializeOwned,
    <H::Spec as CommandSpec>::Result: Serialize,
{
    let name = <H::Spec as CommandSpec>::NAME;
    let req = CommandRequest::<<H::Spec as CommandSpec>::Params>::from_value(params)
        .map_err(|e| format!("{name}: [invalid] {e}"))?;
    let ctx = Ctx {
        handle: req.handle,
        session_id: req.session_id,
        user_id: req.user_id,
        context_id: req.context_id,
        caller,
    };

    let outcome = handler
        .execute(&ctx, req.params)
        .await
        .map_err(|e| format!("{name}: {e}"))?;

    match <H::Spec as CommandSpec>::WIRE {
        WireShape::Enveloped => {
            let mut resp = CommandResponse::ok(outcome.output);
            if let Some(handle) = outcome.handle {
                resp = resp.with_handle_ref(handle);
            }
            resp.into_command_result()
                .map_err(|e| format!("{name}: [internal] {e}"))
        }
        WireShape::Bare | WireShape::Provided => {
            if outcome.handle.is_some() {
                return Err(format!(
                    "{name}: [internal] handler minted a handle but the command's wire \
                     shape is Bare/Provided (no envelope to carry it) — declare it \
                     Enveloped or stop minting"
                ));
            }
            CommandResult::json(&outcome.output).map_err(|e| format!("{name}: [internal] {e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::AccessLevel;
    use serde::Deserialize;
    use ts_rs::TS;

    // Minimal typed payloads for exercising the dispatch mechanism (the framework
    // under test), mirroring how command_envelope.rs unit-tests its envelope with
    // small local structs.
    #[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
    struct EchoParams {
        text: String,
    }
    #[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
    struct EchoResult {
        echoed: String,
    }

    // ── A BARE command ────────────────────────────────────────────────
    struct BareSpec;
    impl CommandSpec for BareSpec {
        const NAME: &'static str = "test/echo-bare";
        const ACCESS_LEVEL: AccessLevel = AccessLevel::AiSafe;
        const WIRE: WireShape = WireShape::Bare;
        type Params = EchoParams;
        type Result = EchoResult;
    }
    struct BareHandler;
    #[async_trait]
    impl CommandHandler for BareHandler {
        type Spec = BareSpec;
        async fn execute(
            &self,
            _ctx: &Ctx,
            p: EchoParams,
        ) -> Result<Outcome<EchoResult>, CommandError> {
            Ok(EchoResult { echoed: p.text }.into())
        }
    }

    // ── An ENVELOPED command that consumes a handle + reads context ────
    struct EnvSpec;
    impl CommandSpec for EnvSpec {
        const NAME: &'static str = "test/echo-env";
        const ACCESS_LEVEL: AccessLevel = AccessLevel::AiSafe;
        const WIRE: WireShape = WireShape::Enveloped;
        type Params = EchoParams;
        type Result = EchoResult;
    }
    struct EnvHandler;
    #[async_trait]
    impl CommandHandler for EnvHandler {
        type Spec = EnvSpec;
        async fn execute(
            &self,
            ctx: &Ctx,
            p: EchoParams,
        ) -> Result<Outcome<EchoResult>, CommandError> {
            // Typed accessor — no manual envelope unwrap, no try/catch.
            let handle = ctx.handle()?;
            Ok(EchoResult {
                echoed: format!("{}::{}", handle.type_tag, p.text),
            }
            .into())
        }
    }

    // ── An ENVELOPED command that MINTS a handle ──────────────────────
    struct MintSpec;
    impl CommandSpec for MintSpec {
        const NAME: &'static str = "test/echo-mint";
        const ACCESS_LEVEL: AccessLevel = AccessLevel::AiSafe;
        const WIRE: WireShape = WireShape::Enveloped;
        type Params = EchoParams;
        type Result = EchoResult;
    }
    struct MintHandler;
    #[async_trait]
    impl CommandHandler for MintHandler {
        type Spec = MintSpec;
        async fn execute(
            &self,
            _ctx: &Ctx,
            p: EchoParams,
        ) -> Result<Outcome<EchoResult>, CommandError> {
            let handle = HandleRef::mint("test", "test::Session");
            Ok(Outcome::with_handle(EchoResult { echoed: p.text }, handle))
        }
    }

    // what this catches: a Bare command's reply is the BARE output (no envelope),
    // so the SDK's bare `Result` type matches the wire — the author wrote only the
    // typed execute.
    #[tokio::test]
    async fn dispatch_bare_returns_bare_output() {
        let cr = dispatch(&BareHandler, serde_json::json!({ "text": "hi" }))
            .await
            .expect("dispatch ok");
        match cr {
            CommandResult::Json(v) => {
                assert_eq!(
                    v,
                    serde_json::json!({ "echoed": "hi" }),
                    "bare output, no envelope"
                );
                assert!(
                    v.get("success").is_none(),
                    "Bare must NOT add a success field"
                );
            }
            other => panic!("expected Json, got {other:?}"),
        }
    }

    // what this catches: an Enveloped command's reply is the FLATTENED
    // CommandResponse the caller's `CommandResponse<T>` type expects, AND the
    // envelope (handle + context) reached the typed Ctx so the author read it
    // without parsing JSON.
    #[tokio::test]
    async fn dispatch_enveloped_wraps_and_exposes_ctx() {
        let handle = HandleRef::mint("test", "test::Cursor");
        let params = serde_json::json!({
            "text": "x",
            "handle": serde_json::to_value(&handle).unwrap(),
            "contextId": Uuid::new_v4().to_string(),
        });
        let cr = dispatch(&EnvHandler, params).await.expect("dispatch ok");
        match cr {
            CommandResult::Json(v) => {
                assert_eq!(v["success"], true, "Enveloped flattens success:true");
                assert_eq!(
                    v["echoed"], "test::Cursor::x",
                    "the handle reached Ctx (typed accessor) and the payload flattened"
                );
            }
            other => panic!("expected Json, got {other:?}"),
        }
    }

    // what this catches: a handle-MINTING command's handle lands on the envelope's
    // `handle` field — the framework placed it, the author just returned it.
    #[tokio::test]
    async fn dispatch_enveloped_mint_places_handle_on_wire() {
        let cr = dispatch(&MintHandler, serde_json::json!({ "text": "y" }))
            .await
            .expect("dispatch ok");
        match cr {
            CommandResult::Json(v) => {
                assert_eq!(v["success"], true);
                assert_eq!(v["echoed"], "y");
                assert_eq!(v["handle"]["owner"], "test", "minted handle on the wire");
                assert_eq!(v["handle"]["type_tag"], "test::Session");
            }
            other => panic!("expected Json, got {other:?}"),
        }
    }

    // what this catches: a missing required handle becomes a typed refusal naming
    // the command — surfaced to the caller as a rejected promise, never a
    // success:false result. (ctx.handle()? did this with one line.)
    #[tokio::test]
    async fn dispatch_missing_handle_is_named_refusal() {
        let err = dispatch(&EnvHandler, serde_json::json!({ "text": "x" }))
            .await
            .expect_err("must refuse without a handle");
        assert!(
            err.starts_with("test/echo-env:"),
            "error names the command: {err}"
        );
        assert!(err.contains("invalid"), "carries the category: {err}");
        assert!(err.contains("handle"), "names what's missing: {err}");
    }

    // what this catches: bad params become a named `invalid` refusal at the parse
    // boundary — the author never wrote a parse/try-catch.
    #[tokio::test]
    async fn dispatch_bad_params_is_named_invalid() {
        let err = dispatch(&BareHandler, serde_json::json!({ "text": 123 }))
            .await
            .expect_err("type mismatch must refuse");
        assert!(
            err.starts_with("test/echo-bare: [invalid]"),
            "named + categorized: {err}"
        );
    }
}
