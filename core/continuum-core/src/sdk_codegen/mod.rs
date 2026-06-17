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

/// The capability a command declares — mirrors the spec `accessLevel`. Carried
/// by the command, enforced by the core's `AuthPolicy`, never by the SDK.
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

/// Drop the `.ts` extension + normalize separators so a ts-rs `output_path`
/// (e.g. `ai/ModelInfo.ts`) becomes an importable module path (`ai/ModelInfo`).
fn module_of(output_path: &Path) -> String {
    output_path
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/")
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

/// The command registry — the generator's input. Explicit for the proving slice;
/// becomes auto-discovery (every `CommandSpec` impl) once the pattern is proven.
pub fn command_registry() -> Vec<CommandDescriptor> {
    vec![
        CommandDescriptor::of::<demo::Ping>(),
        CommandDescriptor::of::<demo::DataList>(),
    ]
}

/// The two outlier commands that prove the `CommandSpec` declaration handles both
/// extremes — a trivial command and a nested-params one. Real commands declare
/// `CommandSpec` where they live; these exist only to validate the interface.
mod demo {
    use super::{AccessLevel, CommandSpec};
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;

    // ── Outlier A: trivial ───────────────────────────────────────────────
    #[derive(Serialize, Deserialize, TS)]
    #[serde(rename_all = "camelCase")]
    pub struct PingParams {
        #[ts(optional)]
        pub message: Option<String>,
    }
    #[derive(Serialize, Deserialize, TS)]
    #[serde(rename_all = "camelCase")]
    pub struct PingResult {
        pub ok: bool,
        pub round_trip_ms: u32,
    }
    pub struct Ping;
    impl CommandSpec for Ping {
        const NAME: &'static str = "ping";
        const ACCESS_LEVEL: AccessLevel = AccessLevel::AiSafe;
        type Params = PingParams;
        type Result = PingResult;
    }

    // ── Outlier B: nested params (the extreme the interface must also fit) ──
    #[derive(Serialize, Deserialize, TS)]
    #[serde(rename_all = "camelCase")]
    pub struct OrderBy {
        pub field: String,
        pub direction: String,
    }
    #[derive(Serialize, Deserialize, TS)]
    #[serde(rename_all = "camelCase")]
    pub struct DataListParams {
        pub collection: String,
        #[ts(optional)]
        pub order_by: Option<Vec<OrderBy>>,
    }
    #[derive(Serialize, Deserialize, TS)]
    #[serde(rename_all = "camelCase")]
    pub struct DataListResult {
        pub total: u32,
    }
    pub struct DataList;
    impl CommandSpec for DataList {
        const NAME: &'static str = "data/list";
        const ACCESS_LEVEL: AccessLevel = AccessLevel::AiSafe;
        type Params = DataListParams;
        type Result = DataListResult;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the Rust-source → TS CommandMap generation end-to-end.
    // Two single-source CommandSpec declarations (trivial + nested) project into a
    // valid TS CommandMap that imports every type it references — INCLUDING the
    // nested dependency (OrderBy). The generator IS the consistency guarantee, so
    // it's pinned: drift in the emitter or a dropped dependency fails here.
    #[test]
    fn generates_command_map_importing_all_single_source_types() {
        let out = generate_command_map(&command_registry(), "@protocol");

        // Both commands appear, keyed by their NAME.
        assert!(out.contains("'ping': { params: PingParams; result: PingResult }"));
        assert!(out.contains("'data/list': { params: DataListParams; result: DataListResult }"));

        // Every referenced type is imported from the single-source location —
        // top-level params/result AND the nested dependency.
        assert!(out.contains("PingParams"), "params imported");
        assert!(out.contains("PingResult"), "result imported");
        assert!(out.contains("DataListParams"));
        assert!(out.contains("DataListResult"));
        assert!(
            out.contains("OrderBy"),
            "nested dependency must be imported, not dropped (the outlier-B catch)"
        );
        assert!(out.contains("import type {"), "import statements emitted");
        assert!(out.contains("from '@protocol/"), "imports use the configured base");

        // The map + key type close it out; GENERATED banner; never hand-edited.
        assert!(out.contains("export interface CommandMap"));
        assert!(out.contains("export type CommandName = keyof CommandMap;"));
        assert!(out.starts_with("// GENERATED from the Rust command registry"));
    }

    // what this catches: a command's typed declaration round-trips to a
    // descriptor purely at the type level, carrying name + access + the nested
    // dependency among its type_refs.
    #[test]
    fn descriptor_captures_name_access_and_nested_deps() {
        let d = CommandDescriptor::of::<demo::DataList>();
        assert_eq!(d.name, "data/list");
        assert_eq!(d.access_level, AccessLevel::AiSafe);
        assert_eq!(d.params.name, "DataListParams");
        assert_eq!(d.result.name, "DataListResult");
        assert!(
            d.type_refs.iter().any(|t| t.name == "OrderBy"),
            "nested type is captured as an importable ref"
        );
    }
}
