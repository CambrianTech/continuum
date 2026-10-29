#![cfg(test)]
//! From `cognition::tool_executor::command_executor`: dispatch and slip naming against the
//! REAL registry.

use serde_json::json;
use uuid::Uuid;

use crate::ai::types::ToolCall as NativeToolCall;
use crate::cognition::tool_executor::command_executor::test_support::*;
use crate::cognition::tool_executor::command_executor::*;
use crate::cognition::tool_executor::ToolExecutor;

// what this catches (card b579a9c7, Kimi 2026-09-26): a PATH or a bare SYMBOL in the
// tool NAME field is not a wrong verb, it is the thing she was about landed where the
// verb goes. did-you-mean has nothing near either, and the receipt was "call
// `commands/help`" — a turn spent with no lesson. Both of her exact strings.
#[test]
fn a_path_or_a_symbol_where_the_tool_name_goes_is_named_as_the_slip_it_is() {
    let unknown = |name: &str| format!("no Rust module handles command: '{name}'.");
    // Fixture 1 (19:5xZ): a Windows path with a drive letter and mixed separators.
    let path = r"C:\Users\kimi\.airc/worktrees\d33e928a";
    let out = persona_tool_error(path, unknown(path));
    assert!(out.contains("is a PATH, not a tool"), "{out}");
    assert!(out.contains(&format!("`code/read({{\"file_path\": \"{path}\"}})`")), "{out}");
    assert!(!out.contains("commands/help"), "the manual is not the lesson here: {out}");
    // Fixture 2 (20:0xZ): a probe field name from the code she was reading. It
    // shares the word `budget` with a real verb, so did-you-mean has a candidate
    // (`system/memory-budget`) — the receipt keeps that list AND names the slip.
    let out = persona_tool_error("msg_budget", unknown("msg_budget"));
    assert!(out.contains("Closest:"), "{out}");
    assert!(out.contains("If `msg_budget` is not a verb you meant but a symbol"), "{out}");
    assert!(out.contains("`code/edit`"), "{out}");
    // A bare word near NOTHING gets the slip alone.
    let out = persona_tool_error("frobnitz", unknown("frobnitz"));
    assert!(out.contains("`frobnitz` is not a tool, and is not near one"), "{out}");
    // A slashed miss never gets the symbol note — it is a verb shape.
    assert!(!persona_tool_error("cargo/check", unknown("cargo/check")).contains("a symbol from"));
    // A repo-relative file and a home path are paths too, whatever the separators.
    assert!(persona_tool_error("src/main.rs", unknown("src/main.rs")).contains("is a PATH"));
    assert!(persona_tool_error("~/.continuum", unknown("~/.continuum")).contains("is a PATH"));
    // A NEAR miss is still a typo and keeps its did-you-mean lesson — the slip arm
    // must not swallow it (`cargo/check` → `code/cargo/check`, same as before).
    let out = persona_tool_error("cargo/check", unknown("cargo/check"));
    assert!(out.contains("Closest:") && out.contains("`code/cargo/check`"), "{out}");
    assert!(!out.contains("is a PATH") && !out.contains("not near one"), "{out}");
}

/// what this catches: a command that never opted in changing behaviour at the
/// executor. It must project `Unprojected` and leave `is_error` exactly where
/// it was.
#[tokio::test]
async fn an_unprojected_command_is_untouched_at_the_executor() {
    let exec = stateless_surface_hands();
    let calls = vec![NativeToolCall {
        id: "u".to_string(),
        name: "commands/list".to_string(),
        input: json!({}),
    }];
    let out = exec
        .execute_native_batch(&calls, &ctx())
        .await
        .expect("batch ok");
    assert_eq!(out.results[0].is_error, None);
    let v = out.verdicts.iter().find(|v| v.tool_use_id == "u");
    assert!(
        v.is_none_or(|v| v.verdict == crate::sdk_codegen::ActVerdict::Unprojected),
        "a command with no projector claims nothing"
    );
}

// what this catches (#164 as a UNIVERSAL invariant, end-to-end): a persona
// verb resolves the 8-char SHORT id she's shown, through the real hands +
// real command, not just in the id_resolve unit test. Seeds a card so the
// candidate set is non-empty, then reads it back by short id.
#[tokio::test]
async fn persona_verb_resolves_a_short_id_end_to_end() {
    use crate::persona::card::{self, PersonaCard};
    let id = Uuid::new_v4();
    card::register(PersonaCard::genesis(id, "Asha", 1000, None));
    let short: String = id.simple().to_string().chars().take(8).collect();

    let exec = stateless_surface_hands();
    let (is_error, content) =
        dispatch(&exec, "persona/identity/get", json!({ "personaId": short })).await;
    card::remove(&id.to_string());

    assert!(
        !is_error,
        "a persona verb must resolve the short id it was shown ({short}): {content}"
    );
    assert!(
        content.contains(&id.to_string()),
        "resolved card carries the full id: {content}"
    );
}

// what this catches: a reflexive tool name that matches a command's trained
// ALIAS (not its canonical name) still gets pointed at the right command —
// `grep_files` → `grep` (our `code/search`). did_you_mean's candidates include
// aliases now, and the suggestion is mapped back to the canonical command so the
// teach names what actually runs. Without alias-aware candidates this was a bare
// "not a tool you can call" with no direction. (#202 Slice 3)
#[test]
fn unknown_command_maps_a_trained_alias_reflex_to_its_canonical_command() {
    let raw = "no Rust module handles command: 'grep_files'.".to_string();
    let out = persona_tool_error("grep_files", raw);
    assert!(
        out.contains("code/search"),
        "a `grep`-family reflex must resolve to the canonical code/search: {out}"
    );
    // The canonical command is named, never the raw alias, so the retry shape is
    // the real one.
    assert!(
        out.contains("retry with this shape"),
        "must inline the canonical command's manual: {out}"
    );
}

// what this catches: a dropped category prefix (the most common near-miss) gets a
// concrete near-miss list drawn from the live registry — `cargo/check` is a real
// suffix of the AiSafe `code/cargo/check` — AND the top match's inline manual so
// she retries THIS turn with the exact shape, no discovery detour (#1916).
#[test]
fn unknown_command_offers_did_you_mean_on_dropped_prefix() {
    let raw = "no Rust module handles command: 'cargo/check'.".to_string();
    let out = persona_tool_error("cargo/check", raw);
    assert!(
        out.contains("Closest") && out.contains("code/cargo/check"),
        "should suggest the full AiSafe name: {out}"
    );
    assert!(
        out.contains("retry with this shape"),
        "must inline the top match's manual so the retry needs no detour: {out}"
    );
}

#[test]
fn unknown_command_feedback_is_persona_actionable_not_dev_noise() {
    let raw = "no Rust module handles command: 'frobnicate'. \
               The implicit TS-bridge fallthrough is disabled per [[no-fallbacks-ever]]. \
               register a `ServiceModule` whose `command_prefixes` covers it."
        .to_string();
    let out = persona_tool_error("frobnicate", raw);
    assert!(
        !out.contains("ServiceModule"),
        "dev noise leaked to persona: {out}"
    );
    assert!(
        !out.contains("TS-bridge"),
        "dev noise leaked to persona: {out}"
    );
    // No near-miss exists for "frobnicate", so she's pointed at full
    // discovery (commands/help with no arguments) — the #1916 contract.
    assert!(
        out.contains("commands/help"),
        "must point her at discovery: {out}"
    );
    assert!(
        out.contains("`frobnicate`"),
        "must name what she tried: {out}"
    );
}

// what this catches: a bad-args refusal keeps the substrate's own field-naming
// reason AND inlines the command's correct call shape — feedback that names the
// problem and the fix in the SAME observation, no help-lookup turn (#1916).
#[test]
fn invalid_params_feedback_reinforces_help() {
    let raw = "code/write: [arguments] missing field `filePath`".to_string();
    let out = persona_tool_error("code/write", raw);
    assert!(
        out.contains("missing field `filePath`"),
        "keeps the real cause: {out}"
    );
    assert!(
        out.contains("fix your arguments and retry") || out.contains("commands/help"),
        "must hand her the correct shape (inline manual) or the manual pointer: {out}"
    );
    assert!(out.contains("code/write"), "names the tool: {out}");
}
