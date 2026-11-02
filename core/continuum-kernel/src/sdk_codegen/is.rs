use super::*;

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
    pub fn is_enveloped(self) -> bool {
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
    /// One-line, model-facing description of what this command DOES — surfaced
    /// into a persona's DYNAMIC tool surface (`command_registry × AiSafe`,
    /// `cognition::persona_tools`) so the reasoner knows when to call it. This is
    /// HEADLESS metadata (a Rust const), declared by the command itself in its own
    /// file (compartmentalized, like its `ACCESS_LEVEL`) — nothing to do with the
    /// TS emitter. Defaults to empty; the tool projection falls back to a
    /// name-based description when unset, so existing commands need no change.
    const DESCRIPTION: &'static str = "";
    /// Whether this command joins the persona's NATIVE tool surface — the bounded set
    /// handed to the model as first-class structured tool-call *schemas* every turn.
    /// (Distinct from the compact catalog, which already lists EVERY authorized command
    /// by name for on-demand `commands/help` expansion — awareness of all commands is
    /// automatic regardless of this flag.) Declared HERE, by the command in its own
    /// file, so a new command joins the native surface AUTOMATICALLY on registration —
    /// never by editing a central list ([[commands-do-real-work-and-return-receipts-not-promise-slop]]).
    /// Defaults `false` (catalog-only): the native set stays the small core agentic
    /// working set so the schema payload never floods the window — the ~150-tool dump
    /// muted personas (see [`native_tool_specs`](crate::cognition::persona_tools::native_tool_specs)).
    const NATIVE: bool = false;
    /// The trained/former/expected tool-call names this command ANSWERS TO (see
    /// [`ActionCommand::ALIASES`](crate::sdk_codegen::command::ActionCommand)). Declared
    /// by the command in its own file so it stays portable; aggregated into one
    /// generated inverse index. Defaults to none.
    const ALIASES: &'static [&'static str] = &[];
    /// What this command's calls do to the world (MIND-CONTEXT invariant 4): declared HERE,
    /// by the command in its own file, never inferred from its name or output. Defaults to
    /// `Undeclared`, which authorizes nothing: such a call is never a repeat and never taken as
    /// observing, and the replay validator counts it.
    const EFFECT: continuum_base::tool_effect::ToolEffect = continuum_base::tool_effect::ToolEffect::Undeclared;
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

    /// The params' JSON Schema — the canonical, machine-readable input contract
    /// every interface adapts FROM (CLI flags, web forms, mobile pickers, AI tool
    /// `input_schema`). Default `Null` (no schema). The base traits
    /// ([`ActionCommand`](crate::sdk_codegen::ActionCommand), …) override this to
    /// derive it AUTOMATICALLY from the Rust type via `schemars` — so a command
    /// declared (or ported) onto a base trait gains a real schema with no extra
    /// code, and every SDK handles it symmetrically. Manual `CommandSpec` impls
    /// keep `Null` until migrated.
    fn params_schema() -> serde_json::Value {
        serde_json::Value::Null
    }
}

/// Reflect a typed input contract for any command wire shape, including external
/// adapters. Keep schema generation identical for native and provided commands.
pub fn input_schema<P: schemars::JsonSchema>() -> serde_json::Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(P))
        .expect("typed command schema must serialize"); // JUSTIFIED: schemars RootSchema is plain data; converting it to a Value cannot fail
    flatten_singleton_all_of(&mut schema);
    schema
}

/// schemars 0.8 wraps a documented field of a referenced type as
/// `{"allOf": [{"$ref": ...}], "description": ...}`. The engine's grammar converter
/// (llama.cpp json-schema-to-grammar) cannot match a valid value against that shape, so
/// the field is unwritable by a constrained model: from #5059 (2026-10-15) the edit tool's
/// `edit_mode` had it, and no resident made a single code/edit call after it (the M5,
/// captured request). A one-entry `allOf` means "exactly this schema"; folding its entry
/// into the parent says the same thing in the shape every consumer reads.
pub(super) fn flatten_singleton_all_of(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let single = match map.get("allOf") {
                Some(serde_json::Value::Array(all)) if all.len() == 1 && all[0].is_object() => Some(all[0].clone()),
                _ => None,
            };
            if let Some(serde_json::Value::Object(inner)) = single {
                map.remove("allOf");
                for (key, val) in inner {
                    map.entry(key).or_insert(val);
                }
            }
            for child in map.values_mut() {
                flatten_singleton_all_of(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                flatten_singleton_all_of(item);
            }
        }
        _ => {}
    }
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
    pub(super) fn from_dependency(dep: &Dependency) -> Self {
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
pub(super) const TS_ROOT: &str = "protocol/typescript/";

/// Convert a ts-rs `output_path` into an importable module path relative to
/// [`TS_ROOT`]. Real wire types carry the escape-hatch form
/// (`../../../protocol/typescript/ai/ModelInfo.ts`) → `ai/ModelInfo`; a type with
/// no `#[ts(export_to)]` carries a bare derived name (`PingParams.ts`) → its stem.
/// Anchoring on `TS_ROOT` is what makes the generated imports resolve regardless
/// of where ts-rs's export dir sits — the bug the demo types (no `export_to`) hid.
pub(super) fn module_of(output_path: &Path) -> String {
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
    /// Headless, model-facing description (from [`CommandSpec::DESCRIPTION`]) —
    /// what the persona tool surface shows the reasoner. Empty when the command
    /// hasn't declared one yet.
    pub description: &'static str,
    /// The handler's real wire convention (decides envelope wrapping).
    pub wire: WireShape,
    /// Whether the command opts into the persona's native tool surface (from
    /// [`CommandSpec::NATIVE`]). The projection [`native_tool_specs`] filters the
    /// registry on this — so the native set is derived, never a hand-kept list.
    pub native: bool,
    /// The trained/former/expected tool-call names this command answers to (from
    /// [`CommandSpec::ALIASES`]). Aggregated into the one generated inverse index
    /// (`ai::tool_dialect`) that maps a model's reflex name back to this
    /// command. Empty for most commands.
    pub aliases: &'static [&'static str],
    /// What its calls do to the world (from [`CommandSpec::EFFECT`]): the effect an executed
    /// call carries into her act, read from the command that actually ran.
    pub effect: continuum_base::tool_effect::ToolEffect,
    pub params: TypeRef,
    /// The params' JSON Schema (from [`CommandSpec::params_schema`]) — the
    /// canonical input contract every SDK/interface adapts from. `Null` when the
    /// command hasn't declared one yet (manual `CommandSpec` not on a base trait).
    pub params_schema: serde_json::Value,
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
        let params_dep = Dependency::from_ty::<C::Params>(&cfg).unwrap_or_else(|| {
            panic!(
                "command '{}' Params must be a named TS type (struct/enum), not an inline primitive",
                C::NAME
            )
        });
        let result_dep = Dependency::from_ty::<C::Result>(&cfg).unwrap_or_else(|| {
            panic!(
                "command '{}' Result must be a named TS type (struct/enum), not an inline primitive",
                C::NAME
            )
        });
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
            description: C::DESCRIPTION,
            wire: C::WIRE,
            native: C::NATIVE,
            aliases: C::ALIASES,
            effect: C::EFFECT,
            params,
            params_schema: C::params_schema(),
            result,
            type_refs,
        }
    }
}

impl CommandDescriptor {
    /// The TS type a caller PASSES — the inner params, wrapped in
    /// `CommandRequest<P>` for `Enveloped` commands, bare otherwise.
    pub(super) fn ts_param_type(&self) -> String {
        if self.wire.is_enveloped() {
            format!("CommandRequest<{}>", self.params.name)
        } else {
            self.params.name.clone()
        }
    }

    /// The TS type a caller RECEIVES — the inner result, wrapped in
    /// `CommandResponse<T>` for `Enveloped` commands, bare otherwise.
    pub(super) fn ts_result_type(&self) -> String {
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
    pub(super) fn accessor_name(&self) -> String {
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
pub(super) const ENVELOPE_REQUEST_MODULE: &str = "runtime/CommandRequest";

/// The TS module the [`CommandResponse`](crate::runtime::command_envelope::CommandResponse)
/// generic is emitted to by ts-rs.
pub(super) const ENVELOPE_RESPONSE_MODULE: &str = "runtime/CommandResponse";

/// Build the `import type {...}` block every generated surface shares: each
/// command's transitive `type_refs` grouped by module + deduped, plus the
/// envelope generics when any command is `Enveloped`. Returns the import lines
/// (newline-terminated). Centralized so the map and the accessor API can't drift
/// on what they import.
pub(super) fn render_imports(commands: &[CommandDescriptor], import_base: &str) -> String {
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
pub(super) fn render_import_lines(refs: &[&TypeRef], import_base: &str) -> String {
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
    pub descriptor_fn: fn() -> CommandDescriptor,
}

impl CommandRegistration {
    /// Build a registration from a descriptor constructor — the
    /// `|| CommandDescriptor::of::<C>()` the macro supplies.
    pub const fn new(descriptor_fn: fn() -> CommandDescriptor) -> Self {
        Self { descriptor_fn }
    }
}

inventory::collect!(CommandRegistration);

/// The command registry — the generator's input, ASSEMBLED from every
/// `register_command!` submission across the crate. Sorted by name so the
/// generated output is deterministic regardless of inventory iteration order.
///
/// **Built ONCE, then cached.** The inventory is link-time-static and each
/// descriptor's `params_schema()` runs `schemars` reflection — so rebuilding per
/// call (the old behavior) made `commands/list` and the persona tool surface
/// O(commands × reflection) every invocation. The descriptors are
/// after-boot-immutable, so they're computed once into a `OnceLock` and cloned out
/// (a cheap `Vec`/`Value` clone vs. re-reflecting every command). The duplicate-
/// name check + sort happen once, inside the init.
pub fn command_registry() -> Vec<CommandDescriptor> {
    static REGISTRY: std::sync::OnceLock<Vec<CommandDescriptor>> = std::sync::OnceLock::new();
    REGISTRY
        .get_or_init(|| {
            let mut descriptors: Vec<CommandDescriptor> = inventory::iter::<CommandRegistration>()
                .map(|reg| (reg.descriptor_fn)())
                .collect();
            descriptors.sort_by(|a, b| a.name.cmp(b.name));
            // Hard-fail on a duplicate command NAME. The "no central list" design
            // removes the human backstop that would otherwise catch a collision, so
            // the registry must catch it itself — two CommandSpec impls claiming the
            // same name would silently emit a conflicting/duplicate CommandMap key (a
            // TS error or a silent shadow). Sorted above, so duplicates are adjacent.
            if let Some(dup) = descriptors.windows(2).find(|w| w[0].name == w[1].name) {
                panic!(
                    "sdk_codegen: duplicate command NAME '{}' — two CommandSpec impls \
                     claim it. Command names must be unique across the whole registry.",
                    dup[0].name
                );
            }
            descriptors
        })
        .clone()
}

/// Is `name` declared in the descriptor registry?
///
/// The half of a command's declaration that codegen, the ACL, `commands/list` and the
/// persona tool surface all read. `ModuleRegistry::register` asks this of every
/// constructor a module contributes, so a command that declared only one half of its
/// (descriptor, constructor) pair fails LOUD at boot instead of routing invisibly.
///
/// `command_registry()` sorts by name and panics on duplicates, so a binary search is
/// exact — and it reads the SAME list every other consumer reads, which is the point:
/// this must never become a second opinion about what commands exist.
pub fn descriptor_exists(name: &str) -> bool {
    command_registry()
        .binary_search_by(|d| d.name.cmp(name))
        .is_ok()
}
