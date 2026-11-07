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
//! point shares — the persona tool-call path, the `uu` CLI, MCP. The mapping is
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
use crate::ai::tool_usage::{record, Outcome};
use crate::sdk_codegen::ext::command_registry_live;

/// alias → canonical command name. Built ONCE from every command's declared
/// `ALIASES`. A name claimed by two commands panics at init (fail-loud, like the
/// registry's duplicate-NAME guard) — an ambiguous alias is a bug, not a
/// silent last-writer-wins.
pub fn alias_to_command() -> &'static HashMap<&'static str, &'static str> {
    static IDX: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    IDX.get_or_init(|| {
        let mut m: HashMap<&'static str, &'static str> = HashMap::new();
        for d in command_registry_live() {
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

/// canonical command name → its PRIMARY offered alias ([`primary_alias`], the FIRST declared):
/// the trained reflex we rename to on the wire. Commands with no alias are absent.
fn command_to_primary_alias() -> &'static HashMap<&'static str, &'static str> {
    static IDX: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    IDX.get_or_init(|| {
        command_registry_live()
            .iter()
            .filter_map(|d| primary_alias(d.aliases).map(|alias| (d.name, alias)))
            .collect()
    })
}

/// The offer policy and the one wire-name function live beside the command declaration
/// (`sdk_codegen::verb_ref`), so a verb her text names (`VerbRef`) renders exactly as her
/// offered tool list does. Re-exported here, the dialect's surface.
pub use crate::sdk_codegen::{offer_style_for, OfferStyle};
use crate::sdk_codegen::{primary_alias, wire_name};

/// Rename a spec for the wire under an explicit offer style. Always yields a
/// charset-legal name (never a raw slash), so the offered surface is wire-valid
/// regardless of style. See [`OfferStyle`].
pub fn to_wire_spec_with(mut spec: NativeToolSpec, style: OfferStyle) -> NativeToolSpec {
    spec.name = offered_name(&spec.name, style);
    spec
}

/// The wire name of the canonical command `name` under `style` ([`wire_name`]).
fn offered_name(name: &str, style: OfferStyle) -> String {
    wire_name(name, command_to_primary_alias().get(name).copied(), style)
}

/// The name her tool list offers a STORED verb under: a name recorded before a rename resolves
/// through the alias the rename kept ([`resolve_wire_name`]) to the command it names now, and
/// renders as that command is offered today. A record never re-renders a name she cannot call.
pub fn offered_name_of_stored(stored: &str) -> String {
    offered_name(&resolve_wire_name(stored), offer_style_for(None))
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
    NAMES.get_or_init(|| command_registry_live().iter().map(|d| d.name).collect())
}

/// A verb's purpose and what to use instead, shared with error feedback without erasing the
/// real failure or declaring a discoverable command impossible to use. Guidance, NOT
/// authorization.
pub struct WithheldVerb {
    pub why: &'static str,
    pub instead: &'static str,
}

/// What the executor tells a caller who reached a verb aimed at someone else's work (a peer's
/// diff, a review verdict): its purpose and the verb for her own work. Guidance only; whether a
/// verb is among her hands is the value its command declares (`CommandSpec::DISCLOSURE`).
pub fn withheld_from_hands(name: &str) -> Option<WithheldVerb> {
    match name.replace('_', "/").as_str() {
        "code/git/apply" => Some(WithheldVerb {
            why: "`code/git/apply` applies a peer's unified diff; it is not the command for authoring your own edits.",
            instead: "Your edits land with `code/edit` and `code/write`; commit them with `code/git/commit`.",
        }),
        _ => None,
    }
}

/// What sat in the tool NAME field when it was not a verb at all. The executor's
/// did-you-mean answers a NEAR verb; these two are not near anything, and before this
/// the receipt was "call `commands/help`" — a turn spent with no lesson (card b579a9c7,
/// Kimi 2026-09-26: `⚙ C:\Users\...\.airc/worktrees\d33e928a ✗`, then `⚙ msg_budget ✗`,
/// a probe field she had just read). Both are one model-authoring slip at the envelope:
/// the thing she was ABOUT (a path, a symbol) landed where the verb goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotAVerb {
    /// A filesystem path: a drive letter, a backslash, a leading `/` `~` `.`, or a
    /// `dir/file.ext` shape. No command name has any of these.
    Path,
    /// A bare identifier (no `/` in what she wrote) that did-you-mean could not place
    /// near any verb or alias — a field, a variable, a symbol from what she was reading.
    Identifier,
}

/// Classify a wire name that the registry did not know. `near_a_verb` is whether
/// did-you-mean found ANY candidate for it — a bare identifier near a real verb is a
/// typo and keeps the did-you-mean lesson; only an identifier near nothing is named as
/// one. A path is a path whatever did-you-mean says.
pub fn not_a_verb(attempted: &str, near_a_verb: bool) -> Option<NotAVerb> {
    let name = attempted.trim();
    if name.is_empty() {
        return None;
    }
    let bytes = name.as_bytes();
    let drive_letter = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    let leading = name.starts_with('/') || name.starts_with('~') || name.starts_with('.');
    let last_segment_has_extension = name.contains('/')
        && name
            .rsplit('/')
            .next()
            .and_then(|seg| seg.rsplit_once('.'))
            .is_some_and(|(stem, ext)| {
                !stem.is_empty()
                    && (1..=6).contains(&ext.len())
                    && ext.chars().all(|c| c.is_ascii_alphanumeric())
            });
    if drive_letter || name.contains('\\') || leading || last_segment_has_extension {
        return Some(NotAVerb::Path);
    }
    if !name.contains('/') && !near_a_verb {
        return Some(NotAVerb::Identifier);
    }
    None
}

/// Every declared alias whose command is AiSafe — the trained-reflex vocabulary a
/// persona might reach for. Used to WIDEN did-you-mean candidates on a miss, so a
/// reflex like `grep_files` finds `grep` (→ `code/search`) instead of no match.
/// The caller maps a suggested alias back to its canonical command with
/// [`resolve_wire_name`]. Static, built once from the live registry.
pub fn ai_safe_aliases() -> &'static [&'static str] {
    static IDX: OnceLock<Vec<&'static str>> = OnceLock::new();
    IDX.get_or_init(|| {
        command_registry_live()
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
    // A namespace the model writes in front of a real command: `tools/code/run`.
    // Measured 2026-10-04 on peer b6dcfc8e: every act all afternoon was dispatched as
    // the URI `tools/code/run`, the ACL correctly refused it ("no policy grants access
    // to URI"), and she concluded she would "rather not create or execute files" — a
    // verb she can see but cannot use. The remainder is the command she meant.
    if let Some(rest) = wire.strip_prefix("tools/") {
        if command_names().contains(rest) {
            return (rest.to_string(), Outcome::Alias);
        }
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
/// shared resolver every dispatch seam (the `uu` CLI, the IPC/MCP socket route)
/// funnels through so a trained reflex / former name / charset-legal form resolves
/// the SAME way it does on the persona path. No recording here: these surfaces
/// carry infra traffic (`commands/list`, health pings) that would drown the
/// tool-call signal, and the persona path already tallies at [`from_wire_name`].
pub fn resolve_wire_name(wire: &str) -> String {
    classify(wire).0
}

/// Map a wire tool-call name back to the canonical command, and TALLY the outcome
/// (`ai::tool_usage`) so a benchmark run surfaces exactly what each model
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

    /// A command that was renamed: its current name, its trained reflex first, and the name it
    /// carried before the move kept as an alias (the rename rule).
    mod renamed {
        #[derive(serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
        pub struct P {}
        #[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)]
        pub struct R {}
        crate::action_command! {
            /// Test fixture: a renamed reopen verb.
            pub struct Renamed;
            name: "fixture/reopen-now",
            access: AiSafe,
            native: true,
            aliases: &["fixture_reopen", "fixture/reopen-before"],
            disclosure: Always,
            params: P,
            output: R,
            run(_this, _ctx, _p) => { Ok(R {}) }
        }
        continuum_kernel::projects_success_on_return! { Renamed => R }
        pub const RENAMED: crate::sdk_codegen::VerbRef<Renamed> = crate::sdk_codegen::VerbRef::always();
    }

    // what this catches: a rename leaving her text, her offered list and her records disagreeing.
    // A reference by type renders as her tool list offers the command (its first alias, never
    // the slash name); a call by the old name still resolves to it; and a name stored before the
    // rename renders as the command is offered now.
    #[test]
    fn a_renamed_verb_renders_its_current_offer_and_its_old_name_still_resolves() {
        let offered = renamed::RENAMED.offered();
        assert_eq!(offered, "fixture_reopen");
        let Ok(spec) = serde_json::from_value::<NativeToolSpec>(serde_json::json!({
            "name": renamed::RENAMED.canonical(),
            "description": "",
            "input_schema": {"type": "object", "properties": {}},
        })) else {
            panic!("a spec for the fixture")
        };
        assert_eq!(to_wire_spec(spec).name, offered, "her text names what her list offers");
        assert_eq!(resolve_wire_name("fixture/reopen-before"), "fixture/reopen-now");
        assert_eq!(offered_name_of_stored("fixture/reopen-before"), offered);
        assert_eq!(offered_name_of_stored("fixture/reopen-now"), offered);
    }

    /// Signature-carrying names (#326). A model copies the prose menu's
    /// `work/list(claimable?, state?)` shape into the NAME slot; the adapter must meet
    /// it rather than let a URI-with-args reach the ACL, which correctly refuses it.
    mod signature_names {
        use super::*;

        fn call(name: &str, input: serde_json::Value) -> crate::ai::types::ToolCall {
            crate::ai::types::ToolCall {
                id: "t1".into(),
                name: name.into(),
                input,
            }
        }


        #[test]
        fn a_name_carrying_its_arguments_resolves_and_keeps_the_arguments() {
            let mut c = call("work/list(state=open)", serde_json::json!({}));
            let note = normalize_call(&mut c).expect("repaired, so it must explain itself");
            assert_eq!(
                c.name, "work/list",
                "the head must resolve to the real command"
            );
            assert_eq!(
                c.input.get("state").and_then(|v| v.as_str()),
                Some("open"),
                "arguments welded into the name are real intent — they must survive"
            );
            assert!(
                note.contains("work/list"),
                "the note names the canonical form: {note}"
            );
        }

        // what this catches: silently clobbering a well-formed call. Explicit input is
        // authoritative; the name is only a fallback source.
        #[test]
        fn explicit_input_always_wins_over_arguments_echoed_in_the_name() {
            let mut c = call(
                "work/list(state=open)",
                serde_json::json!({"state": "claimed"}),
            );
            normalize_call(&mut c);
            assert_eq!(
                c.input.get("state").and_then(|v| v.as_str()),
                Some("claimed")
            );
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
}
