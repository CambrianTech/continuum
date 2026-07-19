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
        assert_eq!(d.access_level, AccessLevel::AiSafe, "ACCESS default carried through");
        assert_eq!(d.description, "Echo the input text back.");
        assert_eq!(d.wire, WireShape::Bare, "ActionCommand is Bare");

        let cr = cmd
            .invoke(serde_json::json!({ "text": "hi" }), None)
            .await
            .expect("invoke ok");
        match cr {
            CommandResult::Json(v) => {
                assert_eq!(v, serde_json::json!({ "echoed": "hi" }), "bare output, no envelope");
                assert!(v.get("success").is_none(), "Bare must not add a success field");
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
        let first = cmd.invoke(serde_json::json!({ "text": "a" }), None).await.unwrap();
        let second = cmd.invoke(serde_json::json!({ "text": "a" }), None).await.unwrap();
        if let (CommandResult::Json(a), CommandResult::Json(b)) = (first, second) {
            assert_eq!(a["echoed"], "a#1");
            assert_eq!(b["echoed"], "a#2", "shared state advanced across calls");
        } else {
            panic!("expected Json results");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2, "deps are owned by the object");
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
        assert!(err.starts_with("test/echo-action: [invalid]"), "named + categorized: {err}");
    }
}
