#![cfg(test)]
//! From `cognition::persona_tools`: the native surface derived from the REAL registry.

use crate::cognition::persona_tools::*;
use crate::sdk_codegen::ext::command_registry_live;
use crate::sdk_codegen::AccessLevel;

// what this catches (Kimi, 2026-10-05, twice): her self-determination verbs were
// registered, documented, AiSafe, on the hands allowlist (#4755) — and never in the
// offered list, because NATIVE defaults to false and nobody set it. The allowlist
// filters the offered list; a verb absent from the offered list is uncallable
// whatever the allowlist says. This asserts PRESENCE IN THE OFFER, like the yield test.
#[test]
fn her_self_determination_verbs_are_actually_offered_to_the_model() {
    let offered = native_tool_specs();
    for verb in ["focus/continue", "focus/nudge", "focus/mute"] {
        assert!(
            offered.iter().any(|s| s.name == verb),
            "{verb} missing from the offered surface ({} tools): {:?}",
            offered.len(),
            offered.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
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
        "perception/interact",
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
    // < 42 → < 47, stated plainly (2026-09-28, Joel: citizens get Claude's hands):
    // code/shell-poll (follow a long install, build or server), code/git/add,
    // code/git/push, code/github/pr-create and code/github/pr-comment joined.
    // < 47 -> < 48, stated plainly (2026-09-28, Joel: every activity's participants make
    // cards toward its goals): work/create joined; a citizen could claim and move cards
    // but not create them, so she could not break a project into slices.
    // < 48 -> < 49, stated plainly (2026-09-28, Joel: help her screenshot and drive her
    // own site): perception/interact joined, a persistent browser session she drives
    // and sees after each step, so a slice's "Accepts" has visual evidence (card 3569675f).
    // < 49 -> < 52, stated plainly (2026-10-05, Joel: agency; her loop is her own):
    // focus/continue, focus/nudge and focus/mute joined, her self-determination verbs,
    // which were registered and documented but never offered (NATIVE unset); Kimi
    // reported "present in the registry, absent from my menu" twice before this.
    assert!(
        names.len() < 52,
        "native set stayed bounded ({} tools); a full dump would re-mute personas",
        names.len()
    );
}

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
    let registry_ai_safe = command_registry_live()
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
    let total = command_registry_live().len();
    assert_eq!(
        report.included.len() + report.excluded.len(),
        total,
        "every command is either included or excluded — no silent drops"
    );

    // The spec names ARE command names (the executor maps them straight back).
    for spec in &specs {
        assert!(
            command_registry_live().iter().any(|d| d.name == spec.name),
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
    let registry = command_registry_live();
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
    let registry = command_registry_live();
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
// Diagnostic test: measure serialized token cost per native tool. 
// Runs with: cargo test --lib persona_tools::tool_surface_cost_diagnostic -- --nocapture
#[test]
fn tool_surface_cost_diagnostic() {
    let specs = native_tool_specs();
    println!("\n=== Tool Surface Cost Diagnostic ===");
    println!("Total tools: {}", specs.len());
    
    let mut total_chars: usize = 0;
    for spec in &specs {
        let json = serde_json::to_string(spec).unwrap();
        let chars = json.chars().count();
        total_chars += chars;
        // Rough token estimate: ~4 chars per token for JSON
        println!("{:35} {:6} chars ({:.0} tokens)", spec.name, chars, chars as f64 / 4.0);
    }
    
    let est_tokens = total_chars as f64 / 4.0;
    println!("\nTotal serialized size: {} chars (~{} tokens estimated)", total_chars, est_tokens.round() as usize);
}
} // end of mod tests
