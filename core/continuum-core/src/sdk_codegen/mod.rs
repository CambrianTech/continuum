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
#[cfg(feature = "ts-codegen")]
pub mod emit;
pub mod events;
pub mod handler;
#[cfg(feature = "ts-codegen")]
pub use emit::write_typescript_sdk;
pub use events::{event_registry, EventDescriptor, EventSpec};
#[cfg(feature = "ts-codegen")]
pub use events::{generate_event_api, generate_event_map};
pub use handler::{dispatch, CommandError, CommandHandler, Ctx, Outcome};

/// The capability a command declares.
///
/// PLACEHOLDER — not yet load-bearing. Two honest caveats (adversarial review):
/// (1) this is currently metadata only — `generate_command_map` does NOT emit it,
/// and the runtime gate (`routing::auth_policy`) doesn't read it; it is NOT
/// enforced today. (2) It must be RECONCILED before it is — the substrate already
/// has the spec `accessLevel` vocabulary (`ai-safe`/`privileged`/`internal`/
/// `admin`/`human-only`/`owner-only`/`owner`/`system`) AND grid `TrustLevel`; this
/// 3-variant enum is narrower than the spec set and a parallel vocabulary. Do not
/// treat it as the access contract until it's unified with the real ACL type and
/// actually wired into the gate + the generated surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessLevel {
    /// Safe for autonomous AI callers.
    AiSafe,
    /// Requires elevated/human authorization.
    Privileged,
    /// Substrate-internal; not exposed to external callers.
    Internal,
}

impl AccessLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            AccessLevel::AiSafe => "ai-safe",
            AccessLevel::Privileged => "privileged",
            AccessLevel::Internal => "internal",
        }
    }
}

/// The ACTUAL wire shape a command's handler exchanges — the per-command fact
/// the generated surface must model FAITHFULLY.
///
/// This is NOT "where the work happens" (a routing concern); it is "what JSON the
/// handler parses and returns," which is the only thing the SDK type must match.
/// The earlier `Executed | Provided` framing was wrong: it assumed all
/// substrate commands flow through the envelope, but they don't —
/// `inference/llm/request` returns a BARE payload while `chat/*` and
/// `ai/inference/*` ride the envelope. Modeling those identically produced types
/// that LIED about the wire (the adversarial-review failure). So the dimension is
/// the handler's real convention, declared per command and verified against it:
///
/// - [`Bare`](WireShape::Bare) — a substrate `ServiceModule` parses `Params`
///   directly (`serde_json::from_value::<P>`) and returns `Result` directly
///   (`CommandResult::json(&T)`). The SDK sees exactly `P` in, `T` out. Failure
///   is a rejected promise (`ClientError`), not a `success` field. Worked
///   example: `inference/llm/request`.
///
/// - [`Enveloped`](WireShape::Enveloped) — the handler does
///   `CommandRequest::<P>::from_value` in and
///   `CommandResponse::ok(T).into_command_result()` out, so the inner JSON the
///   transport returns is the FLATTENED `CommandResponse<T>` (`{success, ...T,
///   handle?}`) and the accepted params are the flattened `CommandRequest<P>`
///   (caller may add a `handle`; the SDK stamps `contextId`). The generated
///   surface therefore wraps in the envelope generics — faithfully, because the
///   handler genuinely emits them. Worked examples: `chat/send`, `chat/poll`,
///   `ai/inference/*` (incl. handle mint/consume).
///
/// - [`Provided`](WireShape::Provided) — the substrate CANNOT execute it; it
///   routes the call OUT to a client adapter that holds the capability (browser
///   `html2canvas`, native snapshot, VR framebuffer). The adapter exchanges bare
///   `P`/`T` (the `Commands.provide` signature), so the surface is bare — same
///   TYPE shape as `Bare`, but a different *server* (adapter, not ServiceModule).
///   Worked example: `interface/screenshot` — one name, N platform adapters
///   ([[persona-is-a-client]]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireShape {
    /// Substrate handler; bare `P` in, bare `T` out.
    Bare,
    /// Substrate handler; `CommandRequest<P>` in, `CommandResponse<T>` out.
    Enveloped,
    /// Client-adapter-fulfilled; bare `P` in, bare `T` out.
    Provided,
}

impl WireShape {
    /// Whether the generated surface wraps params/result in the envelope generics.
    /// Only `Enveloped` does — `Bare`/`Provided` exchange the types directly.
    fn is_enveloped(self) -> bool {
        matches!(self, WireShape::Enveloped)
    }
}

/// The single source of truth for ONE command. `Params`/`Result` are ts-rs
/// types — the SAME types a handler parses and the wire types ts-rs emits — so
/// one declaration feeds both runtime dispatch (future) and codegen (here).
pub trait CommandSpec {
    /// The command's URI path (e.g. `"data/list"`).
    const NAME: &'static str;
    /// The capability this command requires.
    const ACCESS_LEVEL: AccessLevel;
    /// The handler's ACTUAL wire convention — decides whether the generated
    /// surface is enveloped or bare. MUST match what the handler really does
    /// (a mismatch is a type that lies about the wire).
    const WIRE: WireShape;
    /// The command-specific params payload (the INNER `P`). When
    /// [`WIRE`](CommandSpec::WIRE) is `Enveloped` the generated surface wraps this
    /// in `CommandRequest<P>`; otherwise it's used directly.
    type Params: TS + 'static;
    /// The command-specific result payload (the INNER `T`). When
    /// [`WIRE`](CommandSpec::WIRE) is `Enveloped` the generated surface wraps this
    /// in `CommandResponse<T>`; otherwise it's used directly.
    type Result: TS + 'static;
}

/// A TS type the generated surface references: its TS name + the module it's
/// imported from (the ts-rs output path, extension dropped). The single-source
/// wire type lives at that module; the map only references it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    pub name: String,
    pub module: String,
}

impl TypeRef {
    fn from_dependency(dep: &Dependency) -> Self {
        Self {
            name: dep.ts_name.clone(),
            module: module_of(&dep.output_path),
        }
    }
}

/// The canonical TypeScript root every wire type ultimately lands under. ts-rs
/// `#[ts(export_to)]` paths are anchored at the export dir and climb out to this
/// root via `../../../` escapes (e.g. `../../../protocol/typescript/ai/ModelInfo.ts`);
/// the importable module is the path RELATIVE to this root.
const TS_ROOT: &str = "protocol/typescript/";

/// Convert a ts-rs `output_path` into an importable module path relative to
/// [`TS_ROOT`]. Real wire types carry the escape-hatch form
/// (`../../../protocol/typescript/ai/ModelInfo.ts`) → `ai/ModelInfo`; a type with
/// no `#[ts(export_to)]` carries a bare derived name (`PingParams.ts`) → its stem.
/// Anchoring on `TS_ROOT` is what makes the generated imports resolve regardless
/// of where ts-rs's export dir sits — the bug the demo types (no `export_to`) hid.
fn module_of(output_path: &Path) -> String {
    let normalized = output_path
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/");
    let root = TS_ROOT.trim_end_matches('/');
    match normalized.find(&format!("{root}/")) {
        Some(idx) => normalized[idx + root.len() + 1..].to_string(),
        // No canonical-root segment: fall back to the bare file stem.
        None => normalized
            .rsplit('/')
            .next()
            .unwrap_or(&normalized)
            .to_string(),
    }
}

/// A runtime-value snapshot of a [`CommandSpec`] the generator walks — captured
/// from the type level (no instances), via [`CommandDescriptor::of`].
#[derive(Debug, Clone)]
pub struct CommandDescriptor {
    pub name: &'static str,
    pub access_level: AccessLevel,
    /// The handler's real wire convention (decides envelope wrapping).
    pub wire: WireShape,
    pub params: TypeRef,
    pub result: TypeRef,
    /// Every TS type the params/result transitively need (themselves + nested
    /// deps), so the generated module imports them all — nothing dangles.
    pub type_refs: Vec<TypeRef>,
}

impl CommandDescriptor {
    /// Snapshot a `CommandSpec` into a descriptor. `Params`/`Result` and their
    /// transitive ts-rs dependencies are collected as importable `TypeRef`s.
    pub fn of<C: CommandSpec>() -> Self {
        let cfg = Config::default();
        let params_dep = Dependency::from_ty::<C::Params>(&cfg)
            .expect("command Params must be a named TS type (struct/enum), not an inline primitive");
        let result_dep = Dependency::from_ty::<C::Result>(&cfg)
            .expect("command Result must be a named TS type (struct/enum), not an inline primitive");
        let params = TypeRef::from_dependency(&params_dep);
        let result = TypeRef::from_dependency(&result_dep);

        let mut type_refs = vec![params.clone(), result.clone()];
        for dep in <C::Params as TS>::dependencies(&cfg) {
            type_refs.push(TypeRef::from_dependency(&dep));
        }
        for dep in <C::Result as TS>::dependencies(&cfg) {
            type_refs.push(TypeRef::from_dependency(&dep));
        }

        Self {
            name: C::NAME,
            access_level: C::ACCESS_LEVEL,
            wire: C::WIRE,
            params,
            result,
            type_refs,
        }
    }
}

impl CommandDescriptor {
    /// The TS type a caller PASSES — the inner params, wrapped in
    /// `CommandRequest<P>` for `Enveloped` commands, bare otherwise.
    fn ts_param_type(&self) -> String {
        if self.wire.is_enveloped() {
            format!("CommandRequest<{}>", self.params.name)
        } else {
            self.params.name.clone()
        }
    }

    /// The TS type a caller RECEIVES — the inner result, wrapped in
    /// `CommandResponse<T>` for `Enveloped` commands, bare otherwise.
    fn ts_result_type(&self) -> String {
        if self.wire.is_enveloped() {
            format!("CommandResponse<{}>", self.result.name)
        } else {
            self.result.name.clone()
        }
    }

    /// The camelCase accessor method name derived from the command path —
    /// `chat/send` → `chatSend`, `ai/inference/open` → `aiInferenceOpen`,
    /// `data/query-next` → `dataQueryNext`. The command STRING lives only inside
    /// the generated method body; call sites use this typed name (no string keys).
    fn accessor_name(&self) -> String {
        let mut out = String::new();
        let mut capitalize = false;
        for ch in self.name.chars() {
            match ch {
                '/' | '-' | '_' => capitalize = true,
                c if capitalize => {
                    out.extend(c.to_uppercase());
                    capitalize = false;
                }
                c => out.push(c),
            }
        }
        out
    }
}

/// The TS module the [`CommandRequest`](crate::runtime::command_envelope::CommandRequest)
/// generic is emitted to by ts-rs (relative to [`TS_ROOT`]). The generated map
/// imports the envelope generics from here for `Enveloped` commands.
const ENVELOPE_REQUEST_MODULE: &str = "runtime/CommandRequest";
/// The TS module the [`CommandResponse`](crate::runtime::command_envelope::CommandResponse)
/// generic is emitted to by ts-rs.
const ENVELOPE_RESPONSE_MODULE: &str = "runtime/CommandResponse";

/// Build the `import type {...}` block every generated surface shares: each
/// command's transitive `type_refs` grouped by module + deduped, plus the
/// envelope generics when any command is `Enveloped`. Returns the import lines
/// (newline-terminated). Centralized so the map and the accessor API can't drift
/// on what they import.
fn render_imports(commands: &[CommandDescriptor], import_base: &str) -> String {
    // Import ONLY the types the surface directly NAMES — each command's top-level
    // params + result. The nested/transitive types are referenced only inside the
    // vendored wire files (which import each other), never here, so importing them
    // would be dead imports (and would break under `noUnusedLocals`). Vendoring
    // (emit.rs) still copies the full closure; that's a separate concern.
    let mut refs: Vec<&TypeRef> = Vec::new();
    for c in commands {
        refs.push(&c.params);
        refs.push(&c.result);
    }
    let mut out = render_import_lines(&refs, import_base);
    // Enveloped commands name the envelope generics; import them once.
    if commands.iter().any(|c| c.wire.is_enveloped()) {
        out.push_str(&format!(
            "import type {{ CommandRequest }} from '{import_base}/{ENVELOPE_REQUEST_MODULE}';\n"
        ));
        out.push_str(&format!(
            "import type {{ CommandResponse }} from '{import_base}/{ENVELOPE_RESPONSE_MODULE}';\n"
        ));
    }
    out
}

/// The shared import-block primitive: a set of `TypeRef`s grouped by module +
/// deduped into `import type { A, B } from '<base>/<module>'` lines (names + lines
/// sorted for stable output). Both the command and event surfaces build on this so
/// they can't drift on import shape.
fn render_import_lines(refs: &[&TypeRef], import_base: &str) -> String {
    let mut by_module: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for t in refs {
        by_module
            .entry(t.module.clone())
            .or_default()
            .insert(t.name.clone());
    }
    let mut out = String::new();
    for (module, names) in &by_module {
        let list = names.iter().cloned().collect::<Vec<_>>().join(", ");
        out.push_str(&format!(
            "import type {{ {list} }} from '{import_base}/{module}';\n"
        ));
    }
    out
}

/// Emit the TypeScript `CommandMap` module from the registry — the typed surface
/// every TS SDK consumer infers from (`Commands.execute<K>(name, params)`). Pure:
/// descriptors in, TS text out, so it's testable without node.
///
/// `import_base` is the path (relative to the generated file) where the ts-rs
/// wire types are emitted — e.g. `"@protocol"` or `"../../../protocol/typescript"`.
pub fn generate_command_map(commands: &[CommandDescriptor], import_base: &str) -> String {
    let mut out = String::new();
    out.push_str(
        "// GENERATED from the Rust command registry (core/continuum-core sdk_codegen).\n\
         // DO NOT EDIT. Source of truth: each command's CommandSpec (name + ts-rs\n\
         // Params/Result + wire shape). Regenerate after a command changes.\n\n",
    );
    out.push_str(&render_imports(commands, import_base));
    out.push('\n');

    out.push_str(
        "/**\n\
         * name -> { params, result }. Generated; the contract is Rust-origin and\n\
         * models the REAL wire each handler exchanges.\n\
         *\n\
         * `Enveloped` commands ride the substrate envelope, so their params are\n\
         * `CommandRequest<P>` and results `CommandResponse<T>` (the flattened\n\
         * success/handle the handler actually emits). `Bare` substrate commands and\n\
         * `Provided` adapter commands exchange their payloads directly. Command\n\
         * FAILURE is a rejected promise (transport error), never a result field.\n\
         */\n",
    );
    out.push_str("export interface CommandMap {\n");
    for c in commands {
        out.push_str(&format!(
            "  '{}': {{ params: {}; result: {} }};\n",
            c.name,
            c.ts_param_type(),
            c.ts_result_type()
        ));
    }
    out.push_str("}\n\n");
    out.push_str("export type CommandName = keyof CommandMap;\n");
    out
}

/// Emit the typed command ACCESSOR API — the surface callers actually use. This
/// is the point of the whole generator ([[lock-uniform-client-early]]): a caller
/// writes `api.chatSend(params)` with `params` and the return value strongly
/// typed straight from the Rust definition — NO string command keys at the call
/// site, NO hand-written wrapper, NO casting. The command string lives exactly
/// once, inside the generated method body. Change a param or result type in Rust,
/// regenerate, and every now-wrong call site fails to compile — the change is
/// TRANSFERRED across the boundary, not left as tech debt.
///
/// `client_module` is where the thin `Commands` facade lives relative to the
/// generated file (e.g. `"./Commands"`); `import_base` is the ts-rs wire-type
/// root (e.g. `"@protocol"`). The accessor delegates to `Commands.execute`, so
/// the literal it passes is still checked against the generated `CommandMap`.
pub fn generate_command_api(
    commands: &[CommandDescriptor],
    import_base: &str,
    client_module: &str,
) -> String {
    let mut out = String::new();
    out.push_str(
        "// GENERATED from the Rust command registry (core/continuum-core sdk_codegen).\n\
         // DO NOT EDIT. Typed accessors — call api.<name>(params), never a string key.\n\n",
    );
    out.push_str(&format!("import {{ Commands }} from '{client_module}';\n"));
    out.push_str(&render_imports(commands, import_base));
    out.push('\n');

    out.push_str(
        "/**\n\
         * Typed command accessors. One method per command, derived from its Rust\n\
         * CommandSpec — inputs and outputs strongly typed, the command string baked\n\
         * in once here so call sites stay string-free. A Rust param/result change\n\
         * regenerates this and breaks now-wrong call sites at compile time.\n\
         */\n",
    );
    out.push_str("export class CommandApi {\n");
    out.push_str("  constructor(private readonly commands: Commands) {}\n");
    for c in commands {
        out.push_str(&format!(
            "\n  /** `{name}` */\n  {accessor}(params: {params}): Promise<{result}> {{\n    \
             return this.commands.execute('{name}', params);\n  }}\n",
            name = c.name,
            accessor = c.accessor_name(),
            params = c.ts_param_type(),
            result = c.ts_result_type(),
        ));
    }
    out.push_str("}\n");
    out
}

/// A self-registered command. Each [`CommandSpec`] impl submits one of these at
/// its OWN declaration site via [`register_command!`]; `inventory` collects them
/// all at link time. The associated `Params`/`Result` types are erased to a
/// runtime `CommandDescriptor` by the captured fn — so the registry is a flat
/// list the generator walks, assembled from declarations scattered across the
/// codebase. No central hand-maintained list: adding a command makes it appear in
/// every generated surface automatically (the anti-drift guarantee, mandatory
/// when commands are infinitely-extensible / written on the fly).
pub struct CommandRegistration {
    descriptor_fn: fn() -> CommandDescriptor,
}

impl CommandRegistration {
    /// Build a registration from a descriptor constructor — the
    /// `|| CommandDescriptor::of::<C>()` the macro supplies.
    pub const fn new(descriptor_fn: fn() -> CommandDescriptor) -> Self {
        Self { descriptor_fn }
    }
}

inventory::collect!(CommandRegistration);

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

/// The command registry — the generator's input, ASSEMBLED from every
/// `register_command!` submission across the crate. Sorted by name so the
/// generated output is deterministic regardless of inventory iteration order.
pub fn command_registry() -> Vec<CommandDescriptor> {
    let mut descriptors: Vec<CommandDescriptor> = inventory::iter::<CommandRegistration>()
        .map(|reg| (reg.descriptor_fn)())
        .collect();
    descriptors.sort_by(|a, b| a.name.cmp(b.name));
    // Hard-fail on a duplicate command NAME. The "no central list" design removes
    // the human backstop that would otherwise catch a collision, so the registry
    // must catch it itself — two CommandSpec impls claiming the same name would
    // silently emit a conflicting/duplicate CommandMap key (a TS error or a silent
    // shadow). Sorted above, so duplicates are adjacent.
    if let Some(dup) = descriptors.windows(2).find(|w| w[0].name == w[1].name) {
        panic!(
            "sdk_codegen: duplicate command NAME '{}' — two CommandSpec impls claim \
             it. Command names must be unique across the whole registry.",
            dup[0].name
        );
    }
    descriptors
}

// No demo/fixture commands: the generator is validated against the REAL
// registered commands (declared via CommandSpec at their own sites, e.g. the
// inference module), whose Params/Result are real ts-rs types carrying the
// production `#[ts(export_to = "../../../protocol/typescript/...")]` escape form.
// (The adversarial review caught that earlier demo fixtures — with no export_to —
// hid the export-path resolution bug; validating on real types is the fix.)

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // what this catches (the review's CRITICAL #4): real ts-rs types carry the
    // `../../../protocol/typescript/...` export escape path; module_of must resolve
    // them to clean importable modules. Earlier demo types (no export_to) hid this.
    #[test]
    fn module_of_strips_the_ts_rs_export_escape_path() {
        assert_eq!(
            module_of(Path::new("../../../protocol/typescript/ai/ModelInfo.ts")),
            "ai/ModelInfo",
            "escape-hatch export_to → clean module relative to the TS root"
        );
        assert_eq!(
            module_of(Path::new(
                "../../../protocol/typescript/inference_llm/InferenceRequest.ts"
            )),
            "inference_llm/InferenceRequest"
        );
        // No export_to (bare derived name) → its stem.
        assert_eq!(module_of(Path::new("PingParams.ts")), "PingParams");
    }

    // what this catches: the Rust-source → TS CommandMap generation end-to-end,
    // validated against REAL registered commands (no fictional fixtures). Their
    // types carry the production export_to form, so this exercises module_of on
    // real data: the generated imports must resolve (NO `../../../` escapes) and
    // reference the single-source modules under the TS root.
    #[test]
    fn generates_command_map_from_the_real_registry() {
        let registry = command_registry();
        assert!(
            !registry.is_empty(),
            "real commands are registered (the sampling)"
        );
        let out = generate_command_map(&registry, "@protocol");

        // CRITICAL: imports resolve — module_of stripped the export escape path.
        assert!(out.contains("import type {"), "imports emitted");
        assert!(
            !out.contains("../../../"),
            "no ts-rs export escape path leaks into the generated imports"
        );
        assert!(out.contains("export interface CommandMap"));
        assert!(out.contains("export type CommandName = keyof CommandMap;"));
        assert!(out.starts_with("// GENERATED from the Rust command registry"));
    }

    // what this catches (the heart of the slice): the THREE wire shapes each
    // produce the FAITHFUL type from one CommandMap, modeling exactly what the real
    // handler exchanges (the lie the prior pass shipped was modeling a bare handler
    // as enveloped). The sampling spans all three:
    //   - Bare      (inference/llm/request): handler returns bare InferenceResponse
    //   - Provided  (interface/screenshot):  adapter exchanges bare types
    //   - Enveloped (chat/send, ai/inference/open): handler rides the envelope
    #[test]
    fn wire_shape_decides_envelope_wrapping_faithfully() {
        let registry = command_registry();
        let out = generate_command_map(&registry, "@protocol");

        // Bare substrate command — bare both sides (NOT wrapped). This is the
        // assertion that would have caught the prior lie.
        assert!(
            out.contains(
                "'inference/llm/request': { params: InferenceRequest; result: InferenceResponse }"
            ),
            "Bare command must NOT be enveloped (it returns bare):\n{out}"
        );
        assert!(
            !out.contains("CommandRequest<InferenceRequest>")
                && !out.contains("CommandResponse<InferenceResponse>"),
            "the envelope must NOT wrap the bare inference command"
        );

        // Provided adapter command — bare both sides.
        assert!(
            out.contains(
                "'interface/screenshot': { params: ScreenshotParams; result: ScreenshotResult }"
            ),
            "Provided command must stay bare:\n{out}"
        );

        // Enveloped substrate commands — wrapped both sides (faithful: the handler
        // really emits CommandResponse + parses CommandRequest).
        assert!(
            out.contains(
                "'chat/send': { params: CommandRequest<ChatSendParams>; \
                 result: CommandResponse<ChatSendResult> }"
            ),
            "Enveloped command must be wrapped:\n{out}"
        );
        assert!(
            out.contains(
                "'ai/inference/open': { params: CommandRequest<OpenParams>; \
                 result: CommandResponse<OpenResult> }"
            ),
            "Enveloped handle-minting command must be wrapped:\n{out}"
        );

        // The envelope generics are imported once (Enveloped commands exist), and
        // those modules carry HandleRef themselves so the map needn't.
        assert!(
            out.contains("import type { CommandRequest } from '@protocol/runtime/CommandRequest';"),
            "envelope request generic imported from its single-source module:\n{out}"
        );
        assert!(
            out.contains(
                "import type { CommandResponse } from '@protocol/runtime/CommandResponse';"
            ),
            "envelope response generic imported:\n{out}"
        );
    }

    // what this catches (Joel's goal): the generated ACCESSOR API gives a typed
    // method per command with the command string baked into the BODY — call sites
    // are string-free and strongly typed straight from the Rust definition. Each
    // shape gets the faithful signature; the param/result types are the same ones
    // the CommandMap models, so a Rust type change propagates here too.
    #[test]
    fn command_api_emits_typed_string_free_accessors() {
        let registry = command_registry();
        let out = generate_command_api(&registry, "@protocol", "./Commands");

        // Enveloped: typed accessor, wrapped signature, string only in the body.
        assert!(
            out.contains(
                "chatSend(params: CommandRequest<ChatSendParams>): \
                 Promise<CommandResponse<ChatSendResult>> {"
            ),
            "enveloped accessor is typed both ends:\n{out}"
        );
        // Bare: typed accessor, bare signature.
        assert!(
            out.contains(
                "inferenceLlmRequest(params: InferenceRequest): Promise<InferenceResponse> {"
            ),
            "bare accessor is typed both ends, no envelope:\n{out}"
        );
        // Provided + multi-segment path camelCasing.
        assert!(
            out.contains(
                "interfaceScreenshot(params: ScreenshotParams): Promise<ScreenshotResult> {"
            ),
            "provided accessor present, path camelCased:\n{out}"
        );
        assert!(
            out.contains("aiInferenceOpen(params: CommandRequest<OpenParams>):"),
            "deep path camelCased (ai/inference/open → aiInferenceOpen):\n{out}"
        );

        // The command STRING appears only inside method bodies (execute call), and
        // the accessor delegates to the typed Commands facade.
        assert!(
            out.contains("return this.commands.execute('chat/send', params);"),
            "string key lives once, inside the body:\n{out}"
        );
        // No leaked escape paths; the facade is imported.
        assert!(!out.contains("../../../"), "no ts-rs escape path leaks");
        assert!(out.contains("import { Commands } from './Commands';"));
    }

    // what this catches: a Bare/Provided-only registry must NOT import or use the
    // envelope generics — the wrapping is conditional on an Enveloped command, so a
    // surface with none stays envelope-free.
    #[test]
    fn non_enveloped_registry_omits_envelope_imports() {
        let bare_and_provided: Vec<CommandDescriptor> = command_registry()
            .into_iter()
            .filter(|d| !d.wire.is_enveloped())
            .collect();
        assert!(
            bare_and_provided.iter().any(|d| d.wire == WireShape::Provided)
                && bare_and_provided.iter().any(|d| d.wire == WireShape::Bare),
            "the sampling has both a Bare and a Provided command"
        );
        let out = generate_command_map(&bare_and_provided, "@protocol");
        // Check imports + usages, not the explanatory JSDoc preamble (which names
        // the generics regardless of shape).
        assert!(
            !out.contains("import type { CommandRequest }")
                && !out.contains("import type { CommandResponse }"),
            "no envelope import when nothing is Enveloped:\n{out}"
        );
        assert!(
            !out.contains("params: CommandRequest<")
                && !out.contains("result: CommandResponse<"),
            "no envelope wrapping in the map entries when nothing is Enveloped:\n{out}"
        );
    }

    // what this catches: the registry self-assembles from register_command! sites
    // (inventory) crate-wide — the real inference command appears though nothing
    // lists it centrally — and is sorted for deterministic output.
    #[test]
    fn registry_is_auto_discovered_with_no_central_list() {
        let names: Vec<&str> = command_registry().iter().map(|d| d.name).collect();
        assert!(
            names.contains(&"inference/llm/request"),
            "a real command, registered at its own site, is auto-discovered"
        );
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "registry sorted by name (deterministic output)");
    }

    // what this catches: a real command round-trips to a descriptor at the type
    // level, carrying name + access + its transitive nested deps as importable refs.
    #[test]
    fn descriptor_captures_real_command_and_nested_deps() {
        let d = command_registry()
            .into_iter()
            .find(|d| d.name == "inference/llm/request")
            .expect("inference/llm/request registered");
        assert_eq!(d.access_level, AccessLevel::AiSafe);
        assert_eq!(d.params.name, "InferenceRequest");
        assert_eq!(d.result.name, "InferenceResponse");
        // InferenceResponse nests InferenceComplete + FirstTokenEmitted — the
        // transitive closure must capture them as importable refs.
        assert!(
            d.type_refs.iter().any(|t| t.name == "InferenceComplete"),
            "nested dependency captured (transitive closure)"
        );
    }
}
