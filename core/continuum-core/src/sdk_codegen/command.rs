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
use serde::{de::DeserializeOwned, Serialize};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolVerdict {
    Succeeded,
    Failed,
    Running,
}

/// A command's opt-in projection from its own typed output to an [`ToolVerdict`].
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
}

/// One command's registered projector, keyed by the command name the executor
/// already holds at dispatch.
pub struct OutcomeProjector {
    name: &'static str,
    project: fn(&serde_json::Value) -> Option<ToolVerdict>,
}

impl OutcomeProjector {
    /// Build a registration from a command name and a decode-then-project fn (what
    /// [`crate::register_outcome!`] supplies).
    pub const fn new(
        name: &'static str,
        project: fn(&serde_json::Value) -> Option<ToolVerdict>,
    ) -> Self {
        Self { name, project }
    }
    /// The command this projector speaks for.
    pub fn name(&self) -> &'static str {
        self.name
    }
    /// Project a dispatched result. `None` when the value does not decode as this
    /// command's output — an undecodable result is NOT a failure claim, it is an
    /// absence of information, and the caller must keep today's behaviour.
    pub fn project(&self, value: &serde_json::Value) -> Option<ToolVerdict> {
        (self.project)(value)
    }
}

inventory::collect!(OutcomeProjector);

/// The projector for `name`, or `None` when that command has not opted in.
pub fn outcome_projector(name: &str) -> Option<&'static OutcomeProjector> {
    inventory::iter::<OutcomeProjector>
        .into_iter()
        .find(|p| p.name == name)
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
    /// the transport `Err` arm, so a handler that returned its FAILURE AS DATA took
    /// the `Ok` arm and rendered a success tick — `code/run` reporting `ok: false`
    /// with the compiler's stderr, `code/shell` reporting `status: Running` before
    /// the command had finished. These pin the projection that fixes it.
    mod outcome_projection {
        use crate::sdk_codegen::{outcome_projector, ToolVerdict};

        /// what this catches: the projector silently doing NOTHING. It decodes the
        /// value the executor actually holds — `execute_value` returns the handler's
        /// typed `Output` verbatim (`CommandClient::execute` deserializes it straight
        /// into `R`, no envelope), so a shape change that broke the decode would send
        /// every verdict back to `None` and quietly restore the tick.
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
            let p = outcome_projector("code/run").expect("code/run opted in");
            assert_eq!(
                p.project(&value),
                Some(ToolVerdict::Failed),
                "a nonzero exit must project Failed, not a tick"
            );
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
            let p = outcome_projector("code/run").expect("code/run opted in");
            assert_eq!(p.project(&value), Some(ToolVerdict::Succeeded));
        }

        /// what this catches: the ACCEPTED-is-not-COMPLETED case collapsing into a
        /// terminal verdict. `code/shell` documents itself as "always returns
        /// immediately with the execution handle", so `Running` is the COMMON case,
        /// not an edge one — mapping it to Succeeded would re-create the defect for
        /// every async shell call.
        #[test]
        fn an_unfinished_shell_projects_running_not_succeeded() {
            use crate::code::shell_types::{ShellExecuteResponse, ShellExecutionStatus};
            let running = ShellExecuteResponse {
                execution_id: "exec-1".into(),
                status: ShellExecutionStatus::Running,
                stdout: None,
                stderr: None,
                exit_code: None,
            };
            let value = serde_json::to_value(&running).expect("serializes");
            let p = outcome_projector("code/shell").expect("code/shell opted in");
            assert_eq!(p.project(&value), Some(ToolVerdict::Running));
            assert_ne!(p.project(&value), Some(ToolVerdict::Succeeded));
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
                    execution_id: "exec-1".into(),
                    status: status.clone(),
                    stdout: None,
                    stderr: Some("boom".into()),
                    exit_code: Some(2),
                })
                .expect("serializes");
                let ex = outcome_projector("code/shell").expect("registered");
                let poll = outcome_projector("code/shell-poll").expect("registered");
                assert_eq!(ex.project(&value), Some(expected), "{status:?} on execute");
                assert_eq!(poll.project(&value), Some(expected), "{status:?} on poll");
            }
        }

        /// what this catches: a projector inventing a verdict from a payload it does
        /// not understand. An undecodable value is an ABSENCE of information — the
        /// caller must keep today's behaviour, never receive a manufactured failure.
        #[test]
        fn an_undecodable_payload_yields_no_verdict_rather_than_a_failure() {
            let p = outcome_projector("code/run").expect("registered");
            assert_eq!(p.project(&serde_json::json!({"unrelated": true})), None);
        }

        /// what this catches: the whole mechanism being dead for commands that never
        /// opted in. They must project nothing at all, so their behaviour is byte-for
        /// -byte what it was before this change.
        #[test]
        fn a_command_that_did_not_opt_in_has_no_projector() {
            assert!(outcome_projector("code/read").is_none());
            assert!(outcome_projector("definitely/not/a/command").is_none());
        }
    }
}
