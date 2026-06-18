//! sdk_codegen — the Rust-rooted SDK generator.
//!
//! **The single source of truth for the command surface is RUST.** A command
//! declares, in ONE place, its name + typed `Params`/`Result` (ts-rs types) +
//! access level via [`CommandSpec`]. The generator walks the registry and emits
//! the per-language SDK surface (here: the TypeScript `CommandMap`) FROM that one
//! declaration — so the typed surface cannot drift from the commands. Generation
//! is what keeps every SDK consistent ([[lock-uniform-client-early]],
//! [[persona-is-a-client]]).
//!
//! This replaces the inverted status quo (98 hand-authored TS spec JSONs + the
//! old tsx generator, which made the shallow TS layer the source of truth). Here
//! the deepest layer — the Rust command declaration, whose `Params`/`Result` are
//! the SAME ts-rs types the wire uses — is the source; the TS surface is output.
//!
//! ## Import model (not decl-inlining)
//!
//! ts-rs is built so each type exports to its OWN file and consumers IMPORT it.
//! So the generated `CommandMap` **imports** each command's params/result types
//! (and their transitive dependencies, via [`TS::dependencies`]) from where ts-rs
//! emits them (`protocol/typescript/*`) — the type lives once, the map references
//! it. (The outlier-B nested command proved this: re-emitting `decl()` text drops
//! nested types; importing carries them.)
//!
//! ## Scope (proving slice)
//!
//! Outlier-validation slice: the [`CommandSpec`] pattern + the TS `CommandMap`
//! emitter, proven on TWO outliers — a trivial command (`ping`) and a
//! nested-params one (`data/list`, whose params reference `OrderBy`). Follow-ups
//! behind this proven interface: auto-discovery of all `CommandSpec` impls
//! (`inventory`) replacing the explicit registry; the Swift/Kotlin/CLI emitters
//! (same registry, more backends); the TS front/back binding tiers; the
//! `import_base` resolution against the generated file's real location; and
//! unifying runtime dispatch onto the same `CommandSpec`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ts_rs::{Config, Dependency, TS};

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

/// The single source of truth for ONE command. `Params`/`Result` are ts-rs
/// types — the SAME types a handler parses and the wire types ts-rs emits — so
/// one declaration feeds both runtime dispatch (future) and codegen (here).
pub trait CommandSpec {
    /// The command's URI path (e.g. `"data/list"`).
    const NAME: &'static str;
    /// The capability this command requires.
    const ACCESS_LEVEL: AccessLevel;
    /// Typed params (the wire shape callers pass).
    type Params: TS + 'static;
    /// Typed result (the wire shape callers get back).
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
            params,
            result,
            type_refs,
        }
    }
}

/// Emit the TypeScript `CommandMap` module from the registry — the typed surface
/// every TS SDK consumer infers from (`Commands.execute<K>(name, params)`). Pure:
/// descriptors in, TS text out, so it's testable without node.
///
/// `import_base` is the path (relative to the generated file) where the ts-rs
/// wire types are emitted — e.g. `"@protocol"` or `"../../../protocol/typescript"`.
pub fn generate_command_map(commands: &[CommandDescriptor], import_base: &str) -> String {
    // Group imports by module, deduped — one `import type { A, B } from 'm'` line
    // per module, names sorted for stable output.
    let mut by_module: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for c in commands {
        for t in &c.type_refs {
            by_module
                .entry(t.module.clone())
                .or_default()
                .insert(t.name.clone());
        }
    }

    let mut out = String::new();
    out.push_str(
        "// GENERATED from the Rust command registry (core/continuum-core sdk_codegen).\n\
         // DO NOT EDIT. Source of truth: each command's CommandSpec (name + ts-rs\n\
         // Params/Result). Regenerate after a command changes.\n\n",
    );

    for (module, names) in &by_module {
        let list = names.iter().cloned().collect::<Vec<_>>().join(", ");
        out.push_str(&format!(
            "import type {{ {list} }} from '{import_base}/{module}';\n"
        ));
    }
    out.push('\n');

    out.push_str("/** name -> { params, result }. Generated; the contract is Rust-origin. */\n");
    out.push_str("export interface CommandMap {\n");
    for c in commands {
        out.push_str(&format!(
            "  '{}': {{ params: {}; result: {} }};\n",
            c.name, c.params.name, c.result.name
        ));
    }
    out.push_str("}\n\n");
    out.push_str("export type CommandName = keyof CommandMap;\n");
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
    // validated against a REAL registered command (no fictional fixtures). Its
    // types carry the production export_to form, so this exercises module_of on
    // real data: the generated imports must resolve (NO `../../../` escapes) and
    // reference the single-source modules under the TS root.
    #[test]
    fn generates_command_map_from_the_real_registry() {
        let registry = command_registry();
        assert!(
            !registry.is_empty(),
            "at least one real command is registered (inference/llm/request)"
        );
        let out = generate_command_map(&registry, "@protocol");

        // The real inference command, keyed by NAME, with its real ts-rs types.
        assert!(out.contains(
            "'inference/llm/request': { params: InferenceRequest; result: InferenceResponse }"
        ));

        // CRITICAL: imports resolve — module_of stripped the export escape path.
        assert!(out.contains("import type {"), "imports emitted");
        assert!(
            !out.contains("../../../"),
            "no ts-rs export escape path leaks into the generated imports"
        );
        assert!(
            out.contains("from '@protocol/inference_llm/"),
            "real types import from their clean module path under the TS root"
        );

        assert!(out.contains("export interface CommandMap"));
        assert!(out.contains("export type CommandName = keyof CommandMap;"));
        assert!(out.starts_with("// GENERATED from the Rust command registry"));
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
