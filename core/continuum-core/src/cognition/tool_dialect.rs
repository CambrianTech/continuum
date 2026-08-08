//! tool_dialect — the ADAPTER between our command namespace and the tool-call
//! dialect models were trained on. [[joel-boundary-design-values]]: "always
//! adapters — meet the model ergonomically, never hardcode around it."
//!
//! ## The design (Joel, 2026-07-19): the command owns its own aliases
//!
//! A model reaches for the names it was trained on (`read_file`, `bash`, `grep`)
//! and for former names of tools that have since moved. Rather than a central
//! table that drifts, EACH command declares what it answers to — its
//! [`ALIASES`](crate::sdk_codegen::command::ActionCommand::ALIASES), right in its
//! own file. A command is then fully portable: rename or move it and its aliases
//! travel with it; no second source of truth to keep in sync.
//!
//! This module just AGGREGATES those per-command declarations into two generated
//! indices (built once, cached), and is the surface-agnostic core every entry
//! point shares — the persona tool-call path, the `cu` CLI, MCP. The mapping is
//! one thing; each surface renders it in its own native form.
//!
//! - [`from_wire_name`] — a wire tool-call name → the canonical command. Trained
//!   reflex / former name resolves; a canonical or unknown name passes through
//!   untouched (the adapter only ever WIDENS the surface).
//! - [`to_wire_spec`] — rename a spec to the model's primary reflex on OFFER
//!   (`code/read` → `read_file`), the name it acts on without learning a menu.
//!
//! A tool-call name two commands both claim is a build-time panic — the same
//! fail-loud the registry uses for duplicate command NAMES.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::ai::types::NativeToolSpec;
use crate::cognition::tool_usage::{record, Outcome};
use crate::sdk_codegen::command_registry;

/// alias → canonical command name. Built ONCE from every command's declared
/// `ALIASES`. A name claimed by two commands panics at init (fail-loud, like the
/// registry's duplicate-NAME guard) — an ambiguous alias is a bug, not a
/// silent last-writer-wins.
fn alias_to_command() -> &'static HashMap<&'static str, &'static str> {
    static IDX: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    IDX.get_or_init(|| {
        let mut m: HashMap<&'static str, &'static str> = HashMap::new();
        for d in command_registry() {
            for &alias in d.aliases {
                if let Some(prev) = m.insert(alias, d.name) {
                    panic!(
                        "tool_dialect: tool-call name '{alias}' is claimed by both '{prev}' \
                         and '{}' — a command's ALIASES must be unique across the registry.",
                        d.name
                    );
                }
            }
        }
        m
    })
}

/// canonical command name → its PRIMARY offered alias (the FIRST declared) — the
/// trained reflex we rename to on the wire. Commands with no alias are absent
/// (offered under their canonical name).
fn command_to_primary_alias() -> &'static HashMap<&'static str, &'static str> {
    static IDX: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    IDX.get_or_init(|| {
        command_registry()
            .iter()
            .filter_map(|d| d.aliases.first().map(|&alias| (d.name, alias)))
            .collect()
    })
}

/// The policy for how a command is NAMED on the wire when OFFERED to a model —
/// the seam future per-model / per-persona selection plugs into
/// ([[adaptive-tool-surface-meets-you-in-the-middle]]). Chosen by
/// [`offer_style_for`]; either way, BOTH the offered form and the trained alias
/// resolve on the way back in ([`from_wire_name`]), so switching styles never
/// narrows what a model can call — it only changes what it's SHOWN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferStyle {
    /// Offer the model's trained reflex — the command's primary alias (`code/read`
    /// → `read_file`). Meet their training; no menu to learn.
    TrainedReflex,
    /// Offer OUR canonical name, charset-legal (`code/read` → `code_read`). Converge
    /// the model onto our semantically-grouped namespace; the trained alias still
    /// resolves inbound, so it's meet-in-the-middle, not a hard cutover.
    Canonical,
}

/// The offer policy for a served model — THE per-model adapter seam. Today it
/// returns the global default for every model; the foresight is that a model whose
/// LoRA is tuned on the trained names is met with `TrainedReflex` while a fresh /
/// base model could be offered `Canonical` to learn our namespace. That logic lands
/// HERE, once, instead of scattered at the call sites.
pub fn offer_style_for(_model: Option<&str>) -> OfferStyle {
    // DEFAULT: meet their training. The living personas' coder LoRAs are tuned on the
    // trained names (`read_file`, `bash`); ~99.9% of live tool calls already resolve
    // ([[tool-naming-meet-their-training-alias-or-redirect]]) and tool-naming is NOT
    // the score bottleneck. So we offer the trained reflex — no blind change to the
    // tuned surface.
    //
    // Canonical (`code_read`) is a HYPOTHESIS, not yet validated: the only offer-surface
    // gate we can run today is humaneval-rs, which is PURE CODEGEN and offers NO tools
    // (needs_tools is false for spoken-graded tasks), so it structurally CANNOT measure
    // whether renaming the offered tools helps or hurts. Flipping the global default to
    // Canonical on that gate was a blind flip of the tuned surface — the exact move the
    // hard constraint forbids. Canonical stays available via `to_wire_spec_with` and is
    // adopted here (per-model or globally) only once a TOOL-USING A/B (#204) validates it.
    OfferStyle::TrainedReflex
}

/// Charset-legal wire form of a canonical command name: `/` → `_` (`code/read` →
/// `code_read`), matching the OpenAI function-name charset our slashed names
/// violate. Round-trips: [`from_wire_name`] maps the underscore form back to the
/// slashed command.
fn charset_legal(name: &str) -> String {
    name.replace('/', "_")
}

/// Rename a spec for the wire under an explicit offer style. Always yields a
/// charset-legal name (never a raw slash), so the offered surface is wire-valid
/// regardless of style. See [`OfferStyle`].
pub fn to_wire_spec_with(mut spec: NativeToolSpec, style: OfferStyle) -> NativeToolSpec {
    spec.name = match style {
        OfferStyle::TrainedReflex => command_to_primary_alias()
            .get(spec.name.as_str())
            .map(|&alias| alias.to_string())
            // No trained alias: the long tail is offered under its charset-legal
            // canonical (reachable, just not a trained reflex) — never a raw slash.
            .unwrap_or_else(|| charset_legal(&spec.name)),
        OfferStyle::Canonical => charset_legal(&spec.name),
    };
    spec
}

/// Rename a spec for the wire under the DEFAULT offer policy ([`offer_style_for`]
/// with no model context). The per-model path calls [`to_wire_spec_with`] with the
/// style resolved from the served model.
pub fn to_wire_spec(spec: NativeToolSpec) -> NativeToolSpec {
    to_wire_spec_with(spec, offer_style_for(None))
}

/// The set of canonical command names — for classifying a wire name as CANONICAL
/// (a real command) vs a MISS. Built once from the live registry.
fn command_names() -> &'static HashSet<&'static str> {
    static NAMES: OnceLock<HashSet<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| command_registry().iter().map(|d| d.name).collect())
}

/// Every declared alias whose command is AiSafe — the trained-reflex vocabulary a
/// persona might reach for. Used to WIDEN did-you-mean candidates on a miss, so a
/// reflex like `grep_files` finds `grep` (→ `code/search`) instead of no match.
/// The caller maps a suggested alias back to its canonical command with
/// [`resolve_wire_name`]. Static, built once from the live registry.
pub fn ai_safe_aliases() -> &'static [&'static str] {
    static IDX: OnceLock<Vec<&'static str>> = OnceLock::new();
    IDX.get_or_init(|| {
        command_registry()
            .iter()
            .filter(|d| d.access_level == crate::sdk_codegen::AccessLevel::AiSafe)
            .flat_map(|d| d.aliases.iter().copied())
            .collect()
    })
    .as_slice()
}

/// Classify a wire tool-call name into (canonical command, how it resolved) —
/// the PURE core, no side effects. Resolves, in order: a declared reflex/former
/// alias; our canonical name as-is; our charset-legal name (`code_read` →
/// `code/read`). An unknown name passes through untouched (classified a MISS) —
/// the adapter widens the surface, never narrows it. Idempotent:
/// `classify(canonical) == (canonical, Canonical)`, so it's safe to apply at
/// every dispatch seam without changing an already-resolved name.
fn classify(wire: &str) -> (String, Outcome) {
    // A name carrying its OWN ARGUMENTS — `work/list(state=open)`, `read_file(path=x)`.
    // Models produce this constantly: the prose tool menu renders `work/list(claimable?,
    // state?)`, so copying that shape into the name slot is the trained-reflex thing to
    // do, not a malfunction. Before 2026-08-07 it fell through as a MISS and was
    // dispatched AS A URI, where the ACL correctly refused `work/list(state=open)`
    // because no policy can grant a path with args welded on. The gate was right; the
    // adapter was asleep. Two citizens read that refusal, reported "no open tasks", and
    // it read as a cognition defect for a week (#326).
    //
    // Meet it: resolve the HEAD. Args are recovered separately by [`normalize_call`],
    // which is what the ToolCall seams use — this function only owns the NAME.
    if let (head, Some(_)) = split_signature(wire) {
        if head != wire {
            let (name, outcome) = classify(head);
            // A resolved head is a real hit; an unresolved one stays a miss (so the
            // usage tally still reports the vocabulary gap honestly).
            return (name, outcome);
        }
    }
    if let Some(&cmd) = alias_to_command().get(wire) {
        return (cmd.to_string(), Outcome::Alias);
    }
    if command_names().contains(wire) {
        return (wire.to_string(), Outcome::Canonical);
    }
    // Our charset-legal name (`code_read` → `code/read`) — the underscore form of a
    // real command. Resolving it HERE (not just in the executor's naive replace)
    // classifies it CANONICAL instead of a false miss.
    if wire.contains('_') {
        let slashed = wire.replace('_', "/");
        if command_names().contains(slashed.as_str()) {
            return (slashed, Outcome::Canonical);
        }
    }
    (wire.to_string(), Outcome::Miss)
}

/// Resolve a wire tool-call name to the canonical command WITHOUT tallying — the
/// shared resolver every dispatch seam (the `cu` CLI, the IPC/MCP socket route)
/// funnels through so a trained reflex / former name / charset-legal form resolves
/// the SAME way it does on the persona path. No recording here: these surfaces
/// carry infra traffic (`commands/list`, health pings) that would drown the
/// tool-call signal, and the persona path already tallies at [`from_wire_name`].
pub fn resolve_wire_name(wire: &str) -> String {
    classify(wire).0
}

/// Map a wire tool-call name back to the canonical command, and TALLY the outcome
/// (`cognition::tool_usage`) so a benchmark run surfaces exactly what each model
/// reached for. This is the RECORDING resolver — the persona tool-call path uses
/// it (every miss/alias/canonical is a training + ergonomics signal). Other
/// surfaces use [`resolve_wire_name`] (same mapping, no tally).
pub fn from_wire_name(wire: &str) -> String {
    let (name, outcome) = classify(wire);
    record(wire, outcome);
    name
}

/// Split a wire name that carries its own call signature — `work/list(state=open)`
/// → `("work/list", Some("state=open"))`. Returns `(wire, None)` for an ordinary
/// name, so callers can apply it unconditionally.
///
/// Deliberately conservative: requires a `(` with a matching final `)`, and refuses
/// an empty head (`(foo)` is not a call). `name()` yields `Some("")`, which parses
/// to an empty arg map — a real no-arg call, not a miss.
pub fn split_signature(wire: &str) -> (&str, Option<&str>) {
    let w = wire.trim();
    if !w.ends_with(')') {
        return (wire, None);
    }
    let Some(open) = w.find('(') else {
        return (wire, None);
    };
    let head = w[..open].trim();
    if head.is_empty() {
        return (wire, None);
    }
    (head, Some(w[open + 1..w.len() - 1].trim()))
}

/// Repair a tool call whose NAME carries its arguments, and say what changed.
///
/// This is the adapter half that [`classify`] cannot do alone: `classify` owns the
/// name, but the arguments welded into that name are real intent and must not be
/// dropped. Merges them into `input` — **explicit `input` fields always win**, so a
/// well-formed call is never overwritten by a stray echo in the name.
///
/// Returns `Some(note)` when something was repaired, so the caller can tell the
/// citizen what we accepted and what the canonical form is (Joel's rule: aliases
/// resolve AND say so, #328). Returns `None` when the call was already clean —
/// idempotent, safe to apply at every seam.
pub fn normalize_call(call: &mut crate::ai::types::ToolCall) -> Option<String> {
    let (head, args) = split_signature(&call.name);
    let head = head.to_string();
    let Some(args) = args else {
        // Ordinary name: resolve + tally exactly as before. ONE call at each seam.
        call.name = from_wire_name(&head);
        return None;
    };

    let parsed = crate::ai::json_in_prompt_tools::paren_call_args(args);
    let canonical = from_wire_name(&head);

    // Merge: keep every explicit input field, fill only what's missing from the name.
    let mut merged = 0usize;
    if let Some(from_name) = parsed {
        if !from_name.is_empty() {
            let obj = match call.input.as_object_mut() {
                Some(o) => o,
                None => {
                    call.input = serde_json::Value::Object(serde_json::Map::new());
                    call.input.as_object_mut().expect("just set to an object")
                }
            };
            for (k, v) in from_name {
                if !obj.contains_key(&k) {
                    obj.insert(k, v);
                    merged += 1;
                }
            }
        }
    }

    let spoken = call.name.clone();
    call.name = canonical.clone();
    Some(if merged > 0 {
        format!(
            "accepted `{spoken}` — the arguments were part of the name, so I read them as \
             parameters ({merged} recovered). The canonical form is `{canonical}` with its \
             arguments passed separately."
        )
    } else {
        format!(
            "accepted `{spoken}` — the canonical form is `{canonical}` with arguments passed \
             separately."
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Signature-carrying names (#326). A model copies the prose menu's
    /// `work/list(claimable?, state?)` shape into the NAME slot; the adapter must meet
    /// it rather than let a URI-with-args reach the ACL, which correctly refuses it.
    mod signature_names {
        use super::*;

        fn call(name: &str, input: serde_json::Value) -> crate::ai::types::ToolCall {
            crate::ai::types::ToolCall { id: "t1".into(), name: name.into(), input }
        }

        // what this catches: THE live #326 defect. Before the adapter handled this,
        // `work/list(state=open)` classified as a MISS, passed through untouched, and was
        // dispatched as a URI — "forbidden: no policy grants access to URI:
        // work/list(state=open)". Two citizens read that as "no open tasks".
        #[test]
        fn a_name_carrying_its_arguments_resolves_and_keeps_the_arguments() {
            let mut c = call("work/list(state=open)", serde_json::json!({}));
            let note = normalize_call(&mut c).expect("repaired, so it must explain itself");
            assert_eq!(c.name, "work/list", "the head must resolve to the real command");
            assert_eq!(
                c.input.get("state").and_then(|v| v.as_str()),
                Some("open"),
                "arguments welded into the name are real intent — they must survive"
            );
            assert!(note.contains("work/list"), "the note names the canonical form: {note}");
        }

        // what this catches: silently clobbering a well-formed call. Explicit input is
        // authoritative; the name is only a fallback source.
        #[test]
        fn explicit_input_always_wins_over_arguments_echoed_in_the_name() {
            let mut c = call("work/list(state=open)", serde_json::json!({"state": "claimed"}));
            normalize_call(&mut c);
            assert_eq!(c.input.get("state").and_then(|v| v.as_str()), Some("claimed"));
        }

        // what this catches: an alias that ALSO carries args — both halves of the adapter
        // have to compose, or `list_tasks(state=open)` still dies.
        #[test]
        fn an_alias_carrying_arguments_resolves_through_both_halves() {
            let mut c = call("list_tasks(state=open)", serde_json::json!({}));
            normalize_call(&mut c);
            assert_eq!(c.name, "work/list");
            assert_eq!(c.input.get("state").and_then(|v| v.as_str()), Some("open"));
        }

        // what this catches: idempotence. normalize_call runs at every seam; a clean call
        // must pass through untouched and report NO repair (or the citizen gets told we
        // fixed something we didn't).
        #[test]
        fn an_ordinary_call_is_untouched_and_reports_no_repair() {
            let mut c = call("work/list", serde_json::json!({"state": "open"}));
            assert!(normalize_call(&mut c).is_none(), "nothing was repaired");
            assert_eq!(c.name, "work/list");
            assert_eq!(c.input.get("state").and_then(|v| v.as_str()), Some("open"));
        }

        // what this catches: `name()` is a real no-arg call, not a parse failure.
        #[test]
        fn an_empty_signature_is_a_no_arg_call() {
            let mut c = call("work/list()", serde_json::json!({}));
            normalize_call(&mut c);
            assert_eq!(c.name, "work/list");
        }

        // what this catches: over-eager splitting. A head-less parenthetical is not a
        // call, and must not be mangled into one.
        #[test]
        fn a_headless_parenthetical_is_not_a_call() {
            assert_eq!(split_signature("(whatever)"), ("(whatever)", None));
            assert_eq!(split_signature("work/list"), ("work/list", None));
        }
    }

    fn spec(n: &str) -> NativeToolSpec {
        NativeToolSpec {
            name: n.to_string(),
            description: String::new(),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
                definitions: None,
            },
        }
    }

    // what this catches: the per-command declarations aggregate into a working
    // round-trip over the LIVE registry under BOTH offer styles — a hot verb offers
    // under its trained reflex OR our charset-legal canonical, and EITHER offered
    // form maps back to the same canonical command. This is the decentralized
    // replacement for the old global DIALECT table; if a command's ALIASES stops
    // being read, or the round-trip stops being style-symmetric, these break.
    #[test]
    fn per_command_aliases_round_trip_under_both_offer_styles() {
        for (canonical, reflex) in [
            ("code/shell", "bash"),
            ("code/read", "read_file"),
            ("code/write", "write_file"),
            ("code/run", "run_code"),
            ("code/list", "list_files"),
            ("work/claim", "claim_task"),
            ("work/list", "list_tasks"),
            ("work/get", "get_task"),
        ] {
            // TrainedReflex offers the alias; it maps back to canonical.
            assert_eq!(
                to_wire_spec_with(spec(canonical), OfferStyle::TrainedReflex).name,
                reflex,
                "TrainedReflex offers {canonical} as {reflex}"
            );
            assert_eq!(from_wire_name(reflex), canonical, "map {reflex} back to {canonical}");
            // Canonical offers OUR name charset-legal; it maps back to canonical too.
            let canon_wire = canonical.replace('/', "_");
            assert_eq!(
                to_wire_spec_with(spec(canonical), OfferStyle::Canonical).name,
                canon_wire,
                "Canonical offers {canonical} as {canon_wire}"
            );
            assert_eq!(
                from_wire_name(&canon_wire),
                canonical,
                "map {canon_wire} back to {canonical}"
            );
        }
        // A canonical name a model emits directly still resolves (never narrows).
        assert_eq!(from_wire_name("code/read"), "code/read");
        // An unknown name passes through — the executor fails it loud with a
        // did-you-mean; the adapter never invents a route.
        assert_eq!(from_wire_name("frobnicate"), "frobnicate");
    }

    // what this catches: the DEFAULT offer policy MEETS THEIR TRAINING — a hot verb is
    // offered under the model's trained reflex (`read_file`), NOT our canonical
    // `code_read`. Canonical is a hypothesis not yet validated by a tool-using A/B
    // (#204): the only offer-surface gate we can run (humaneval-rs) is pure codegen and
    // offers no tools, so it can't measure the change. If someone flips the default to
    // Canonical without that validation, this fails loudly — the guard against a blind
    // flip of the tuned surface. [[tool-naming-meet-their-training-alias-or-redirect]]
    #[test]
    fn default_offer_policy_meets_their_training() {
        assert_eq!(offer_style_for(None), OfferStyle::TrainedReflex);
        assert_eq!(to_wire_spec(spec("code/read")).name, "read_file");
        assert_eq!(to_wire_spec(spec("work/claim")).name, "claim_task");
        // Canonical is still REACHABLE as an explicit policy (the seam is intact) —
        // it's just not the blind default.
        assert_eq!(
            to_wire_spec_with(spec("code/read"), OfferStyle::Canonical).name,
            "code_read"
        );
    }

    // what this catches: the socket/CLI resolver (used at Runtime::route_command and
    // the `cu` entry) maps IDENTICALLY to the persona path — alias, canonical
    // (idempotent), charset-legal, unknown-passthrough — so `cu` / IPC / MCP accept
    // the same vocabulary a persona does. Non-recording is a structural guarantee
    // (resolve_wire_name == classify().0, and classify never calls record), so it's
    // asserted by construction, not by racy global-tally inspection.
    #[test]
    fn resolve_wire_name_maps_like_the_persona_path() {
        assert_eq!(resolve_wire_name("read_file"), "code/read"); // trained alias
        assert_eq!(resolve_wire_name("code/read"), "code/read"); // canonical, idempotent
        assert_eq!(resolve_wire_name("code_read"), "code/read"); // charset-legal
        assert_eq!(resolve_wire_name("frobnicate"), "frobnicate"); // unknown passes through
    }

    // what this catches: the index is NON-VACUOUS — the migration actually landed
    // aliases on commands. A regression that dropped the ALIASES consts (or the
    // descriptor plumbing) would empty this and silently stop all reflex mapping.
    #[test]
    fn the_alias_index_is_populated() {
        assert!(
            alias_to_command().len() >= 10,
            "expected the hot-verb aliases from the migrated commands, got {}",
            alias_to_command().len()
        );
        // Spot-check the highest-frequency live reflexes (mined 2026-07-19).
        assert_eq!(from_wire_name("file_tree"), "code/tree");
        assert_eq!(from_wire_name("read_file"), "code/read");
        // Git reflexes declared via the action_command! macro's `aliases:` clause
        // (#202 Slice 6) — a model reaching for the industry-standard `git_status`
        // resolves to our `code/git/status`. If the macro's aliases plumbing breaks,
        // these are the canary.
        assert_eq!(from_wire_name("git_status"), "code/git/status");
        assert_eq!(from_wire_name("git_commit"), "code/git/commit");
        assert_eq!(from_wire_name("git_diff"), "code/git/diff");
    }
}
