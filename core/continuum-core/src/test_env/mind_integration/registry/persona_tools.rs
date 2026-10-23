#![cfg(test)]
//! From `cognition::persona_tools`: the native surface derived from the REAL registry.

use crate::cognition::persona_tools::*;

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
