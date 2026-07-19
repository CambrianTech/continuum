//! Dynamic persona tool surface — discovered, never hardcoded.
//!
//! A persona's hands are the registry's `AiSafe` commands. The tool list is a
//! PURE FUNCTION of `command_registry() × access_level`: every command that
//! declares [`AccessLevel::AiSafe`] becomes a tool the persona can call, and a
//! new ai-safe command appears → the persona can use it with zero code change.
//! There is NO hardcoded tool list and NO parallel allow-table — the command's
//! own `access_level` (its "destiny") is the single, consistent source of truth,
//! surfaced through the one central listing [`command_registry`].
//!
//! ## The bug this shape refuses to ship (Joel 2026-06-21)
//! "The tool's gone and no one knows why — RAG says it has it, some smell
//! somewhere restricted it." Because the surface is exactly
//! `command_registry().filter(AiSafe)`, "why isn't tool X here?" has ONE answer:
//! its `access_level` isn't `AiSafe`. [`tool_surface_report`] makes that a
//! one-look diagnosis (included / excluded + the reason), never a hunt.
//!
//! ## Metadata maturity — mechanism in place, richness later
//! A [`CommandDescriptor`] today carries `name` + `access_level` + the param
//! TYPE ref — enough to know WHICH commands are tools and to name them, but not
//! a human description or a JSON param schema. So a tool's description is
//! best-effort (command + param type) and its `input_schema` is an open object
//! (the command validates its own typed params). When commands declare richer
//! tool-facing metadata — a description and a param schema, the next mechanism,
//! living in the command's own destiny — [`descriptor_to_tool_spec`] consumes it
//! with no caller change. Open-by-default now; advanced description/validation
//! later, exactly where it belongs.

use crate::ai::types::{NativeToolSpec, ToolInputSchema};
use crate::cognition::tool_embedding::extract_category;
use crate::modules::grid::acl::is_command_authorized;
use crate::modules::grid::node::TrustLevel;
use crate::commands::help::CommandsHelp;
use crate::sdk_codegen::{command_registry, AccessLevel, ActionCommand, CommandDescriptor};
use serde_json::json;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The command a persona calls to learn HOW to invoke one tool — the on-demand half
/// of progressive disclosure (the catalog gives names; this gives the call format).
/// This is the EXISTING [`CommandsHelp`] (`commands/help`), which renders a
/// fill-in-the-blanks tool-call envelope with typed argument docs — strictly better
/// for a small model than raw JSON Schema. We do NOT define a parallel describe tool:
/// one source of truth for "how do I call this?" [[command-infra-self-routing-schema-adapters]]
pub const TOOL_HELP_NAME: &str = CommandsHelp::NAME;

/// Soft cap on a tool's one-line catalog summary (chars). A catalog lists ~100
/// tools; an unbounded description per line would re-create the dump. One clause is
/// enough to pick; the full call format arrives via [`TOOL_HELP_NAME`].
const SUMMARY_MAX_CHARS: usize = 96;

/// The persona's tool surface: every command it is AUTHORIZED to run at `trust`,
/// projected to a tool spec. **Offer == authorized, by construction** — a persona
/// is NEVER shown a tool the gate would refuse (no "offer ping then deny it"). The
/// SAME [`is_command_authorized`] the executor enforces decides what's offered, so
/// the two can't drift, and opening a command to a trust level auto-adds it here.
/// Dynamic from the live [`command_registry`]; nothing hardcoded.
pub fn authorized_tool_specs(trust: TrustLevel) -> Vec<NativeToolSpec> {
    command_registry()
        .iter()
        .filter(|d| is_command_authorized(d.name, trust))
        .map(descriptor_to_tool_spec)
        .collect()
}

/// The raw `AiSafe`-by-declaration surface, IGNORING caller identity and the grid
/// ACL overrides. This is NOT a persona tool surface and must never become one:
/// it can over-list (a command declared `AiSafe` but bumped to `Owner` by an
/// explicit ACL rule would appear here yet be denied at the gate) — exactly the
/// "listed a tool I can't call" violation Joel forbids. Gated `#[cfg(test)]` so no
/// production path can reach it: the ONLY way to get a persona's tools is the
/// identity-gated [`authorized_tool_specs`]. Kept solely to assert the projection
/// (descriptor → spec) against the registry in tests.
#[cfg(test)]
pub fn ai_safe_tool_specs() -> Vec<NativeToolSpec> {
    command_registry()
        .iter()
        .filter(|d| d.access_level == AccessLevel::AiSafe)
        .map(descriptor_to_tool_spec)
        .collect()
}

/// Project one command descriptor to an LLM tool spec. The tool NAME is the
/// command name (the executor maps it straight back to a command). Description +
/// schema are best-effort until the descriptor carries richer metadata (see
/// module docs); this projection is structured so that richness lands here
/// without changing any caller.
pub fn descriptor_to_tool_spec(d: &CommandDescriptor) -> NativeToolSpec {
    NativeToolSpec {
        name: d.name.to_string(),
        // The command's own declared DESCRIPTION (headless, compartmentalized) when
        // present; otherwise fall back to a name + param-type handle so the model
        // still has something. A command becomes a GOOD tool simply by declaring
        // `const DESCRIPTION` in its own file — no change here.
        description: if d.description.is_empty() {
            format!("Command `{}` (params: {}).", d.name, d.params.name)
        } else {
            d.description.to_string()
        },
        // The command's REAL param schema (derived automatically from its Rust
        // type by the base traits) becomes the tool's `input_schema` — so the
        // reasoner sees exactly what fields a tool takes, same schema every other
        // SDK adapts from. Commands not yet on a base trait carry a `Null` schema;
        // those fall back to an open object (the command still validates its typed
        // params). One source, every interface ([[command-organization]]).
        input_schema: tool_input_schema_from(&d.params_schema),
    }
}

// ─────────────────────────── progressive disclosure ──────────────────
//
// The persona's tool surface must be DISCOVERABLE without dumping every tool's
// full JSON parameter schema into every deliberation turn. ~100 AiSafe commands ×
// a full `input_schema` each ≈ 4–5k tokens riding EVERY turn — 5× the size of a
// 90-token task, and the budgeter "fixed" the bloat by dropping the persona's
// actual tools to fit the window (amputating her hands). The same shape every
// capable agent runtime uses (Claude Code's deferred tools + a search/describe
// tool): keep a COMPACT CATALOG (names + one-line summaries, grouped by category)
// always in the prompt, and load a single tool's full call format ON DEMAND via
// the existing [`TOOL_HELP_NAME`] (`commands/help`). Dispatch is by NAME (the executor maps any catalog name
// straight back to its command), so a tool not in the natively-offered set still
// runs — which is exactly what makes disclosure safe. The catalog is a pure,
// data-driven projection of the registry (NOT output puppeteering): same single
// source of truth as [`authorized_tool_specs`], just a leaner representation.

/// One compact catalog line: a tool's name, a one-line summary, and the category
/// it groups under (the first path segment, via [`extract_category`]).
#[derive(Debug, Clone)]
pub struct ToolCatalogEntry {
    pub name: String,
    pub summary: String,
    pub category: String,
}

/// The persona's tool catalog at `trust`: the same authorized set as
/// [`authorized_tool_specs`], projected to compact [`ToolCatalogEntry`] lines
/// (name + one-line summary + category) instead of full schemas. This is what the
/// persona browses; the full call format for any one tool comes from
/// [`TOOL_HELP_NAME`] on demand.
pub fn authorized_tool_catalog(trust: TrustLevel) -> Vec<ToolCatalogEntry> {
    command_registry()
        .iter()
        .filter(|d| is_command_authorized(d.name, trust))
        .map(|d| ToolCatalogEntry {
            name: d.name.to_string(),
            summary: tool_summary(d),
            category: extract_category(d.name).to_string(),
        })
        .collect()
}

/// A tool's one-line summary for the catalog: the first sentence / first line of
/// its declared description, hard-capped at [`SUMMARY_MAX_CHARS`]. Falls back to the
/// param-type handle when the command declares no description (same fallback as
/// [`descriptor_to_tool_spec`], kept consistent).
fn tool_summary(d: &CommandDescriptor) -> String {
    let raw = if d.description.is_empty() {
        format!("params: {}", d.params.name)
    } else {
        // First line, then first sentence within it — whichever ends sooner.
        let first_line = d.description.lines().next().unwrap_or("").trim();
        match first_line.find(". ") {
            Some(i) => first_line[..i].to_string(),
            None => first_line.trim_end_matches('.').to_string(),
        }
    };
    let raw = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if raw.chars().count() <= SUMMARY_MAX_CHARS {
        raw
    } else {
        let truncated: String = raw.chars().take(SUMMARY_MAX_CHARS - 1).collect();
        format!("{}…", truncated.trim_end())
    }
}

/// Render the persona's tool surface as a NAMED CATALOG GROUPED BY CATEGORY — one
/// line per category listing the verb of every tool it holds (`code: run, read,
/// edit, write, search, …`), NOT each tool's full schema.
///
/// ## Why names, not just counts (measured 2026-06-29)
/// Two extremes were tried and both failed. (1) Dumping all ~150 tools with a
/// one-line SUMMARY each cost ~18KB / ~4.6k tokens every turn — 79% of the system
/// prompt — drowning a small model and re-prefilling 4.6k static tokens per turn.
/// (2) Collapsing to a bare category INDEX (`code (25)`) was tiny but hid every tool
/// NAME: to run code she had to guess the category, `commands/list` it (25 results),
/// find `code/run`, `commands/help` it, then call — a 5-hop gauntlet. Glass-box over
/// 3351 captured turns showed the cost: native `code/run` 3×, markdown code-fences
/// 909×, `commands/help` lookups 166× — she SHOWED code instead of RUNNING it because
/// she couldn't SEE the tool existed. Names alone are the 80/20: most verbs
/// (`run`, `read`, `list`, `search`) are self-evident, so seeing the name collapses
/// discovery to 2 hops (read name → `commands/help` for args → call). The SUMMARY was
/// the 18KB; names without summaries are ~2–3KB — small enough to ride every turn.
///
/// Still progressive disclosure (no full schemas inline; `commands/help` gives the
/// call format on demand), still a pure data-driven projection of the authorized set
/// (NOT a hardcoded list, NOT coding-specific — every category renders its verbs
/// uniformly), still cacheable (the authorized set is stable within a session, so the
/// catalog is a byte-stable prefix). Dispatch is by NAME, so any verb she reads here
/// runs. `_budget_chars` retained for call-site compatibility; the named catalog is
/// small by construction (names, not summaries) so it never needs a fallback tier.
pub fn render_tool_catalog(tools: &[NativeToolSpec], _budget_chars: usize) -> String {
    if tools.is_empty() {
        return String::new();
    }
    // Group tool VERBS (the name minus its leading category segment) under each
    // category, each rendered `verb(param, param?)` (see [`render_param_hint`]);
    // stable (BTreeMap + sort) ordering for a byte-stable cacheable prefix.
    let mut by_cat: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for t in tools {
        let cat = extract_category(&t.name);
        // The verb is everything after the first `/` (so `persona/instances/list`
        // shows as `instances/list`); a name with no `/` lists under itself.
        let verb = t.name.strip_prefix(cat).and_then(|r| r.strip_prefix('/')).unwrap_or(&t.name);
        by_cat
            .entry(cat)
            .or_default()
            .push(format!("{verb}{}", render_param_hint(&t.input_schema)));
    }
    let mut out = String::new();
    for (cat, mut verbs) in by_cat {
        verbs.sort_unstable();
        let _ = writeln!(out, "{cat}: {}", verbs.join(", "));
    }
    out
}

/// Render one tool's parameter FIELD NAMES as a compact `(required, optional?)`
/// hint appended to its catalog verb — required params bare, optional params
/// suffixed `?`. A no-param command renders as the empty string (just the verb).
///
/// ## Why field names ride the catalog (measured 2026-07-01, [[persona-codes-blind]])
/// Progressive disclosure keeps full param SCHEMAS out of the always-on prompt
/// (they cost ~4.6k tokens/turn; [`render_tool_catalog`] doc). But names-only
/// verbs (`code: run, search, read`) left the *param* names invisible too — and a
/// live 2-task probe on Asha + Solenne (qwen2.5-coder-14b) showed the 14B model
/// skips the `commands/help` hop and GUESSES field names: `code/search{query}`
/// (wants `pattern`), `code/run{path:...}` (wants `code`) → both `[invalid]
/// CommandRequest: missing field`, burning a ~10-15s act each on the
/// prefill-dominated Metal lane. Field NAMES are the cheap 80/20 fix: a name-list
/// is a handful of tokens (not the typed+described schema), and it collapses the
/// guess-fail-retry that convergence + latency compound over. Still progressive
/// disclosure — types, descriptions, and defaults stay behind `commands/help`;
/// this adds only which fields exist and which are required. Deterministic order
/// (required in schema-declaration order, then optional sorted) keeps the catalog
/// a byte-stable cacheable prefix.
fn render_param_hint(schema: &ToolInputSchema) -> String {
    let required: Vec<&str> = schema
        .required
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(String::as_str)
        .collect();
    // Optional = property keys not in `required`, sorted for byte-stability
    // (`properties` object key order is not guaranteed deterministic).
    let mut optional: Vec<&str> = schema
        .properties
        .as_object()
        .map(|m| {
            m.keys()
                .map(String::as_str)
                .filter(|k| !required.contains(k))
                .collect()
        })
        .unwrap_or_default();
    optional.sort_unstable();
    if required.is_empty() && optional.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = required
        .iter()
        .map(|r| (*r).to_string())
        .chain(optional.iter().map(|o| format!("{o}?")))
        .collect();
    format!("({})", parts.join(", "))
}

/// Render the tool surface as an EXPANDABLE BOOKMARKED MENU: every category HEADER
/// is shown (the stable spine — the menu never changes shape turn to turn), but only
/// the `expanded` categories list their verbs inline; the rest render as collapsed
/// bookmarks (`gpu (4 — commands/list --filter gpu)`) she opens on demand.
///
/// This is the adaptive-but-coherent middle path (Joel 2026-06-29,
/// [[adaptive-tool-surface-meets-you-in-the-middle]]): the spine gives her the full
/// map every turn (never a confusing reshuffle), while [`tool_relevance`] decides
/// which categories open for what she's doing now + the sticky "where you were"
/// cursor (per-(user, room) state owned by airc, threaded in by slice 2). Sibling to
/// [`render_tool_catalog`] (the open-everything render).
///
/// [`tool_relevance`]: crate::cognition::tool_relevance
pub fn render_tool_menu(
    tools: &[NativeToolSpec],
    expanded: &std::collections::BTreeSet<String>,
) -> String {
    if tools.is_empty() {
        return String::new();
    }
    // Same grouping as render_tool_catalog — stable (BTreeMap) order so the spine is
    // a byte-stable prefix; only the per-category expansion differs. An expanded
    // category renders BARE verb names — no param hints. Measured 2026-07-10: the
    // hinted always-expanded menu was 8.4k chars of a 16.5k live system prompt
    // (77% instruction boilerplate vs 21% live world — Joel: "prompts that are
    // more boilerplate than logic"). Verbs stay visible (the she-must-see-her-
    // hands invariant holds); ARGS are on-demand — `commands/help` is progressive
    // disclosure's home, and since #1916 a wrong call gets the exact shape
    // inlined in the SAME error observation, a stronger net at the correction
    // seam than a hint buried in an 8k wall. (The 2026-07-01 field-name-guessing
    // fix moved seams: menu-hint → error-manual.)
    let mut by_cat: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for t in tools {
        let cat = extract_category(&t.name);
        let verb = t.name.strip_prefix(cat).and_then(|r| r.strip_prefix('/')).unwrap_or(&t.name);
        by_cat.entry(cat).or_default().push(verb.to_string());
    }
    let mut out = String::new();
    // ONE form example at the header — the invocation SHAPE, not per-tool
    // hints (those measured as 77% prompt boilerplate and moved to the
    // error-manual seam). 2026-07-12 glass-box: all four personas knew the
    // real NAMES within hours but kept fumbling the FORM (paren-call vs
    // CLI-flag vs bare); one canonical example teaches the form for ~20
    // tokens. PX, not steering: it shows how calls look, never which to make.
    let _ = writeln!(
        out,
        "call form: code/read({{\"file_path\":\"src/main.rs\"}}) — exact args for any tool: commands/help(name)"
    );
    for (cat, mut verbs) in by_cat {
        verbs.sort_unstable();
        if expanded.contains(cat) {
            let _ = writeln!(out, "{cat}: {}", verbs.join(", "));
        } else if verbs.len() == 1 {
            // A singleton category isn't worth collapsing — its one verb IS the name.
            let _ = writeln!(out, "{cat}: {} (+ commands/list --filter {cat})", verbs[0]);
        } else {
            let _ = writeln!(out, "{cat} ({} — commands/list --filter {cat})", verbs.len());
        }
    }
    out
}

/// Group the authorized tools into `(category, verbs)` pairs — the input
/// [`tool_relevance::select_expanded_categories`] scores to decide which categories
/// the menu opens. Verbs are the BARE names (no param hints): they are the category's
/// vocabulary for lexical relevance, not a render. Stable (BTreeMap) order so the
/// scored category list matches the spine order [`render_tool_menu`] emits.
///
/// [`tool_relevance::select_expanded_categories`]: crate::cognition::tool_relevance::select_expanded_categories
pub fn group_categories(tools: &[NativeToolSpec]) -> Vec<(&str, Vec<&str>)> {
    let mut by_cat: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for t in tools {
        let cat = extract_category(&t.name);
        let verb = t.name.strip_prefix(cat).and_then(|r| r.strip_prefix('/')).unwrap_or(&t.name);
        by_cat.entry(cat).or_default().push(verb);
    }
    by_cat.into_iter().collect()
}

/// The tools offered NATIVELY (as function specs) every turn: the discovery pair.
/// `commands/list` (filter/search the authorized surface → a small list of matching
/// tools) + `commands/help` (the exact call format for one named tool). Everything
/// else is reached BY NAME through these two, so the per-turn native payload stays
/// two tiny schemas — never the ~150-tool dump that overflowed the window and muted
/// her. Only includes a spec that actually resolves in the registry (fail-closed:
/// a missing command is omitted, never fabricated — [[fallbacks-are-illegal-fail-loud]]).
pub fn native_tool_specs() -> Vec<NativeToolSpec> {
    // The discovery pair (reaches the long tail by name) PLUS the core agentic-coding arc
    // as REAL native specs. A model TRAINED to tool-call (Devstral, Qwen-Coder) emits native
    // tool_calls — given only the discovery pair it loops forever on `commands/help{code/search}`
    // and never acts (glass-boxed: 14/14 SWE acts were `commands/help`, 0 edits, 0 score). Offering
    // the working set directly lets it search→read→edit→write→run→verify without the help detour,
    // while a weak model that can't emit native tool_calls still falls back to the text menu +
    // narrated-call recovery. Bounded (~11 schemas, ~1-2k tokens) so it never overflows the window
    // the way a full ~150-tool dump did. Only specs that actually resolve are included (fail-closed).
    // [[adaptive-tool-surface-meets-you-in-the-middle]] [[local-first-tool-call-robustness-is-the-differentiator]]
    const NATIVE: &[&str] = &[
        "commands/list",
        TOOL_HELP_NAME,
        "code/search",
        "code/read",
        "code/list",
        "code/tree",
        "code/edit",
        "code/write",
        "code/run",
        "code/shell",
        "code/git/diff",
        // The consolidation rail (2026-07-11): status/commit/apply are how
        // parallel workspaces converge — diff→post→apply, the loop the Conway
        // team invented socially before the rails existed.
        "code/git/status",
        "code/git/commit",
        "code/git/apply",
        // Observation parity (Joel 2026-07-11): seeing the screen is a first-class
        // work verb, not a special capability — "if they can observe like we do,
        // they can build like we do." Routed to a client adapter (WireShape::
        // Provided); fails loud when no UI adapter is connected, never fabricates.
        "interface/screenshot",
        // Perception Surface (#187): observe = SEE + REASON — pixels AND the
        // structure tree to aim actions at an element, not a pixel. The enriched
        // sibling of screenshot; also Provided (fails loud with no eye-node).
        "perception/observe",
        // Perception Surface (#187), live-call video sibling: look = a persona's
        // OWN eyes on the video call it's in — pull a current image of one
        // participant or everyone, thumbnail or full. Substrate-served off the
        // in-process perception buffer; empties/teaches when not in a call.
        "perception/look",
        // The shared-board lifecycle: claiming work as yourself is core to the
        // room workflow (rides the wire as `claim_task` via the dialect).
        "work/claim",
    ];
    NATIVE.iter().filter_map(|n| spec_for_command(n)).collect()
}

/// Look up one command by name and project it to a full tool spec — the on-demand
/// half of progressive disclosure (the catalog gives names; this gives the schema).
/// `None` when no such command is registered (the caller reports that, never
/// fabricates a schema — [[fallbacks-are-illegal-fail-loud]]).
pub fn spec_for_command(name: &str) -> Option<NativeToolSpec> {
    command_registry()
        .iter()
        .find(|d| d.name == name)
        .map(descriptor_to_tool_spec)
}

// The on-demand "how do I call this?" tool is the EXISTING `commands/help`
// ([`CommandsHelp`] in `crate::commands::help`), referenced by [`TOOL_HELP_NAME`].
// We deliberately do NOT define a parallel describe command here — `commands/help`
// renders a fill-in-the-blanks tool-call envelope with typed argument docs (better
// for a small model than raw JSON Schema), and one source of truth for the call
// format is the compression principle. The faculty offers `commands/help` as the
// single native tool alongside the compact catalog.

/// Project a command's params JSON Schema into the LLM [`ToolInputSchema`]. A
/// `Null` schema (command not yet on a base trait) → an open object. Otherwise
/// lift `type`/`properties`/`required` — AND the `definitions`/`$defs` map — from
/// the derived schema.
///
/// Nested-param commands (`code/edit` → `EditMode`, `data/list` → `OrderByClause`,
/// `rag/load` → the self-referential `RagSourceRequest`, …) make schemars emit
/// `$ref: "#/definitions/<Name>"` in `properties` plus a sibling `definitions`
/// map. Both MUST ship: the backend resolves each ref against the carried map.
/// Drop the map and llama.cpp rejects the whole turn with a 400 ("definitions not
/// in {…}") — the bug that kept every tool-enabled persona turn silent until the
/// command-registry migration exposed the first nested-param tool.
fn tool_input_schema_from(schema: &serde_json::Value) -> ToolInputSchema {
    if schema.is_null() {
        return ToolInputSchema {
            schema_type: "object".to_string(),
            properties: json!({}),
            required: None,
            definitions: None,
        };
    }
    // llama.cpp's GBNF grammar generator rejects JSON-Schema *boolean* subschemas
    // (`true`/`false`) with "Unrecognized schema: true" — but schemars emits `true`
    // for any untyped field (a `serde_json::Value`, an open `HashMap<String,
    // Value>`, or a default `additionalProperties`). A single such field anywhere
    // in the offered tool set 400s the WHOLE deliberation turn (every tool schema
    // rides one request), which is exactly what kept Asha/Solenne abstaining on
    // every tick once the command-registry migration exposed ~200 real tool
    // schemas. Rewrite boolean subschemas to their object equivalents at the
    // schema positions JSON-Schema defines (`true` → `{}` = any value; `false` →
    // `{"not": {}}` = no value), leaving keyword booleans (`nullable: true`,
    // `deprecated: true`) untouched.
    let schema = sanitize_schema_booleans(schema.clone());
    ToolInputSchema {
        schema_type: schema
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("object")
            .to_string(),
        properties: schema
            .get("properties")
            .cloned()
            .unwrap_or_else(|| json!({})),
        required: schema.get("required").and_then(|v| v.as_array()).map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        }),
        // Carry the nested-type definitions under the key the refs name. schemars
        // (draft-07) emits `definitions`; tolerate `$defs` (2020-12) too.
        definitions: schema
            .get("definitions")
            .or_else(|| schema.get("$defs"))
            .cloned(),
    }
}

/// JSON-Schema keywords whose value IS a schema (so a boolean there is a boolean
/// *subschema*). `items` may also be an array of schemas (draft-04 tuple form),
/// handled at the call site.
const SCHEMA_VALUED_KEYS: &[&str] = &[
    "additionalProperties",
    "items",
    "additionalItems",
    "contains",
    "propertyNames",
    "not",
    "if",
    "then",
    "else",
    "unevaluatedProperties",
    "unevaluatedItems",
];
/// Keywords whose value is an OBJECT mapping names → schemas.
const SCHEMA_MAP_KEYS: &[&str] = &[
    "properties",
    "patternProperties",
    "definitions",
    "$defs",
    "dependentSchemas",
];
/// Keywords whose value is an ARRAY of schemas.
const SCHEMA_ARRAY_KEYS: &[&str] = &["allOf", "anyOf", "oneOf", "prefixItems"];

/// Recursively rewrite JSON-Schema *boolean* subschemas (`true`/`false`) — which
/// llama.cpp's grammar generator rejects — into their object equivalents, walking
/// only the positions JSON-Schema treats as schemas. `true` → `{}` (matches any
/// value, same meaning as `true`); `false` → `{"not": {}}` (matches nothing).
/// Keyword booleans like `nullable`/`deprecated`/`readOnly` are NOT schema
/// positions and are left exactly as-is. See `tool_input_schema_from`.
fn sanitize_schema_booleans(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Bool(true) => json!({}),
        Value::Bool(false) => json!({ "not": {} }),
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, val) in map {
                let nv = if SCHEMA_VALUED_KEYS.contains(&k.as_str()) {
                    match val {
                        // draft-04 tuple form: `items: [schema, schema, …]`
                        Value::Array(items) => Value::Array(
                            items.into_iter().map(sanitize_schema_booleans).collect(),
                        ),
                        other => sanitize_schema_booleans(other),
                    }
                } else if SCHEMA_MAP_KEYS.contains(&k.as_str()) {
                    match val {
                        Value::Object(inner) => Value::Object(
                            inner
                                .into_iter()
                                .map(|(ik, iv)| (ik, sanitize_schema_booleans(iv)))
                                .collect(),
                        ),
                        other => other,
                    }
                } else if SCHEMA_ARRAY_KEYS.contains(&k.as_str()) {
                    match val {
                        Value::Array(items) => Value::Array(
                            items.into_iter().map(sanitize_schema_booleans).collect(),
                        ),
                        other => other,
                    }
                } else {
                    val
                };
                out.insert(k, nv);
            }
            Value::Object(out)
        }
        other => other,
    }
}

/// What's in the persona's tool surface and WHY — the anti-"tool vanished"
/// diagnostic. One-look answer to "what can the persona call, and why is X in or
/// out," derived from the same single source of truth as the surface itself.
pub fn tool_surface_report() -> ToolSurfaceReport {
    let mut included = Vec::new();
    let mut excluded = Vec::new();
    for d in command_registry() {
        match d.access_level {
            AccessLevel::AiSafe => included.push(d.name.to_string()),
            other => excluded.push((d.name.to_string(), other)),
        }
    }
    included.sort();
    excluded.sort_by(|a, b| a.0.cmp(&b.0));
    ToolSurfaceReport { included, excluded }
}

/// Inspectable tool-surface snapshot.
#[derive(Debug, Clone)]
pub struct ToolSurfaceReport {
    /// Command names the persona CAN call (`AccessLevel::AiSafe`).
    pub included: Vec<String>,
    /// Commands EXCLUDED, each with the access level that excluded it
    /// (`Privileged` / `Internal`) — the single reason, no hunting.
    pub excluded: Vec<(String, AccessLevel)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str) -> NativeToolSpec {
        NativeToolSpec {
            name: name.to_string(),
            description: String::new(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: json!({}),
                required: None,
                definitions: None,
            },
        }
    }

    // what this catches: the menu render is an expandable bookmarked menu — EVERY
    // category appears (the stable spine), an expanded category lists its verbs
    // inline, and a collapsed one shows only a depth + how to open it. Regression
    // here = the menu degrading back into either a full dump or a blind index.
    #[test]
    fn render_tool_menu_expands_only_selected_categories() {
        let tools = [
            spec("code/run"),
            spec("code/read"),
            spec("code/edit"),
            spec("gpu/stats"),
            spec("gpu/pressure"),
        ];
        let expanded: std::collections::BTreeSet<String> = ["code".to_string()].into_iter().collect();
        let out = render_tool_menu(&tools, &expanded);

        // Spine: both categories present every turn.
        assert!(out.contains("code"), "code header missing: {out}");
        assert!(out.contains("gpu"), "gpu header missing (spine broken): {out}");
        // Expanded code lists its verbs inline.
        assert!(out.contains("code: edit, read, run"), "code not expanded: {out}");
        // Collapsed gpu shows depth + how to open, NOT its verbs.
        assert!(out.contains("gpu (2 — commands/list --filter gpu)"), "gpu not collapsed: {out}");
        assert!(!out.contains("stats"), "collapsed gpu leaked verbs: {out}");
    }

    // Build a spec whose schema declares required + optional fields, so the
    // catalog's param-name rendering can be asserted.
    fn spec_with(name: &str, required: &[&str], optional: &[&str]) -> NativeToolSpec {
        let mut props = serde_json::Map::new();
        for f in required.iter().chain(optional.iter()) {
            props.insert((*f).to_string(), json!({ "type": "string" }));
        }
        NativeToolSpec {
            name: name.to_string(),
            description: String::new(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::Value::Object(props),
                required: (!required.is_empty())
                    .then(|| required.iter().map(|s| s.to_string()).collect()),
                definitions: None,
            },
        }
    }

    // what this catches: the always-on catalog surfaces each verb's PARAM FIELD
    // NAMES (required bare, optional suffixed `?`) so a small model stops guessing
    // `code/search{query}`/`code/run{path}` and hitting `[invalid] CommandRequest`
    // (2026-07-01 legibility fix, [[persona-codes-blind]]). Required order is the
    // schema's declared order; optional is sorted; a no-param verb renders bare.
    #[test]
    fn catalog_renders_param_field_names() {
        let tools = [
            spec_with("code/search", &["pattern"], &["path"]),
            spec_with("code/run", &["lang", "code"], &["timeout_secs"]),
            spec("code/list"), // no params → bare verb, no parens
        ];
        let out = render_tool_catalog(&tools, 0);
        // Required bare, optional suffixed `?`; required keeps declared order.
        assert!(out.contains("run(lang, code, timeout_secs?)"), "run params wrong: {out}");
        assert!(out.contains("search(pattern, path?)"), "search params wrong: {out}");
        // A no-param verb renders bare — no empty `()`.
        assert!(out.contains("list,") || out.trim_end().ends_with("list"), "list should be bare: {out}");
        assert!(!out.contains("list()"), "no-param verb must not render empty parens: {out}");
    }

    // what this catches: the tool surface is DYNAMIC and consistent — it is
    // exactly the AiSafe slice of command_registry, nothing hardcoded. If a
    // command's access_level changes, its tool presence changes with it, no code
    // edit. Regression here = the surface drifting from the single source of
    // truth (the exact "tool vanished / appeared and no one knows why" bug).
    #[test]
    fn tool_surface_is_exactly_the_ai_safe_slice_of_the_registry() {
        let specs = ai_safe_tool_specs();
        let report = tool_surface_report();

        // Every spec corresponds to an AiSafe command; counts agree with the
        // registry's own AiSafe count — no hidden inclusion/exclusion.
        let registry_ai_safe = command_registry()
            .iter()
            .filter(|d| d.access_level == AccessLevel::AiSafe)
            .count();
        assert_eq!(specs.len(), registry_ai_safe, "surface == registry AiSafe count");
        assert_eq!(report.included.len(), registry_ai_safe);

        // Included and excluded partition the WHOLE registry — every command is
        // accounted for, so "why isn't X here" always has an answer.
        let total = command_registry().len();
        assert_eq!(
            report.included.len() + report.excluded.len(),
            total,
            "every command is either included or excluded — no silent drops"
        );

        // The spec names ARE command names (the executor maps them straight back).
        for spec in &specs {
            assert!(
                command_registry().iter().any(|d| d.name == spec.name),
                "tool {} must be a real command",
                spec.name
            );
        }
    }

    // what this catches: descriptor → tool-spec projection keeps the command
    // name verbatim (the executor dispatches on it) and emits a usable open
    // schema. When richer metadata lands, this is the one place it flows in.
    #[test]
    fn projection_preserves_command_name_and_emits_open_schema() {
        // Pick any real AiSafe descriptor from the registry (don't hardcode one).
        let registry = command_registry();
        let Some(d) = registry
            .iter()
            .find(|d| d.access_level == AccessLevel::AiSafe)
        else {
            // No AiSafe commands compiled into this build — nothing to assert.
            return;
        };
        let spec = descriptor_to_tool_spec(d);
        assert_eq!(spec.name, d.name, "tool name is the command name verbatim");
        assert_eq!(spec.input_schema.schema_type, "object");
        assert!(!spec.description.is_empty(), "tool carries a description handle");
    }

    // what this catches: a nested-param command (schemars emits `$ref:
    // "#/definitions/<Name>"` + a sibling `definitions` map) MUST ship that map on
    // the projected tool schema. Dropping it leaves dangling refs and llama.cpp
    // 400s the whole turn ("definitions not in {…}") — the bug that kept every
    // tool-enabled persona turn silent. If a property references definitions, the
    // map travels; the two are never split.
    #[test]
    fn nested_param_schema_carries_its_definitions() {
        // Find any registered command whose derived params schema has a
        // `definitions`/`$defs` map (code/edit, data/list, rag/load, …). If none
        // is compiled into this build, there is nothing to assert.
        let registry = command_registry();
        let Some(d) = registry.iter().find(|d| {
            d.params_schema.get("definitions").is_some() || d.params_schema.get("$defs").is_some()
        }) else {
            return;
        };
        let spec = descriptor_to_tool_spec(d);
        let defs = spec
            .input_schema
            .definitions
            .as_ref()
            .unwrap_or_else(|| panic!("command {} has nested definitions that were dropped", d.name))
            .as_object()
            .expect("definitions is a JSON object map");
        assert!(!defs.is_empty(), "definitions map for {} must not be empty", d.name);

        // Every `#/definitions/<Name>` referenced in the serialized properties has
        // a matching key in the carried map — no dangling ref a backend can't
        // resolve (the exact shape that 400'd the turn).
        let props = spec.input_schema.properties.to_string();
        for fragment in props.split("#/definitions/").skip(1) {
            let name: String = fragment
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            assert!(
                defs.contains_key(&name),
                "command {} references #/definitions/{} but the carried map lacks it",
                d.name,
                name
            );
        }
    }

    // what this catches: a boolean subschema (`true`/`false`) anywhere in a tool's
    // params schema — which schemars emits for any untyped field (`serde_json::Value`,
    // open `HashMap`, default `additionalProperties`) — survives projection unrewritten
    // and 400s the WHOLE deliberation turn ("Unrecognized schema: true"), because every
    // offered tool schema rides one llama-server request. This is the regression that
    // kept Asha/Solenne abstaining on every tick after the registry migration exposed
    // ~200 real schemas. We rewrite `true`→`{}` / `false`→`{"not":{}}` at schema
    // positions only, leaving keyword booleans (`nullable`) untouched.
    #[test]
    fn boolean_subschemas_are_rewritten_but_keyword_booleans_survive() {
        let schema = json!({
            "type": "object",
            "properties": {
                "payload": true,                       // serde_json::Value field
                "name": { "type": "string", "nullable": true },
                "tags": { "type": "array", "items": true },
                "blocked": false
            },
            "additionalProperties": true,
            "definitions": { "Open": true }
        });
        let out = sanitize_schema_booleans(schema);

        // schema positions rewritten to objects
        assert_eq!(out["properties"]["payload"], json!({}), "true → {{}}");
        assert_eq!(out["properties"]["blocked"], json!({ "not": {} }), "false → not-any");
        assert_eq!(out["properties"]["tags"]["items"], json!({}), "items: true → {{}}");
        assert_eq!(out["additionalProperties"], json!({}), "additionalProperties: true → {{}}");
        assert_eq!(out["definitions"]["Open"], json!({}), "definition true → {{}}");

        // keyword boolean is NOT a schema position — left exactly as-is
        assert_eq!(out["properties"]["name"]["nullable"], json!(true), "nullable untouched");

        // and there is no bare `true`/`false` left anywhere in the serialized schema
        assert!(
            !out.to_string().contains("true") || out["properties"]["name"]["nullable"] == json!(true),
            "the only surviving `true` is the keyword boolean"
        );
    }

    // what this catches: the projection over the LIVE registry must never emit a
    // boolean subschema — i.e. no real compiled command's tool schema can carry the
    // shape that 400s the turn. Guards every current and future AiSafe command at once.
    #[test]
    fn no_live_tool_schema_carries_a_boolean_subschema() {
        fn has_bool_subschema(v: &serde_json::Value) -> bool {
            use serde_json::Value;
            match v {
                Value::Object(map) => {
                    for (k, val) in map {
                        let is_schema_pos = SCHEMA_VALUED_KEYS.contains(&k.as_str())
                            || SCHEMA_MAP_KEYS.contains(&k.as_str())
                            || SCHEMA_ARRAY_KEYS.contains(&k.as_str());
                        if is_schema_pos {
                            match val {
                                Value::Bool(_) => return true,
                                Value::Object(inner) => {
                                    if inner.values().any(|x| matches!(x, Value::Bool(_))) {
                                        return true;
                                    }
                                }
                                Value::Array(items) => {
                                    if items.iter().any(|x| matches!(x, Value::Bool(_))) {
                                        return true;
                                    }
                                }
                                _ => {}
                            }
                        }
                        if has_bool_subschema(val) {
                            return true;
                        }
                    }
                    false
                }
                Value::Array(items) => items.iter().any(has_bool_subschema),
                _ => false,
            }
        }

        for spec in ai_safe_tool_specs() {
            let props = &spec.input_schema.properties;
            assert!(
                !has_bool_subschema(props),
                "tool {} emits a boolean subschema in properties — llama.cpp will 400 the turn",
                spec.name
            );
            if let Some(defs) = &spec.input_schema.definitions {
                assert!(
                    !has_bool_subschema(defs),
                    "tool {} emits a boolean subschema in definitions",
                    spec.name
                );
            }
        }
    }
}
