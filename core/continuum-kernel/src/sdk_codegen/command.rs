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
    fn descriptor(&self) -> Result<CommandDescriptor, super::CodegenDefect>;

    /// Parse the JSON envelope → run the typed handler → shape the reply per the
    /// command's [`WireShape`] → map errors to the refusal channel. `caller` is the
    /// authenticated identity the executor already gated on (threaded into [`Ctx`]
    /// so the handler can gate/scope/compose by identity). Internally just
    /// [`dispatch_with_caller`]; the type erasure happens here so the kernel can
    /// call it without knowing `Params`/`Result`. A refusal keeps its typed next moves
    /// ([`super::CommandFailure`]).
    async fn invoke(
        &self,
        params: Value,
        caller: Option<crate::routing::CallerIdentity>,
    ) -> Result<CommandResult, super::CommandFailure>;
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

    fn descriptor(&self) -> Result<CommandDescriptor, super::CodegenDefect> {
        CommandDescriptor::of::<H::Spec>()
    }

    async fn invoke(
        &self,
        params: Value,
        caller: Option<crate::routing::CallerIdentity>,
    ) -> Result<CommandResult, super::CommandFailure> {
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
// REQUIRED of every native tool (ONE-ACT 6b). A command projects by submitting an
// [`OutcomeProjector`] (`register_outcome!`), because the descriptor's
// `of::<C: CommandSpec>()` cannot see that `C` also implements [`ProjectsOutcome`] (Rust
// has no specialization). `action_command!`'s `native: true` submits it itself, so a native
// tool declared through the macro without a `ProjectsOutcome` does not compile; a hand-written
// native command is held to it by [`natives_without_an_outcome`] (= 0). Before
// this, `code/edit` and `code/write` projected nothing, so a refused edit (`success: false`)
// rendered as a success tick (Benchy, the M5, 2026-11-05 on 0b2854327).

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

impl ToolVerdict {
    /// The verdict of a result that reports its own outcome as a `success` flag.
    pub fn of_success(success: bool) -> Self {
        if success {
            ToolVerdict::Succeeded
        } else {
            ToolVerdict::Failed
        }
    }
}

/// What the act seam KNOWS about one call's outcome. Three states, because two
/// would lie: "no projector" and "a projector that could not read its own result"
/// are different facts, and collapsing them is what let schema drift render as a
/// success tick (Astra on #4352).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "verdict")]
pub enum ActVerdict {
    /// The command declares no projection (a catalog command; every native tool projects):
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

/// A command's projection from its own typed result to a [`ToolVerdict`]: required of every
/// native tool.
///
/// The `DeserializeOwned` bound lands HERE rather than on the result type, so only commands
/// that project pay it.
pub trait ProjectsOutcome: CommandSpec
where
    <Self as CommandSpec>::Result: serde::de::DeserializeOwned,
{
    /// Read this command's own typed result. No key scanning, no guessing: the
    /// field being read is one the command already declares and documents.
    fn outcome(output: &<Self as CommandSpec>::Result) -> ToolVerdict;

    /// The long-running handle this result hands back, when it has one.
    ///
    /// Declared by the COMMAND because only the command knows which of its fields
    /// is the handle. Without it the act seam had to re-parse the result text to
    /// find `execution_id` — against the FOLDED preview, and matching the command
    /// name by hand, so a `bash` alias missed it entirely and a flood-sized result
    /// failed to parse. Defaults to none; a command with no handle ignores it.
    fn dispatch_handle(_output: &<Self as CommandSpec>::Result) -> Option<uuid::Uuid> {
        None
    }
}

/// One command's registered projector, keyed by the command name the executor
/// already holds at dispatch.
///
/// ONE function pointer, not two: both facts come from the same `Output`, so they are read
/// in a single decode and returned together (Astra on #4352).
pub struct OutcomeProjector {
    name: &'static str,
    read: fn(&serde_json::Value) -> (ActVerdict, Option<uuid::Uuid>),
}

impl OutcomeProjector {
    /// Build a registration from a command name and a single decode-then-read fn
    /// (what [`crate::register_outcome!`] supplies).
    pub const fn new(
        name: &'static str,
        read: fn(&serde_json::Value) -> (ActVerdict, Option<uuid::Uuid>),
    ) -> Self {
        Self { name, read }
    }
    /// The command this projector speaks for.
    pub fn name(&self) -> &'static str {
        self.name
    }
    /// Decode ONCE and report both the verdict and the command-declared dispatch
    /// handle. Never `Unprojected` — reaching this function means a projector IS
    /// registered, so the only outcomes are the command's own
    /// [`ActVerdict::Declared`] verdict or [`ActVerdict::Undecodable`].
    pub fn read(&self, value: &serde_json::Value) -> (ActVerdict, Option<uuid::Uuid>) {
        (self.read)(value)
    }
}

inventory::collect!(OutcomeProjector);

/// `C`'s projection of a dispatched value: ONE decode of its declared result, both facts from
/// it. A value that does not decode is schema drift ([`ActVerdict::Undecodable`]), never a
/// success. What `register_outcome!(C)` submits.
pub fn read_outcome<C>(value: &Value) -> (ActVerdict, Option<uuid::Uuid>)
where
    C: ProjectsOutcome,
    <C as CommandSpec>::Result: serde::de::DeserializeOwned,
{
    match <<C as CommandSpec>::Result as Deserialize>::deserialize(value) {
        Ok(output) => (ActVerdict::Declared(C::outcome(&output)), C::dispatch_handle(&output)),
        Err(_) => (ActVerdict::Undecodable, None),
    }
}

/// The projection of a command whose result carries no failure state: every failure it has is
/// a refusal (`Err`), so a value it returns is the work done. What
/// `register_outcome!(C, refusals_only)` submits: declared at the registration, like any outcome.
pub fn read_refusals_only(_: &Value) -> (ActVerdict, Option<uuid::Uuid>) {
    (ActVerdict::Declared(ToolVerdict::Succeeded), None)
}

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

/// Every native tool that does not say what its own result means (ONE-ACT 6b), by name: empty
/// is conformant. `action_command!`'s `native: true` cannot appear here (it submits the
/// projection, so a macro-declared native tool without a `ProjectsOutcome` does not compile);
/// this holds the hand-written native commands to the same contract. Without it, `code/edit`'s
/// refused edit (`success: false`) rendered as a success tick (Benchy, the M5, 2026-11-05).
pub fn natives_without_an_outcome() -> Vec<&'static str> {
    super::command_registry()
        .into_iter()
        .filter(|d| d.native && outcome_projector(d.name).is_none())
        .map(|d| d.name)
        .collect()
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
        Some(p) => p.read(value),
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
    /// offered natively AUTOMATICALLY, no central list. See [`CommandSpec::NATIVE`]: a native
    /// command must project its outcome (`register_outcome!`).
    const NATIVE: bool = false;
    /// The trained/former/expected names this command ANSWERS TO — the conventional
    /// tool-call names a model reaches for (`read_file`, `bash`), plus any FORMER
    /// name this command carried before it moved. Declared HERE, on the command
    /// itself, so a command is fully portable: rename/move it and its aliases
    /// travel with it — no central table to keep in sync. Aggregated into one
    /// generated inverse index ([`crate::ai::tool_dialect`]); a name two
    /// commands both claim is a build-time panic. Defaults to none.
    const ALIASES: &'static [&'static str] = &[];
    /// What this command's calls do to the world. See [`CommandSpec::EFFECT`]; defaults to
    /// `Undeclared`.
    const EFFECT: continuum_base::tool_effect::ToolEffect = continuum_base::tool_effect::ToolEffect::Undeclared;
    /// Where this verb sits in a mind's hands. See [`CommandSpec::DISCLOSURE`]; defaults to `Core`.
    const DISCLOSURE: continuum_base::tool_disclosure::Disclosure = continuum_base::tool_disclosure::Disclosure::Core;
    /// For a `Withheld` verb: why, and what to use instead. See [`CommandSpec::WITHHELD`].
    const WITHHELD: Option<continuum_base::tool_disclosure::WithheldGuidance> = None;

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
    const EFFECT: continuum_base::tool_effect::ToolEffect = <T as ActionCommand>::EFFECT;
    const DISCLOSURE: continuum_base::tool_disclosure::Disclosure = <T as ActionCommand>::DISCLOSURE;
    const WITHHELD: Option<continuum_base::tool_disclosure::WithheldGuidance> = <T as ActionCommand>::WITHHELD;
    const WIRE: WireShape = WireShape::Bare;
    type Params = <T as ActionCommand>::Params;
    type Result = <T as ActionCommand>::Output;

    /// Derived AUTOMATICALLY from the params type — the base trait's payoff: every
    /// `ActionCommand` exposes a real param schema to every SDK, no hand-authoring.
    fn params_schema() -> Result<serde_json::Value, serde_json::Error> {
        super::input_schema::<<T as ActionCommand>::Params>()
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

        let d = cmd.descriptor().expect("EchoAction is describable");
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
            cmd.descriptor().expect("CountingAction is describable").access_level,
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

    // what this catches: bad params become a named `arguments` refusal at the
    // erased boundary — the author wrote no parse/try-catch, and the type erasure
    // didn't swallow the categorized error.
    #[tokio::test]
    async fn invoke_maps_bad_params_to_named_refusal() {
        let cmd = EchoAction;
        let err = cmd
            .invoke(serde_json::json!({ "text": { "a": 1, "b": 2 } }), None)
            .await
            .expect_err("an object of two keys has no one reading as a string: it must refuse");
        assert!(
            err.message.starts_with("test/echo-action: [arguments]"),
            "named + categorized: {}",
            err.message
        );
        assert_eq!(err.refusal, None, "an arguments refusal carries its own typed form, not next moves");
    }

    // what this catches (card 9ed04d88): the check a call passes before dispatch reads it
    // against the command's own declared schema and hands back the value to dispatch, so an
    // unambiguous shape runs instead of refusing, and a call that decodes is passed untouched.
    #[test]
    fn the_descriptor_accepts_an_unambiguous_shape_and_returns_what_to_dispatch() {
        let d = CommandDescriptor::of::<EchoAction>().expect("EchoAction is describable");
        let wrapped = serde_json::json!({ "text": { "text": "hi" } });
        assert_eq!(d.accept(&wrapped).expect("one reading").as_ref(), &serde_json::json!({ "text": "hi" })); // test: a refusal here is the failure being caught
        let number = serde_json::json!({ "text": 5 });
        assert_eq!(d.accept(&number).expect("rendered").as_ref(), &serde_json::json!({ "text": "5" })); // test: a refusal here is the failure being caught
        let clean = serde_json::json!({ "text": "hi" });
        assert!(matches!(d.accept(&clean), Ok(std::borrow::Cow::Borrowed(_))));
        assert!(d.accept(&serde_json::json!({ "text": ["hi"] })).is_err(), "a list is not one string");
    }

}
