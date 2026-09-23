//! The command OBJECT layer — the routing-side erasure + the base-trait hierarchy.
//!
//! Two jobs, both about keeping per-command burden near zero (Joel 2026-06-21:
//! "alleviate commands from re-implementing by using hierarchies and abstraction.
//! Less code the better"):
//!
//! 1. **[`DynCommand`]** — the object-safe, type-erased command the kernel can hold
//!    in a flat `name -> Arc<dyn DynCommand>` map and route to DIRECTLY (no
//!    per-module `match` arm, no prefix double-routing). A blanket impl makes EVERY
//!    [`CommandHandler`] a `DynCommand` for free — the routing side and the typed
//!    authoring side share one [`CommandSpec`], so they can't drift.
//!
//! 2. **The base-trait hierarchy** ([`ActionCommand`], and later `QueryCommand` /
//!    `CrudCommand` / `SessionCommand`) — a command shape is a trait with blanket
//!    [`CommandSpec`] + [`CommandHandler`] impls, so *implementing the shape IS
//!    implementing the command*. An [`ActionCommand`] author writes a `run` body
//!    and four associated items; the wire shape, the envelope, the parse, the
//!    error-mapping, and the routable object all come from the blanket impls.
//!
//! The chain of blanket impls is the whole trick:
//! `ActionCommand` ⟹ `CommandSpec` + `CommandHandler` ⟹ `DynCommand`. Declare the
//! shape, get the routable object. See
//! [docs/architecture/COMMAND-ORGANIZATION.md](../../../../docs/architecture/COMMAND-ORGANIZATION.md).

use std::sync::Arc;

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::runtime::CommandResult;

use super::handler::{CommandError, CommandHandler, Ctx, Outcome};
use super::{AccessLevel, CommandDescriptor, CommandSpec, WireShape};

/// The type-erased command object — the unit the kernel routes to DIRECTLY.
///
/// A `DynCommand` is what goes into the boot-time `name -> Arc<dyn DynCommand>`
/// map (built once, read lock-free on the hot path). It captures whatever deps it
/// needs at construction (an `Arc<Shared>`), exactly as the old `GenerateHandler(self)`
/// borrowed the module — owned instead of borrowed, so it can live in the map.
///
/// Authors never implement this by hand: the blanket impl below turns any
/// [`CommandHandler`] into a `DynCommand`, and the base traits turn any command
/// shape into a `CommandHandler`. This trait is the routing seam, not an authoring
/// surface.
#[async_trait]
pub trait DynCommand: Send + Sync {
    /// The routing key (e.g. `"data/list"`) — the command's `CommandSpec::NAME`.
    fn name(&self) -> &'static str;

    /// The codegen / tool-surface / ACL descriptor — delegates to the command's
    /// [`CommandSpec`], so the object map and the static registry describe the
    /// SAME command.
    fn descriptor(&self) -> CommandDescriptor;

    /// Parse the JSON envelope → run the typed handler → shape the reply per the
    /// command's [`WireShape`] → map errors to the refusal channel. `caller` is the
    /// authenticated identity the executor already gated on (threaded into [`Ctx`]
    /// so the handler can gate/scope/compose by identity). Internally just
    /// [`dispatch_with_caller`]; the type erasure happens here so the kernel can
    /// call it without knowing `Params`/`Result`.
    async fn invoke(
        &self,
        params: Value,
        caller: Option<crate::routing::CallerIdentity>,
    ) -> Result<CommandResult, String>;
}

/// Every [`CommandHandler`] IS a routable command object. This is what lets a
/// command be dropped straight into the kernel's command map with no wrapper and
/// no per-module match arm — the routing side comes free from the authoring side.
/// `name`/`descriptor` read the shared [`CommandSpec`]; `invoke` is [`dispatch`].
#[async_trait]
impl<H> DynCommand for H
where
    H: CommandHandler + 'static,
    <H::Spec as CommandSpec>::Params: DeserializeOwned + Send,
    <H::Spec as CommandSpec>::Result: Serialize + Send,
{
    fn name(&self) -> &'static str {
        <H::Spec as CommandSpec>::NAME
    }

    fn descriptor(&self) -> CommandDescriptor {
        CommandDescriptor::of::<H::Spec>()
    }

    async fn invoke(
        &self,
        params: Value,
        caller: Option<crate::routing::CallerIdentity>,
    ) -> Result<CommandResult, String> {
        super::dispatch_with_caller(self, params, caller).await
    }
}

/// A self-registering STATELESS command object — captures no deps, so it can be
/// constructed at link time and dropped straight into the kernel's command map
/// with ZERO host-module ceremony. This is what kills the "every command needs a
/// module to expose it" friction: a stateless command does
/// `register_command!(MyCmd)` (already in the static descriptor registry) and is
/// ALSO live on the runtime typed path via this inventory. Dep-holding commands
/// still come from a module's [`ServiceModule::commands`] (the deps must be
/// constructed somewhere). See docs/architecture/COMMAND-ORGANIZATION.md.
pub struct StatelessCommand {
    ctor: fn() -> Arc<dyn DynCommand>,
}

impl StatelessCommand {
    /// Build a registration from a no-arg constructor (the `|| Arc::new(MyCmd)`
    /// the [`crate::register_stateless_command!`] macro supplies).
    pub const fn new(ctor: fn() -> Arc<dyn DynCommand>) -> Self {
        Self { ctor }
    }
    /// Construct the command object.
    pub fn build(&self) -> Arc<dyn DynCommand> {
        (self.ctor)()
    }
}

inventory::collect!(StatelessCommand);

// ─────────────────────── the tool call's own outcome ───────────────────────
//
// A ✓ in a room receipt used to mean DISPATCHED, not SUCCEEDED. The executor set
// `is_error: Some(true)` ONLY on the transport `Err` arm, so a handler that
// returned failure AS DATA — `ok: false`, `status: Failed`, a stderr and an exit
// code — took the `Ok` arm and rendered a tick. Measured 2026-09-22 (card
// f6c50a49, filed 09-15 and then read past for a night): three agents each built
// a hypothesis about why a citizen was not working, on receipts where every one
// of her failures showed as a success.
//
// The fix is NOT key-scanning the JSON — no handler owes the executor a field
// name — and NOT forcing failures onto `Err`, which would throw away the very
// payload the model needs to recover (for `code/run` that payload IS the compiler
// stderr). The command PROJECTS its own typed result into an outcome.
//
// Opt-in BY SUBMISSION, the same way `StatelessCommand` above opts in: the
// descriptor's `of::<C: CommandSpec>()` is generic over the base trait and cannot
// see that `C` also implements [`ProjectsOutcome`] (Rust has no specialization),
// so participation is a second inventory entry rather than a type test. A command
// that submits nothing keeps today's behaviour byte for byte.

/// What a tool call actually did, as its OWN typed result reports it.
///
/// `Running` is not decoration: `code/shell` returns `ShellExecutionStatus::Running`
/// immediately by design ("always returns immediately with the execution handle"),
/// so every async shell call rendered a success tick for work that had not
/// finished. Accepted is not completed, and a receipt must be able to say so.
// Deliberately NOT a ts-rs export yet: the desktop consumes the act receipt's
// `state` string (see `PersonaActUpdate`), not this enum, and adding a binding
// here would require regenerating `protocol/typescript` in the same change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolVerdict {
    Succeeded,
    Failed,
    Running,
}

/// What the act seam KNOWS about one call's outcome. Three states, because two
/// would lie: "no projector" and "a projector that could not read its own result"
/// are different facts, and collapsing them is what let schema drift render as a
/// success tick (Astra on #4352).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "verdict")]
pub enum ActVerdict {
    /// The command never opted in. Byte-for-byte the pre-projection behaviour —
    /// the transport arm alone decides, and nothing here claims anything.
    #[default]
    Unprojected,
    /// The command read its own declared result and said so.
    Declared(ToolVerdict),
    /// A projector IS registered and the dispatched value did not decode as the
    /// command's own declared `Output`. That is SCHEMA DRIFT — a defect — not an
    /// outcome, and it must never be quietly rendered as success. The payload is
    /// preserved untouched; only the claim is withheld.
    Undecodable,
}

impl ActVerdict {
    /// The one question the `is_error` bool can answer. `Running` is deliberately
    /// NOT a failure, and `Undecodable` is deliberately NOT a success — the latter
    /// rides the typed field to the receipt instead of being flattened here.
    pub fn failed(self) -> bool {
        matches!(self, ActVerdict::Declared(ToolVerdict::Failed))
    }
    /// True only where the command itself asserted completion. Used by receipts
    /// that must not render a tick for work that has not finished or for a result
    /// nobody could read.
    pub fn succeeded(self) -> bool {
        matches!(self, ActVerdict::Declared(ToolVerdict::Succeeded))
    }
    /// Accepted-but-unfinished: a handle came back and the work is still running.
    pub fn running(self) -> bool {
        matches!(self, ActVerdict::Declared(ToolVerdict::Running))
    }
    /// A short stable label for receipts and probes.
    pub fn label(self) -> &'static str {
        match self {
            ActVerdict::Unprojected => "unprojected",
            ActVerdict::Declared(ToolVerdict::Succeeded) => "succeeded",
            ActVerdict::Declared(ToolVerdict::Failed) => "failed",
            ActVerdict::Declared(ToolVerdict::Running) => "running",
            ActVerdict::Undecodable => "undecodable",
        }
    }
}

/// A command's opt-in projection from its own typed output to a [`ToolVerdict`].
///
/// The `DeserializeOwned` bound lands HERE rather than on [`ActionCommand::Output`],
/// so only commands that opt in pay it — every other command's output keeps the
/// `TS + Serialize + Send` it has today.
pub trait ProjectsOutcome: ActionCommand
where
    Self::Output: serde::de::DeserializeOwned,
{
    /// Read this command's own typed result. No key scanning, no guessing: the
    /// field being read is one the command already declares and documents.
    fn outcome(output: &Self::Output) -> ToolVerdict;

    /// The long-running handle this result hands back, when it has one.
    ///
    /// Declared by the COMMAND because only the command knows which of its fields
    /// is the handle. Without it the act seam had to re-parse the result text to
    /// find `execution_id` — against the FOLDED preview, and matching the command
    /// name by hand, so a `bash` alias missed it entirely and a flood-sized result
    /// failed to parse. Defaults to none; a command with no handle ignores it.
    fn dispatch_handle(_output: &Self::Output) -> Option<uuid::Uuid> {
        None
    }
}

/// One command's registered projector, keyed by the command name the executor
/// already holds at dispatch.
pub struct OutcomeProjector {
    name: &'static str,
    project: fn(&serde_json::Value) -> ActVerdict,
    handle: fn(&serde_json::Value) -> Option<uuid::Uuid>,
}

impl OutcomeProjector {
    /// Build a registration from a command name, a decode-then-project fn and a
    /// decode-then-handle fn (what [`crate::register_outcome!`] supplies).
    pub const fn new(
        name: &'static str,
        project: fn(&serde_json::Value) -> ActVerdict,
        handle: fn(&serde_json::Value) -> Option<uuid::Uuid>,
    ) -> Self {
        Self {
            name,
            project,
            handle,
        }
    }
    /// The command this projector speaks for.
    pub fn name(&self) -> &'static str {
        self.name
    }
    /// Project a dispatched result. Never `Unprojected` — reaching this function
    /// means a projector IS registered, so the only outcomes are the command's own
    /// [`ActVerdict::Declared`] verdict or [`ActVerdict::Undecodable`].
    pub fn project(&self, value: &serde_json::Value) -> ActVerdict {
        (self.project)(value)
    }
    /// The long-running handle this result carries, when the command declares one.
    pub fn dispatch_handle(&self, value: &serde_json::Value) -> Option<uuid::Uuid> {
        (self.handle)(value)
    }
}

inventory::collect!(OutcomeProjector);

/// The projector for `name`, or `None` when that command has not opted in.
///
/// Callers MUST pass the CANONICAL name (`tool_dialect::resolve_wire_name`), never
/// the raw wire name: a model reaching for `bash` or `code_run` would otherwise
/// miss its own command's projector.
pub fn outcome_projector(name: &str) -> Option<&'static OutcomeProjector> {
    inventory::iter::<OutcomeProjector>
        .into_iter()
        .find(|p| p.name == name)
}

/// The act seam's ONE read of a dispatched result: resolve the canonical name,
/// project if the command opted in, and say which of the three states this is.
/// Returns the verdict and the command-declared dispatch handle together, because
/// both come from the SAME decode of the SAME pre-fold value.
pub fn project_result(
    canonical_name: &str,
    value: &serde_json::Value,
) -> (ActVerdict, Option<uuid::Uuid>) {
    match outcome_projector(canonical_name) {
        Some(p) => (p.project(value), p.dispatch_handle(value)),
        None => (ActVerdict::Unprojected, None),
    }
}

/// Every stateless command object, assembled from `register_stateless_command!`
/// submissions across the crate — the kernel seeds its command map with these at
/// startup (no module needed). Sorted by name for deterministic order.
pub fn stateless_command_objects() -> Vec<Arc<dyn DynCommand>> {
    let mut objs: Vec<Arc<dyn DynCommand>> = inventory::iter::<StatelessCommand>()
        .map(|s| s.build())
        .collect();
    objs.sort_by(|a, b| a.name().cmp(b.name()));
    objs
}

/// A fire-and-forget verb: typed params in, typed output out, no handle, runs
/// locally on whichever node holds it. The most common command shape — `ping`,
/// `grid/pair`, `interface/screenshot`, most `*/run` verbs.
///
/// Implementing `ActionCommand` gives you [`CommandSpec`] (with `WIRE = Bare`),
/// [`CommandHandler`], and [`DynCommand`] via the blanket impls below — so the
/// author writes ONLY [`run`](ActionCommand::run) plus four associated items. The
/// command's deps live on `Self` (captured at construction); `run` borrows them
/// through `&self`, exactly like a method.
///
/// Cross-cutting policy is declared, not re-implemented: `ACCESS` defaults to
/// [`AccessLevel::AiSafe`] (open to autonomous callers) — a command tightens it by
/// overriding the const, never by hand-writing a gate.
#[async_trait]
pub trait ActionCommand: Send + Sync + Sized + 'static {
    /// The command's URI path (e.g. `"ping"`).
    const NAME: &'static str;
    /// Required capability. Defaults to `AiSafe`; override to tighten.
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    /// Model-facing one-liner surfaced into the persona tool surface. Defaults
    /// empty (falls back to a name-based description).
    const DESCRIPTION: &'static str = "";
    /// Whether this command joins the persona's NATIVE tool surface (the bounded set
    /// given as full structured tool-call schemas every turn). Defaults `false`
    /// (catalog-only); a core agentic command overrides it to `true` — and then it's
    /// offered natively AUTOMATICALLY, no central list. See [`CommandSpec::NATIVE`].
    const NATIVE: bool = false;
    /// The trained/former/expected names this command ANSWERS TO — the conventional
    /// tool-call names a model reaches for (`read_file`, `bash`), plus any FORMER
    /// name this command carried before it moved. Declared HERE, on the command
    /// itself, so a command is fully portable: rename/move it and its aliases
    /// travel with it — no central table to keep in sync. Aggregated into one
    /// generated inverse index ([`crate::cognition::tool_dialect`]); a name two
    /// commands both claim is a build-time panic. Defaults to none.
    const ALIASES: &'static [&'static str] = &[];

    /// The typed request payload (a ts-rs wire type). `JsonSchema` so its schema
    /// is derived automatically (no hand-authoring) and exposed to every SDK.
    type Params: TS + DeserializeOwned + schemars::JsonSchema + Send + 'static;
    /// The typed response payload (a ts-rs wire type).
    type Output: TS + Serialize + Send + 'static;

    /// The ONE method an author writes. Typed params in, typed output out; errors
    /// via `?`. The framework owns parse, envelope, wire-shaping, and routing.
    async fn run(&self, ctx: &Ctx, params: Self::Params) -> Result<Self::Output, CommandError>;
}

/// `ActionCommand` ⟹ `CommandSpec` (Bare wire). The action's `Params`/`Output`
/// become the spec's `Params`/`Result`; the consts carry straight through.
impl<T: ActionCommand> CommandSpec for T {
    const NAME: &'static str = <T as ActionCommand>::NAME;
    const ACCESS_LEVEL: AccessLevel = <T as ActionCommand>::ACCESS;
    const DESCRIPTION: &'static str = <T as ActionCommand>::DESCRIPTION;
    const NATIVE: bool = <T as ActionCommand>::NATIVE;
    const ALIASES: &'static [&'static str] = <T as ActionCommand>::ALIASES;
    const WIRE: WireShape = WireShape::Bare;
    type Params = <T as ActionCommand>::Params;
    type Result = <T as ActionCommand>::Output;

    /// Derived AUTOMATICALLY from the params type — the base trait's payoff: every
    /// `ActionCommand` exposes a real param schema to every SDK, no hand-authoring.
    fn params_schema() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(<T as ActionCommand>::Params))
            .unwrap_or(serde_json::Value::Null)
    }
}

/// `ActionCommand` ⟹ `CommandHandler`. The handler IS the action object (`Spec =
/// Self`); `execute` wraps `run`'s output into a handle-less [`Outcome`]. Combined
/// with the `DynCommand` blanket above, an `ActionCommand` is a routable object
/// with no extra code.
#[async_trait]
impl<T: ActionCommand> CommandHandler for T {
    type Spec = T;

    async fn execute(
        &self,
        ctx: &Ctx,
        params: <T as ActionCommand>::Params,
    ) -> Result<Outcome<<T as ActionCommand>::Output>, CommandError> {
        Ok(self.run(ctx, params).await?.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, schemars::JsonSchema)]
    struct EchoParams {
        text: String,
    }
    #[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
    struct EchoResult {
        echoed: String,
    }

    // ── Outlier A: a STATELESS action (captures no deps) ──────────────────
    // Mirrors `ping` — the abstraction must fit a command with zero state.
    struct EchoAction;
    #[async_trait]
    impl ActionCommand for EchoAction {
        const NAME: &'static str = "test/echo-action";
        const DESCRIPTION: &'static str = "Echo the input text back.";
        type Params = EchoParams;
        type Output = EchoResult;
        async fn run(&self, _ctx: &Ctx, p: EchoParams) -> Result<EchoResult, CommandError> {
            Ok(EchoResult { echoed: p.text })
        }
    }

    // ── Outlier B: a STATEFUL, dep-holding action ─────────────────────────
    // Maximally different from A: owns shared state (an Arc'd counter) captured
    // at construction, exactly how a real command captures `Arc<Store>`. Proves
    // the same trait fits both extremes WITHOUT forcing (CLAUDE.md §methodical).
    struct CountingAction {
        calls: Arc<AtomicU32>,
    }
    #[async_trait]
    impl ActionCommand for CountingAction {
        const NAME: &'static str = "test/counting-action";
        // Tighten access — proves the policy const is per-command, not fixed.
        const ACCESS: AccessLevel = AccessLevel::Privileged;
        type Params = EchoParams;
        type Output = EchoResult;
        async fn run(&self, _ctx: &Ctx, p: EchoParams) -> Result<EchoResult, CommandError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(EchoResult {
                echoed: format!("{}#{n}", p.text),
            })
        }
    }

    // what this catches: the blanket chain (ActionCommand ⟹ CommandSpec ⟹
    // CommandHandler ⟹ DynCommand) actually composes — a stateless action, with
    // ONLY a `run` body, is a routable DynCommand whose name/descriptor come from
    // the shared spec and whose invoke returns the BARE output (Bare wire, no
    // envelope). This is the "less code" guarantee made executable.
    #[tokio::test]
    async fn stateless_action_is_a_routable_bare_command() {
        let cmd = EchoAction;
        assert_eq!(DynCommand::name(&cmd), "test/echo-action");

        let d = cmd.descriptor();
        assert_eq!(d.name, "test/echo-action");
        assert_eq!(
            d.access_level,
            AccessLevel::AiSafe,
            "ACCESS default carried through"
        );
        assert_eq!(d.description, "Echo the input text back.");
        assert_eq!(d.wire, WireShape::Bare, "ActionCommand is Bare");

        let cr = cmd
            .invoke(serde_json::json!({ "text": "hi" }), None)
            .await
            .expect("invoke ok");
        match cr {
            CommandResult::Json(v) => {
                assert_eq!(
                    v,
                    serde_json::json!({ "echoed": "hi" }),
                    "bare output, no envelope"
                );
                assert!(
                    v.get("success").is_none(),
                    "Bare must not add a success field"
                );
            }
            other => panic!("expected Json, got {other:?}"),
        }
    }

    // what this catches: a dep-holding action captures its Arc'd state at
    // construction and routes identically — the deps are OWNED by the object, so it
    // can live in the kernel's command map (the property that lets the per-module
    // match arm die). Also proves ACCESS is per-command (Privileged here).
    #[tokio::test]
    async fn stateful_action_owns_its_deps_and_routes() {
        let calls = Arc::new(AtomicU32::new(0));
        let cmd: Arc<dyn DynCommand> = Arc::new(CountingAction {
            calls: calls.clone(),
        });
        assert_eq!(cmd.name(), "test/counting-action");
        assert_eq!(
            cmd.descriptor().access_level,
            AccessLevel::Privileged,
            "per-command ACCESS override is honored"
        );

        // Two invokes through the type-erased object hit the captured state.
        let first = cmd
            .invoke(serde_json::json!({ "text": "a" }), None)
            .await
            .unwrap();
        let second = cmd
            .invoke(serde_json::json!({ "text": "a" }), None)
            .await
            .unwrap();
        if let (CommandResult::Json(a), CommandResult::Json(b)) = (first, second) {
            assert_eq!(a["echoed"], "a#1");
            assert_eq!(b["echoed"], "a#2", "shared state advanced across calls");
        } else {
            panic!("expected Json results");
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "deps are owned by the object"
        );
    }

    // what this catches: bad params become a named `invalid` refusal at the
    // erased boundary — the author wrote no parse/try-catch, and the type erasure
    // didn't swallow the categorized error.
    #[tokio::test]
    async fn invoke_maps_bad_params_to_named_refusal() {
        let cmd = EchoAction;
        let err = cmd
            .invoke(serde_json::json!({ "text": 123 }), None)
            .await
            .expect_err("type mismatch must refuse");
        assert!(
            err.starts_with("test/echo-action: [invalid]"),
            "named + categorized: {err}"
        );
    }

    /// The ✓-on-failure defect (card f6c50a49). The executor set `is_error` only on
    /// the transport `Err` arm, so a handler that returned its failure AS DATA took
    /// the `Ok` arm and rendered a success tick — `code/run` reporting `ok: false`
    /// with the compiler's stderr, `code/shell` reporting `status: Running` before
    /// the command had finished. These pin the projection that fixes it.
    mod outcome_projection {
        use crate::sdk_codegen::{project_result, ActVerdict, ToolVerdict};

        /// what this catches: the projector silently doing NOTHING. It decodes the
        /// value the executor actually holds — `execute_value` returns the handler's
        /// typed `Output` verbatim (`CommandClient::execute` deserializes it straight
        /// into `R`, no envelope), so a shape change that broke the decode would send
        /// every verdict back to a tick.
        #[test]
        fn a_failing_run_projects_failed_from_its_own_typed_output() {
            let failed = crate::commands::code::run::CodeRunResult {
                exit_code: Some(1),
                ok: false,
                stdout: String::new(),
                stderr: "error[E0425]: cannot find value `x`".into(),
                duration_ms: 12,
                timed_out: false,
                interpreter: "rustc".into(),
            };
            let value = serde_json::to_value(&failed).expect("serializes");
            let (verdict, handle) = project_result("code/run", &value);
            assert_eq!(verdict, ActVerdict::Declared(ToolVerdict::Failed));
            assert!(verdict.failed(), "a nonzero exit must read as failed");
            assert_eq!(handle, None, "code/run hands back no running handle");
        }

        /// what this catches: a run that DID succeed being flagged as an error — the
        /// opposite regression, which would feed the model a false failure.
        #[test]
        fn a_clean_run_projects_succeeded() {
            let ok = crate::commands::code::run::CodeRunResult {
                exit_code: Some(0),
                ok: true,
                stdout: "hello".into(),
                stderr: String::new(),
                duration_ms: 3,
                timed_out: false,
                interpreter: "python3".into(),
            };
            let value = serde_json::to_value(&ok).expect("serializes");
            let (verdict, _) = project_result("code/run", &value);
            assert_eq!(verdict, ActVerdict::Declared(ToolVerdict::Succeeded));
            assert!(!verdict.failed());
        }

        /// what this catches: the ACCEPTED-is-not-COMPLETED case collapsing into a
        /// terminal verdict. `code/shell` documents itself as "always returns
        /// immediately with the execution handle", so `Running` is the COMMON case,
        /// not an edge one — mapping it to Succeeded would re-create the defect for
        /// every async shell call. Also pins that the handle rides along, because the
        /// act seam registers the dispatch from it instead of re-parsing the text.
        #[test]
        fn an_unfinished_shell_projects_running_and_carries_its_handle() {
            use crate::code::shell_types::{ShellExecuteResponse, ShellExecutionStatus};
            let id = uuid::Uuid::new_v4();
            let running = ShellExecuteResponse {
                execution_id: id.to_string(),
                status: ShellExecutionStatus::Running,
                stdout: None,
                stderr: None,
                exit_code: None,
            };
            let value = serde_json::to_value(&running).expect("serializes");
            let (verdict, handle) = project_result("code/shell", &value);
            assert_eq!(verdict, ActVerdict::Declared(ToolVerdict::Running));
            assert!(verdict.running() && !verdict.failed() && !verdict.succeeded());
            assert_eq!(handle, Some(id), "the command declares its own handle");
        }

        /// what this catches: the poll half drifting from the execute half. They
        /// share one `ShellExecuteResponse` and must share one reading of it, or a
        /// single execution could be Failed on one seam and Succeeded on the other.
        #[test]
        fn shell_and_shell_poll_read_the_same_execution_the_same_way() {
            use crate::code::shell_types::{ShellExecuteResponse, ShellExecutionStatus};
            for (status, expected) in [
                (ShellExecutionStatus::Running, ToolVerdict::Running),
                (ShellExecutionStatus::Completed, ToolVerdict::Succeeded),
                (ShellExecutionStatus::Failed, ToolVerdict::Failed),
                (ShellExecutionStatus::TimedOut, ToolVerdict::Failed),
                (ShellExecutionStatus::Killed, ToolVerdict::Failed),
            ] {
                let value = serde_json::to_value(ShellExecuteResponse {
                    execution_id: uuid::Uuid::new_v4().to_string(),
                    status: status.clone(),
                    stdout: None,
                    stderr: Some("boom".into()),
                    exit_code: Some(2),
                })
                .expect("serializes");
                assert_eq!(
                    project_result("code/shell", &value).0,
                    ActVerdict::Declared(expected),
                    "{status:?} on execute"
                );
                assert_eq!(
                    project_result("code/shell-poll", &value).0,
                    ActVerdict::Declared(expected),
                    "{status:?} on poll"
                );
            }
        }

        /// what this catches — and this test REPLACES one that codified the bug
        /// (Astra, #4352 review): a REGISTERED projector whose own declared output no
        /// longer decodes must NOT be indistinguishable from an unregistered command.
        /// Collapsing both into "no verdict" is what let schema drift render as a
        /// success tick. `Undecodable` is a third state precisely so the receipt can
        /// refuse to claim anything.
        #[test]
        fn a_registered_projector_that_cannot_decode_says_so_instead_of_succeeding() {
            let (verdict, handle) =
                project_result("code/run", &serde_json::json!({"unrelated": true}));
            assert_eq!(verdict, ActVerdict::Undecodable);
            assert_ne!(
                verdict,
                ActVerdict::Unprojected,
                "schema drift must not masquerade as a command that never opted in"
            );
            assert!(
                !verdict.succeeded(),
                "a result nobody could decode is NOT a success"
            );
            assert!(
                !verdict.failed(),
                "nor is it a failure — the payload is kept and no claim is made"
            );
            assert_eq!(handle, None);
        }

        /// what this catches: the whole mechanism being dead for commands that never
        /// opted in. They must project `Unprojected`, so their behaviour is
        /// byte-for-byte what it was before this change.
        #[test]
        fn a_command_that_did_not_opt_in_is_unprojected_not_undecodable() {
            let v = project_result("code/read", &serde_json::json!({"anything": 1})).0;
            assert_eq!(v, ActVerdict::Unprojected);
            assert!(!v.failed() && !v.running());
            assert_eq!(
                project_result("definitely/not/a/command", &serde_json::json!({})).0,
                ActVerdict::Unprojected
            );
        }

        /// what this catches: a projector keyed on the RAW wire name. The model does
        /// not always send the canonical one — `code/shell` declares `bash` among its
        /// aliases and small tiers emit `code_run` — and a lookup that missed those
        /// would leave the defect fully alive for exactly the calls that hit it most.
        #[test]
        fn the_lookup_is_by_canonical_name_so_aliases_resolve() {
            use crate::cognition::tool_dialect::resolve_wire_name;
            assert_eq!(resolve_wire_name("bash"), "code/shell");
            assert_eq!(resolve_wire_name("code_run"), "code/run");
            let failed = crate::commands::code::run::CodeRunResult {
                exit_code: Some(2),
                ok: false,
                stdout: String::new(),
                stderr: "boom".into(),
                duration_ms: 1,
                timed_out: false,
                interpreter: "python3".into(),
            };
            let value = serde_json::to_value(&failed).expect("serializes");
            assert_eq!(
                project_result(&resolve_wire_name("code_run"), &value).0,
                ActVerdict::Declared(ToolVerdict::Failed),
                "the underscore form must reach its own command's projector"
            );
        }
    }
}
