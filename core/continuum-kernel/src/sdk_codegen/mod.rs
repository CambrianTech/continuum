//! sdk_codegen — the Rust-rooted SDK generator.
//!
//! **The single source of truth for the command surface is RUST.** A command
//! declares, in ONE place, its [`CommandSpec`] — name + [`WireShape`] + typed
//! `Params`/`Result` (ts-rs types). That ONE declaration drives BOTH sides of the
//! command, so they can never drift:
//!
//! - **Call side** ([`generate_command_api`]) — a typed, string-free accessor per
//!   command (`api.chatSend(params)`, typed in/out, the command string baked in
//!   once). Change a `Params` type in Rust → regenerate → wrong call sites fail to
//!   compile. The change is transferred across the boundary, not left as debt.
//! - **Write side** ([`handler`]) — one typed `execute(ctx, params) -> Result<…>`;
//!   the framework [`dispatch`] owns parse + envelope + error→reject, so authors
//!   write no `from_value`/`match`/`try-catch`/envelope boilerplate.
//!
//! This replaces the inverted status quo (hand-authored TS spec JSONs + the old
//! tsx generator, which made the shallow TS layer the source of truth). Here the
//! deepest layer — the Rust declaration, whose `Params`/`Result` are the SAME
//! ts-rs wire types — is the source; every language SDK is output
//! ([[lock-uniform-client-early]], [[persona-is-a-client]]).
//!
//! ## Wire shape, modeled faithfully
//!
//! A command's [`WireShape`] is the REAL convention its handler uses, declared per
//! command and verified against it (an earlier pass modeling a bare handler as
//! enveloped LIED about the wire — adversarial review caught it): `Bare`
//! (`P`→`T`), `Enveloped` (`CommandRequest<P>`→`CommandResponse<T>`, the envelope
//! single-sourced from Rust via ts-rs `#[ts(flatten)]`), `Provided` (adapter,
//! bare). Command FAILURE is a rejected promise (`ClientError`), never a result
//! field.
//!
//! ## Import / vendor model
//!
//! ts-rs exports each type to its own file. The generated surface IMPORTS each
//! command's params/result + their transitive deps (via [`TS::dependencies`]).
//! [`emit::write_typescript_sdk`] then VENDORS that closure — copying the ts-rs
//! output into the SDK's own `generated/wire/**`, following imports transitively
//! so the tree is closed — so the SDK is self-contained yet single-sourced.
//!
//! ## Status
//!
//! Registry self-assembles via `inventory` (no central list). A diverse REAL
//! sampling is migrated across all three wire shapes (`inference/llm/request`,
//! `interface/screenshot`, `chat/*`, `ai/inference/*`), with the authoring trait
//! proven on the `ai/inference/*` family. Pending: pointing the emitter at the
//! live `sdk/typescript/generated` (needs a TS typecheck + reconciling the stub's
//! event split); the Swift/Kotlin emitters (atop the uniffi facade); reconciling
//! [`AccessLevel`] with the runtime ACL; the remaining ~250 commands.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ts_rs::{Config, Dependency, TS};

/// The legacy TypeScript-SDK emitter — OFF by default (`ts-codegen` feature).
/// Nothing live consumes its output; the headless command framework below is
/// always compiled. See the `ts-codegen` feature note in Cargo.toml.
pub mod ai_help;
pub mod command;
pub mod ext;
pub mod conformance;
#[cfg(feature = "ts-codegen")]
pub mod emit;
pub mod events;
pub mod handler;
pub use command::{
    outcome_projector, project_result, stateless_command_objects, ActVerdict, ActionCommand,
    DynCommand, OutcomeProjector, ProjectsOutcome, StatelessCommand, ToolVerdict,
};
#[cfg(feature = "ts-codegen")]
pub use emit::write_typescript_sdk;
/// What a command does to the world, declared on it (`ActionCommand::EFFECT`); re-exported so
/// `action_command!`'s `effect:` clause resolves in any crate that expands it.
pub use continuum_base::tool_effect::ToolEffect;
pub use events::{event_registry, EventDescriptor, EventSpec};
#[cfg(feature = "ts-codegen")]
pub use events::{generate_event_api, generate_event_map};
pub use handler::{dispatch, dispatch_with_caller, CommandError, CommandHandler, Ctx, Outcome};

/// Self-register a `CommandSpec` type into the auto-discovered registry. ONE line
/// at the command's own declaration site — never a central list:
/// ```ignore
/// register_command!(MyCommand);
/// ```
#[macro_export]
macro_rules! register_command {
    ($cmd:ty) => {
        inventory::submit! {
            $crate::sdk_codegen::CommandRegistration::new(
                || $crate::sdk_codegen::CommandDescriptor::of::<$cmd>(),
            )
        }
    };
}

/// Opt a command into TRUTHFUL tool receipts — one line at the command's own site.
///
/// The command must implement [`sdk_codegen::ProjectsOutcome`]. Without this
/// submission a command's receipts behave exactly as before: the transport arm
/// alone decides, and a failure returned as data still renders a tick.
///
/// ```ignore
/// register_outcome!(CodeRun);
/// ```
#[macro_export]
macro_rules! register_outcome {
    ($cmd:ty) => {
        inventory::submit! {
            $crate::sdk_codegen::OutcomeProjector::new(
                <$cmd as $crate::sdk_codegen::ActionCommand>::NAME,
                |value| {
                    // ONE decode, both facts. A registered projector that cannot
                    // decode its OWN declared output is SCHEMA DRIFT, not an outcome.
                    // It must never collapse into "no projector" (which renders
                    // success) — the payload is preserved and the claim is withheld.
                    match ::serde_json::from_value::<
                        <$cmd as $crate::sdk_codegen::ActionCommand>::Output,
                    >(value.clone())
                    {
                        Ok(o) => (
                            $crate::sdk_codegen::ActVerdict::Declared(
                                <$cmd as $crate::sdk_codegen::ProjectsOutcome>::outcome(&o),
                            ),
                            <$cmd as $crate::sdk_codegen::ProjectsOutcome>::dispatch_handle(&o),
                        ),
                        Err(_) => ($crate::sdk_codegen::ActVerdict::Undecodable, None),
                    }
                },
            )
        }
    };
}

/// Self-register a STATELESS command at its own declaration site — ONE line, no
/// host module. Registers BOTH the static descriptor (for codegen / ACL / the
/// persona tool surface) AND a runtime constructor (so the kernel routes the
/// command name directly to the object). The command type must be `Default`
/// (stateless ⇒ trivially constructible). Dep-holding commands can't use this —
/// they come from a module's `commands()` so their deps get constructed.
/// ```ignore
/// register_stateless_command!(PingCommand);
/// ```
#[macro_export]
macro_rules! register_stateless_command {
    ($cmd:ty) => {
        $crate::register_command!($cmd);
        inventory::submit! {
            $crate::sdk_codegen::StatelessCommand::new(
                || ::std::sync::Arc::new(<$cmd as ::std::default::Default>::default())
                    as ::std::sync::Arc<dyn $crate::sdk_codegen::DynCommand>,
            )
        }
    };
}

/// **The command generator** ([docs/architecture/COMMAND-ORGANIZATION.md] §3).
///
/// One declarative block ⟹ the struct, the [`ActionCommand`](crate::sdk_codegen::ActionCommand)
/// impl (and so, via the blanket impls, `CommandSpec` + `CommandHandler` +
/// `DynCommand`), and — for the stateless form — registration onto the ONE
/// registry. The author writes a doc comment (⟹ `DESCRIPTION`), four fields, and
/// a body. No envelope, no `match` arm, no hand-written `CommandSpec` consts, no
/// `from_value`. This is how every command is authored going forward; porting a
/// legacy `handle_command` arm is "fill the body, drop the arm".
///
/// **Stateless** (no deps ⇒ `Default` ⇒ auto-registered; lives in `commands/<area>.rs`):
/// ```ignore
/// action_command! {
///     /// Health check: confirm the substrate is alive.
///     pub struct Ping;
///     name: "ping",
///     access: AiSafe,
///     params: PingParams,
///     output: PingResult,
///     run(_ctx, _p) => { Ok(PingResult { ok: true }) }
/// }
/// ```
///
/// **Dep-holding** (captures `Arc<State>` ⇒ NOT auto-registered; the owning
/// module's `commands()` constructs it with its deps, and the executor routes the
/// name straight to the object — winning over the legacy prefix arm):
/// ```ignore
/// action_command! {
///     /// Show git status for the caller's workspace.
///     pub struct CodeGitStatus { state: Arc<CodeState> }
///     name: "code/git/status",
///     access: AiSafe,
///     params: GitStatusParams,
///     output: GitStatusResult,
///     run(this, ctx, p) => { /* uses this.state, ctx, p */ }
/// }
/// ```
///
/// The body is a normal block. The `run(this, ctx, p)` clause names the THREE
/// bindings in scope: `this` is the receiver (deps live on `this.state`), `ctx`
/// the [`Ctx`](crate::sdk_codegen::Ctx), `p` the params. (The receiver is named
/// explicitly rather than `self` because a macro-introduced `self` is invisible to
/// the call-site body under macro hygiene — naming it threads call-site hygiene
/// through.) The body returns `Result<Output, CommandError>` — `?` works.
#[macro_export]
macro_rules! action_command {
    // ── Stateless: unit struct, auto-registered onto the ONE registry. ──
    (
        $(#[doc = $doc:literal])*
        $vis:vis struct $cmd:ident;
        name: $name:expr,
        access: $access:ident,
        $(native: $native:expr,)?
        $(aliases: $aliases:expr,)?
        $(effect: $effect:ident,)?
        params: $params:ty,
        output: $output:ty,
        run($this:ident, $ctx:ident, $p:ident) => $body:block
    ) => {
        $(#[doc = $doc])*
        #[derive(::std::default::Default)]
        $vis struct $cmd;
        $crate::action_command!(@impl $cmd, $name, $access, [$($doc)*], [$($native)?], [$($aliases)?], [$($effect)?], $params, $output, $this, $ctx, $p, $body);
        $crate::register_stateless_command!($cmd);
    };

    // ── Dep-holding: struct carries its deps; registered via the owning
    //    module's `commands()` (deps need construction, so no Default). ──
    (
        $(#[doc = $doc:literal])*
        $vis:vis struct $cmd:ident { $($(#[$field_meta:meta])* $field:ident : $fty:ty),+ $(,)? }
        name: $name:expr,
        access: $access:ident,
        $(native: $native:expr,)?
        $(aliases: $aliases:expr,)?
        $(effect: $effect:ident,)?
        params: $params:ty,
        output: $output:ty,
        run($this:ident, $ctx:ident, $p:ident) => $body:block
    ) => {
        $(#[doc = $doc])*
        $vis struct $cmd { $($(#[$field_meta])* pub $field: $fty),+ }
        $crate::action_command!(@impl $cmd, $name, $access, [$($doc)*], [$($native)?], [$($aliases)?], [$($effect)?], $params, $output, $this, $ctx, $p, $body);
        // Register the DESCRIPTOR (type-only — no instance needed) so the command
        // appears in `command_registry()`, the persona tool surface, the ACL, and
        // codegen. Its RUNTIME object is constructed with deps by the owning
        // module's `commands()`; the executor routes the name straight to it.
        $crate::register_command!($cmd);
    };

    // ── Internal: the shared `ActionCommand` impl both forms expand to. ──
    (@impl $cmd:ident, $name:expr, $access:ident, [$($doc:literal)*], [$($native:expr)?], [$($aliases:expr)?], [$($effect:ident)?], $params:ty, $output:ty, $this:ident, $ctx:ident, $p:ident, $body:block) => {
        #[::async_trait::async_trait]
        impl $crate::sdk_codegen::ActionCommand for $cmd {
            const NAME: &'static str = $name;
            const ACCESS: $crate::sdk_codegen::AccessLevel = $crate::sdk_codegen::AccessLevel::$access;
            // Doc comment ⟹ model-facing DESCRIPTION. Each `///` line keeps its
            // leading space, so concatenation separates lines naturally; no docs ⇒ "".
            const DESCRIPTION: &'static str = ::std::concat!($($doc),*);
            // Optional `native: <bool>,` clause ⟹ NATIVE; absent ⇒ false (catalog-only).
            // `false || <expr>` when present, bare `false` when absent — const-valid, no
            // unused bindings.
            const NATIVE: bool = false $(|| $native)?;
            // Optional `aliases: &[...],` clause ⟹ ALIASES (the trained/former names a
            // model reaches for, resolved inbound by tool_dialect). Absent ⇒ the trait
            // default (empty). Emitted only when present, so the default stands otherwise.
            $(const ALIASES: &'static [&'static str] = $aliases;)?
            // Optional `effect: <ToolEffect variant>,` clause ⟹ EFFECT (MIND-CONTEXT invariant 4:
            // only a command that declares it `Observes` can have an unchanged result render as a
            // repeat). Absent ⇒ the trait default, `Undeclared`, which authorizes nothing.
            $(const EFFECT: $crate::sdk_codegen::ToolEffect = $crate::sdk_codegen::ToolEffect::$effect;)?
            type Params = $params;
            type Output = $output;
            async fn run(
                &self,
                $ctx: &$crate::sdk_codegen::Ctx,
                $p: $params,
            ) -> ::std::result::Result<$output, $crate::sdk_codegen::CommandError> {
                // Bind the call-site-named receiver to `&self` so the body can reach
                // deps (`this.state`). Unit-struct (stateless) commands ignore it.
                #[allow(unused_variables)]
                let $this = self;
                $body
            }
        }
    };
}

mod is;
pub use is::*;

// No demo/fixture commands: the generator is validated against the REAL
// registered commands (declared via CommandSpec at their own sites, e.g. the
// inference module), whose Params/Result are real ts-rs types carrying the
// production `#[ts(export_to = "../../../protocol/typescript/...")]` escape form.
// (The adversarial review caught that earlier demo fixtures — with no export_to —
// hid the export-path resolution bug; validating on real types is the fix.)

#[cfg(test)]
mod tests;
