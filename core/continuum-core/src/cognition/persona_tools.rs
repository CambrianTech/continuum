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
use crate::commands::help::CommandsHelp;
use crate::modules::grid::acl::is_command_authorized;
use crate::modules::grid::node::TrustLevel;
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

// The per-tool catalog-summary bound is a fraction of her LIVE window
// (`ContextBudget::catalog_summary_chars`), never a constant: a catalog lists ~100 tools, so
// an unbounded line re-creates the dump — but a roomy window can afford a fuller clause per
// tool, and that is real PX. The full call format still arrives via `TOOL_HELP_NAME`.
// [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
use crate::cognition::context_budget::ContextBudget;

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
/// its declared description, bounded by the live window's catalog share. Falls back to the
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
    // `live_or_floor`, not `live`: the catalog is assembled BEFORE the prompt guard, so an
    // unbounded summary here yields a menu that cannot fit rather than one trimmed later.
    let cap = ContextBudget::live_or_floor().catalog_summary_chars();
    if raw.chars().count() <= cap {
        raw
    } else {
        let truncated: String = raw.chars().take(cap.saturating_sub(1)).collect();
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
        let verb = t
            .name
            .strip_prefix(cat)
            .and_then(|r| r.strip_prefix('/'))
            .unwrap_or(&t.name);
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
        let verb = t
            .name
            .strip_prefix(cat)
            .and_then(|r| r.strip_prefix('/'))
            .unwrap_or(&t.name);
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
            let _ = writeln!(
                out,
                "{cat} ({} — commands/list --filter {cat})",
                verbs.len()
            );
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
        let verb = t
            .name
            .strip_prefix(cat)
            .and_then(|r| r.strip_prefix('/'))
            .unwrap_or(&t.name);
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
    // DERIVED, not a hand-kept list: the native surface is every command that DECLARES
    // itself native (`CommandSpec::NATIVE = true`, in its OWN file) — so adding a command
    // and marking it native offers it here AUTOMATICALLY, no central array to edit. This
    // is the dynamic-discovery contract, not a switch statement (CLAUDE.md §Anti-Pattern).
    //
    // Why a bounded native set at all (rather than every AiSafe command): a model TRAINED
    // to tool-call (Devstral, Qwen-Coder) emits native tool_calls, and given the FULL
    // ~150-tool schema dump it was muted (glass-boxed: window overflow). Given ONLY the
    // discovery pair it loops on `commands/help{code/search}` and never acts (14/14 SWE
    // acts were `commands/help`, 0 edits). So the core agentic working set opts into
    // NATIVE while the long tail stays reachable BY NAME through the compact catalog +
    // `commands/help`. The `native` flag is per-command; this projection just collects it.
    // [[adaptive-tool-surface-meets-you-in-the-middle]] [[local-first-tool-call-robustness-is-the-differentiator]]
    command_registry()
        .iter()
        .filter(|d| d.native)
        .map(descriptor_to_tool_spec)
        .chain(verdict_tool_specs())
        .collect()
}

/// The canonical name of the verdict verb that yields the turn.
///
/// Kept as a constant because it is load-bearing in TWO places that must never drift —
/// the offered schema below and the interception in
/// [`super::llm_deliberation_faculty`] — and a name that only agrees by coincidence is
/// the defect class this whole verb exists to end
/// ([[command-names-must-be-accurate-and-a-constant-nobody-references-is-worse-than-none]]).
pub const VERDICT_YIELD_TURN: &str = "yield_turn";

/// Names a model may reach for meaning the same thing. Matched, like every other
/// dialect mapping, on the NAME of a verb we defined — never on prose.
pub const VERDICT_YIELD_ALIASES: &[&str] = &["pass_turn", "pass", "stay_silent", "skip_turn"];

/// VERDICT verbs — the participation decisions that are NOT actions on the world.
///
/// # Why these are faculty-owned and not commands (Joel's call, 2026-08-07)
///
/// [`Decision`](super::workspace::Decision) has four variants, and until now the tool
/// channel could express exactly one of them. `Act` gets its vocabulary from the command
/// registry because acts ARE commands. `Speak` is expressed as prose. `Pass` had **no
/// structured expression at all** — the only way a citizen could decline a turn was to
/// emit a magic word into her prose and hope the parser recognised the sentence she
/// wrapped it in.
///
/// That asymmetry is the actual bug behind #271/#264. Instruction-tuned models naturally
/// wrap a token in a sentence ("Therefore, I will proceed with PASS to avoid further
/// redundancy" — eight of those in one monitor window, three citizens, 2026-08-07), fall
/// off the protocol, and every previous fix compensated with cleverer string matching:
/// a phrase list, then a length cap of 500, then 700, each one beaten by the next
/// message. **The regex was never the disease; it was scar tissue around a missing
/// channel.** Joel, 2026-08-07: "Regex ideas and string matches for semantic
/// understanding is not good for reliability."
///
/// So `Pass` gets a verb. Recognising `yield_turn` in a tool call is protocol decoding —
/// the same category as `write_file` or a JSON fence — not inference about what a
/// sentence means. Evidence it works: the citizens already emit correct native calls
/// (`claim_task`, `update_task`, `list_tasks` in one turn, live 2026-08-07). They use the
/// structured channel fine when we give them one; we simply never gave them this one.
///
/// These live here, next to the action specs, so there is still exactly ONE offered tool
/// surface ([`native_tool_specs`]) rather than a parallel one — but they carry no
/// `CommandSpec`, no ACL entry and no executor arm, because they have no world-effect to
/// authorize. The faculty that turns a generation into a `Decision` owns the vocabulary
/// for the decisions it can reach; it intercepts these before dispatch and they never
/// reach the registry.
pub fn verdict_tool_specs() -> Vec<NativeToolSpec> {
    vec![NativeToolSpec {
        name: VERDICT_YIELD_TURN.to_string(),
        description: "Yield this turn and say nothing. Use this when you have nothing to add \
                      — silence is a real, first-class choice and costs the room nothing. \
                      Prefer it over posting a message that announces you have nothing to \
                      say: that announcement IS noise, and it wakes every peer into posting \
                      their own. Calling this ends your turn silently."
            .to_string(),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            // Deliberately ARGUMENT-FREE. A `reason` field would invite her to compose
            // the very closure paragraph this verb exists to stop her broadcasting, and
            // #334 already measured what happens to a field a model must fill with
            // nothing: it degenerates. Her reasoning is already captured as thinking.
            properties: serde_json::json!({}),
            required: None,
            definitions: None,
        },
    }]
}

/// True if `name` is the yield verb under any name a model reaches for.
pub fn is_yield_turn(name: &str) -> bool {
    name == VERDICT_YIELD_TURN || VERDICT_YIELD_ALIASES.contains(&name)
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
            .map(|p| lead_paragraph_descriptions(p.clone()))
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
            .map(|d| lead_paragraph_descriptions(d.clone())),
    }
}

/// Keep only the LEADING PARAGRAPH of every `description` in a schema subtree.
///
/// # Why the schema and the source comment are not the same text
///
/// `schemars` lifts a field's `///` doc comment verbatim into the JSON Schema, so
/// one piece of prose is asked to serve two readers with opposite needs. The
/// maintainer needs the WHY — why the Rust type is what it is, what the doc used to
/// claim, why a list is deliberately not enumerated. The caller deciding whether to
/// invoke the verb needs only WHAT the field is and what a valid value looks like,
/// and pays for every token of the rest on every turn it is offered.
///
/// Measured on the 26-verb native surface (build 4743): 550 tokens of the 6,935
/// were maintainer rationale, 310 of them on `activity/spawn` alone — where a
/// citizen was billed for `schemars(with = "String") describes the WIRE (a uuid
/// string, per #[serde(transparent)]) to the tool schema while Rust keeps the type`
/// on the way to deciding whether to spawn a room.
///
/// The split is CONVENTIONAL rather than declared — a blank line — because that is
/// how these docs are already written (lead sentence, then rationale), so it needs
/// no per-command annotation and cannot drift from a second source. A single-
/// paragraph doc is unchanged, which is the common case: only 2 of 26 verbs carry
/// a second paragraph today.
///
/// Deliberately NOT a length cap. A long first paragraph is a long ANSWER to
/// "what is this field", and truncating that is how a verb becomes unusable —
/// #358's shape, where a citizen who cannot find the verb reaches for the wrong one.
/// Contentless length is the defect; length is not.
fn lead_paragraph_descriptions(node: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match node {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    // Only a `description` STRING is prose. A key called
                    // `description` whose value is a schema (a param actually named
                    // `description`, e.g. code/edit's) recurses like any other.
                    match (k.as_str(), &v) {
                        ("description", Value::String(text)) => {
                            let lead = text.split("\n\n").next().unwrap_or(text).trim_end();
                            (k, Value::String(lead.to_string()))
                        }
                        _ => (k, lead_paragraph_descriptions(v)),
                    }
                })
                .collect(),
        ),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(lead_paragraph_descriptions).collect())
        }
        other => other,
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
                        Value::Array(items) => {
                            Value::Array(items.into_iter().map(sanitize_schema_booleans).collect())
                        }
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
                        Value::Array(items) => {
                            Value::Array(items.into_iter().map(sanitize_schema_booleans).collect())
                        }
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

    // what this catches: the yield verb must be REACHABLE — offered on the same surface
    // the citizens actually use. It exists precisely because `Pass` had no structured
    // channel and we compensated with prose matching for months; a verb that is built,
    // correct, and absent from the offer list would be that same bug wearing a new hat
    // ([[green-by-every-check-is-not-evidence-of-reachability]]). So this asserts
    // PRESENCE IN THE OFFERED LIST, not merely that the constructor returns something.
    #[test]
    fn the_yield_verb_is_actually_offered_to_the_model() {
        let offered = native_tool_specs();
        let yielded = offered
            .iter()
            .find(|s| s.name == VERDICT_YIELD_TURN)
            .unwrap_or_else(|| {
                panic!(
                    "yield_turn missing from the offered surface ({} tools): {:?}",
                    offered.len(),
                    offered.iter().map(|s| &s.name).collect::<Vec<_>>()
                )
            });
        // Argument-free on purpose: a `reason` field would invite the very closure
        // paragraph the verb exists to stop her broadcasting (#334's degeneration shape).
        assert_eq!(yielded.input_schema.schema_type, "object");
        assert_eq!(yielded.input_schema.properties, serde_json::json!({}));
        assert!(yielded.input_schema.required.is_none());
        // And the ACTION tools are still all there — the verdict verb is appended to the
        // one derived surface, never a replacement for it.
        assert!(
            offered.len() > 1,
            "verdict verb must ADD to the registry-derived tools, not replace them"
        );
    }

    // what this catches: the name→verb mapping is a NAME match on a verb we defined
    // (protocol), which is the whole reason this replaced a prose phrase-list. Aliases
    // resolve; an unrelated command must never be mistaken for a yield.
    #[test]
    fn yield_recognition_is_a_name_match_and_nothing_broader() {
        assert!(is_yield_turn(VERDICT_YIELD_TURN));
        for alias in VERDICT_YIELD_ALIASES {
            assert!(is_yield_turn(alias), "alias must resolve: {alias}");
        }
        for other in [
            "work/list",
            "code/write",
            "yield",
            "passing",
            "pass_the_config",
        ] {
            assert!(!is_yield_turn(other), "must NOT read as a yield: {other}");
        }
    }

    // what this catches: the native surface is DERIVED from each command's declared
    // `NATIVE` flag, not a hand-kept list — a command that declares `native: true` is
    // offered automatically, and a sibling AiSafe command that does NOT (e.g. code/glob)
    // is excluded. This is the anti-switch-statement contract: add a command, mark it
    // native in its OWN file, and it appears here with zero edits to this function. If
    // someone reintroduces a hardcoded array, or the flag stops being read, this breaks.
    #[test]
    fn native_surface_is_derived_from_the_per_command_native_flag() {
        let specs = native_tool_specs();
        let names: std::collections::HashSet<&str> =
            specs.iter().map(|s| s.name.as_str()).collect();

        // The declared core agentic working set is present (each opted in at its own site).
        for expected in [
            "commands/list",
            "commands/help",
            "code/search",
            "code/read",
            "code/edit",
            "code/write",
            "code/run",
            "code/shell",
            "code/git/status",
            "interface/screenshot",
            "perception/observe",
            "perception/look",
            "work/claim",
            // #358: the social sense. Pinned here because #339 proved a correct verb
            // that never declares NATIVE is invisible to every citizen — this list is
            // the reachability contract, not a nicety.
            "room/members",
        ] {
            assert!(
                names.contains(expected),
                "native surface must include declared-native {expected}"
            );
        }

        // A sibling AiSafe command that did NOT opt in is EXCLUDED — proving this is a
        // filter on the flag, not "every AiSafe command" (which would re-flood the window).
        assert!(
            !names.contains("code/glob"),
            "code/glob is AiSafe but not declared native — must stay catalog-only, not native"
        );

        // And it stays BOUNDED (the muting guardrail) — nowhere near the full registry.
        // < 40 → < 42, stated plainly (2026-09-01): activity/recipes + activity/invite
        // joined the native set (the spawn→invite→brief flow — see the agentic-surface
        // ceiling note in llm_deliberation_faculty for the full rationale + token cost).
        assert!(
            names.len() < 42,
            "native set stayed bounded ({} tools); a full dump would re-mute personas",
            names.len()
        );
    }

    // what this catches: the schema is a TEACHING surface, not a copy of the source
    // comment. schemars lifts `///` verbatim, so maintainer rationale (why the Rust
    // type is what it is, what the doc used to say) rode into every offered tool on
    // every turn — 550 tokens of the 26-verb native surface, measured. The lead
    // paragraph survives intact (truncating the ANSWER is #358's shape); everything
    // after the blank line is for whoever opens the file. Regression here = the
    // rationale coming back, or — worse — the lead sentence getting clipped.
    #[test]
    fn schema_descriptions_carry_the_lead_paragraph_not_the_rationale() {
        let schema = json!({
            "type": "object",
            "properties": {
                "recipe": {
                    "type": "string",
                    "description": "Which recipe to build from — the EXACT `purpose` key.\n\nNot enumerated here on purpose: recipes are DATA, so any list in this comment is stale the moment someone authors a new one."
                },
                // A param literally NAMED `description` must recurse, not be treated
                // as prose — the code/edit shape that a naive key-match would corrupt.
                "description": {
                    "type": "string",
                    "description": "Optional note describing the change.\n\nRecorded in the change history."
                }
            },
            "definitions": {
                "EditMode": {
                    "description": "How to edit.\n\nHistorical note nobody calling this needs."
                }
            }
        });

        let out = tool_input_schema_from(&schema);
        let recipe = out.properties["recipe"]["description"].as_str().unwrap();
        assert_eq!(
            recipe, "Which recipe to build from — the EXACT `purpose` key.",
            "lead paragraph must survive VERBATIM — clipping the answer makes the verb unusable"
        );
        assert!(
            !recipe.contains("stale the moment"),
            "maintainer rationale must not reach the offered schema: {recipe}"
        );

        // The param named `description` is a SCHEMA, so it keeps its own shape and
        // its nested prose is trimmed like any other field's.
        let named = &out.properties["description"];
        assert_eq!(named["type"], "string", "a param named `description` is not prose");
        assert_eq!(
            named["description"].as_str().unwrap(),
            "Optional note describing the change."
        );

        // definitions travel with the schema (llama.cpp resolves $refs against them)
        // and get the same treatment — they are offered prose too.
        let defs = out.definitions.expect("definitions must still ship");
        assert_eq!(defs["EditMode"]["description"].as_str().unwrap(), "How to edit.");
    }

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
        let expanded: std::collections::BTreeSet<String> =
            ["code".to_string()].into_iter().collect();
        let out = render_tool_menu(&tools, &expanded);

        // Spine: both categories present every turn.
        assert!(out.contains("code"), "code header missing: {out}");
        assert!(
            out.contains("gpu"),
            "gpu header missing (spine broken): {out}"
        );
        // Expanded code lists its verbs inline.
        assert!(
            out.contains("code: edit, read, run"),
            "code not expanded: {out}"
        );
        // Collapsed gpu shows depth + how to open, NOT its verbs.
        assert!(
            out.contains("gpu (2 — commands/list --filter gpu)"),
            "gpu not collapsed: {out}"
        );
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
        assert!(
            out.contains("run(lang, code, timeout_secs?)"),
            "run params wrong: {out}"
        );
        assert!(
            out.contains("search(pattern, path?)"),
            "search params wrong: {out}"
        );
        // A no-param verb renders bare — no empty `()`.
        assert!(
            out.contains("list,") || out.trim_end().ends_with("list"),
            "list should be bare: {out}"
        );
        assert!(
            !out.contains("list()"),
            "no-param verb must not render empty parens: {out}"
        );
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
        assert_eq!(
            specs.len(),
            registry_ai_safe,
            "surface == registry AiSafe count"
        );
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
        assert!(
            !spec.description.is_empty(),
            "tool carries a description handle"
        );
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
            .unwrap_or_else(|| {
                panic!(
                    "command {} has nested definitions that were dropped",
                    d.name
                )
            })
            .as_object()
            .expect("definitions is a JSON object map");
        assert!(
            !defs.is_empty(),
            "definitions map for {} must not be empty",
            d.name
        );

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
        assert_eq!(
            out["properties"]["blocked"],
            json!({ "not": {} }),
            "false → not-any"
        );
        assert_eq!(
            out["properties"]["tags"]["items"],
            json!({}),
            "items: true → {{}}"
        );
        assert_eq!(
            out["additionalProperties"],
            json!({}),
            "additionalProperties: true → {{}}"
        );
        assert_eq!(
            out["definitions"]["Open"],
            json!({}),
            "definition true → {{}}"
        );

        // keyword boolean is NOT a schema position — left exactly as-is
        assert_eq!(
            out["properties"]["name"]["nullable"],
            json!(true),
            "nullable untouched"
        );

        // and there is no bare `true`/`false` left anywhere in the serialized schema
        assert!(
            !out.to_string().contains("true")
                || out["properties"]["name"]["nullable"] == json!(true),
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
