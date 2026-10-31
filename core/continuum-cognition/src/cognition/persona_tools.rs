//! Dynamic persona tool surface — discovered, never hardcoded.
//!
//! A persona's hands are the registry's `AiSafe` commands. The tool list is a
//! PURE FUNCTION of `command_registry_live() × access_level`: every command that
//! declares [`AccessLevel::AiSafe`] becomes a tool the persona can call, and a
//! new ai-safe command appears → the persona can use it with zero code change.
//! There is NO hardcoded tool list and NO parallel allow-table — the command's
//! own `access_level` (its "destiny") is the single, consistent source of truth,
//! surfaced through the one central listing [`command_registry`].
//!
//! ## The bug this shape refuses to ship (Joel 2026-06-21)
//! "The tool's gone and no one knows why — RAG says it has it, some smell
//! somewhere restricted it." Because the surface is exactly
//! `command_registry_live().filter(AiSafe)`, "why isn't tool X here?" has ONE answer:
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
use crate::routing::command_acl::is_command_authorized;
use crate::routing::trust_level::TrustLevel;
use crate::sdk_codegen::{AccessLevel, CommandDescriptor};
use crate::sdk_codegen::ext::command_registry_live;
use serde_json::json;
use std::collections::BTreeMap;
use std::fmt::Write as _;


/// The persona's tool surface: every command it is AUTHORIZED to run at `trust`,
/// projected to a tool spec. **Offer == authorized, by construction** — a persona
/// is NEVER shown a tool the gate would refuse (no "offer ping then deny it"). The
/// SAME [`is_command_authorized`] the executor enforces decides what's offered, so
/// the two can't drift, and opening a command to a trust level auto-adds it here.
/// Dynamic from the live [`command_registry`]; nothing hardcoded.
pub fn authorized_tool_specs(trust: TrustLevel) -> Vec<NativeToolSpec> {
    command_registry_live()
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
#[cfg(any(test, feature = "test-fixtures"))]
pub fn ai_safe_tool_specs() -> Vec<NativeToolSpec> {
    command_registry_live()
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
            .unwrap_or(&t.name); // unwrap_or: an uncategorised name renders whole
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
        .unwrap_or(&[]) // unwrap_or: a schema with no required list requires nothing
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
        .unwrap_or_default(); // unwrap_or_default: a schema with no properties has no optional fields
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
            .unwrap_or(&t.name); // unwrap_or: an uncategorised name renders whole
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

/// Group the authorized tools into `(category, verbs)` pairs for the tool menu. Verbs are the BARE names (no param hints). Stable (BTreeMap) order
/// so the category list matches the spine order [`render_tool_menu`] emits.
pub fn group_categories(tools: &[NativeToolSpec]) -> Vec<(&str, Vec<&str>)> {
    let mut by_cat: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for t in tools {
        let cat = extract_category(&t.name);
        let verb = t
            .name
            .strip_prefix(cat)
            .and_then(|r| r.strip_prefix('/'))
            .unwrap_or(&t.name); // unwrap_or: an uncategorised name renders whole
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
    command_registry_live()
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
    command_registry_live()
        .iter()
        .find(|d| d.name == name)
        .map(descriptor_to_tool_spec)
}

// The on-demand "how do I call this?" tool is the EXISTING `commands/help`
// ([`CommandsHelp`] in `commands::help`), referenced by [`TOOL_HELP_NAME`].
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
            .unwrap_or("object") // unwrap_or: JSON Schema params with no declared type are an object
            .to_string(),
        properties: schema
            .get("properties")
            .map(|p| lead_paragraph_descriptions(p.clone()))
            .unwrap_or_else(|| json!({})), // unwrap_or_else: a command with no properties takes an empty object
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
                            let lead = text.split("\n\n").next().unwrap_or(text).trim_end(); // unwrap_or: split always yields one piece; the text itself is the lead
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
pub const SCHEMA_VALUED_KEYS: &[&str] = &[
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
pub const SCHEMA_MAP_KEYS: &[&str] = &[
    "properties",
    "patternProperties",
    "definitions",
    "$defs",
    "dependentSchemas",
];
/// Keywords whose value is an ARRAY of schemas.
pub const SCHEMA_ARRAY_KEYS: &[&str] = &["allOf", "anyOf", "oneOf", "prefixItems"];

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
    for d in command_registry_live() {
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
mod tests;
