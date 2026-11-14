#![cfg(test)]
use super::*;

fn faculty_on(window: u32) -> LlmDeliberationFaculty {
    LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Aiko",
        "You are Aiko.",
        Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
    )
    .with_context_window(window)
    .with_tools(persona_tools::native_tool_specs())
}

// what this catches: the tool schemas moving between consecutive acts in one activity,
// which re-prefills everything after her system prompt. Aiko on the M5, 2026-10-24:
// `delib.prompt.reuse_split` diverged in `tools` (`edit_file` one act, `conclude_activity`
// the next) because a message turn took the whole registry (51 schemas), a work or
// initiative turn her hands (23), a review card two more verbs, and the tool share
// followed the turn's audience and measured prefill rate. Every kind of turn, under either
// audience, now sends the same bytes, and those bytes carry the reviewer verbs and her
// coding arc. Goal 10 (card ecd5dff3).
#[tokio::test]
async fn consecutive_acts_in_one_activity_send_byte_identical_tool_schemas() {
    use crate::cognition::audience;
    use crate::inference::prefill_rate::Audience;
    let window = 87_296;
    let faculty = faculty_on(window);
    let message = Workspace::new("anything open?");
    let mut work = Workspace::new("fix the failing test");
    work.workspace_deliverable = true;
    let mut initiative = Workspace::new("your own time");
    initiative.initiative = true;
    let mut held_review = Workspace::new("review card deec4ac2: is the fix right?");
    held_review.workspace_deliverable = true;
    let wire = |ws: &Workspace| {
        let selected = faculty.prompt_view(ws).tools;
        (selected.reason, serde_json::to_string(&selected.specs).expect("specs serialize"))
    };
    let first = wire(&message);
    assert_eq!(first.0, SurfaceReason::Hands);
    for (kind, ws) in [("work", &work), ("initiative", &initiative), ("held review", &held_review), ("message", &message)] {
        for who in [Audience::Interactive, Audience::Unattended] {
            let act = audience::with(who, async { wire(ws) }).await;
            assert_eq!(act, first, "a {who:?} {kind} act changed the tool schemas");
        }
    }
    for must in ["code_edit", "code_shell", "commands_list", "commands_help"] {
        assert!(first.1.contains(&format!("\"{must}\"")), "{must} rides natively: {}", first.1);
    }
    // An on-demand verb (concluding an activity) is one commands/help away, not a schema every act.
    // The reviewer's verbs are in hand when her held card puts them there (a review card she
    // holds: `work_preconditions::in_hand`, the fix for Benchy's 2026-11-04 review card), never
    // as core hands: offered on every card, residents called work/review on their own card 22
    // times and work/submission on cards with nothing submitted 27 times (the M5, 2026-11-07).
    for absent in ["activity_archive", "work_review", "work_submission"] {
        assert!(!first.1.contains(&format!("\"{absent}\"")), "{absent} stays one commands/help away: {}", first.1);
    }
}

// what this catches: S1's whole point. A room whose recipe declares affordances
// gets EXACTLY those native verbs plus every verb that declares `Always` (the
// discovery pair) — the recipe is the rule of the room's prompt, never of her
// mind: a verb that declares `OnDemand` (`focus/continue`) stays among her hands,
// one `commands/help` away. Before this, `affordances` rode into the
// manifest and every room offered the same prefix-filtered global list.
#[test]
fn a_room_that_declares_affordances_selects_exactly_those_plus_the_discovery_pair() {
    let faculty = faculty_on(65_536);
    let mut ws = Workspace::new("find the docs for this API");
    ws.room_affordances = vec!["web/fetch".to_string(), "web/search".to_string()];

    let selected = faculty.prompt_view(&ws).tools;
    assert_eq!(selected.reason, SurfaceReason::RoomAffordances);
    let offered = selected.specs.as_deref().expect("a surface was offered");
    let surface = faculty.tool_surface();
    let names: Vec<&str> = surface
        .names()
        .iter()
        .zip(surface.registry().iter())
        .filter(|(_, spec)| offered.iter().any(|o| o.name == spec.name))
        .map(|(raw, _)| raw.as_str())
        .collect();
    assert!(!names.contains(&"focus/continue"), "an on-demand verb rides no prompt unlearned: {names:?}");
    assert!(
        persona_tools::native_offers().iter().any(|o| o.spec.name == "focus/continue" && o.disclosure.in_hands()),
        "and stays among her hands, one commands/help away"
    );
    for must in ["web/fetch", "web/search", "commands/list", "commands/help"] {
        assert!(
            names.contains(&must),
            "room-selected surface must carry {must}: {names:?}"
        );
    }
    assert!(
        !names.iter().any(|n| n.starts_with("code/")),
        "a room that did not grant code/* must not offer it: {names:?}"
    );
    assert_eq!(
        selected.tokens,
        crate::cognition::tool_surface::tool_surface_tokens_of(offered),
        "the price is the price of what was sent"
    );
}

// what this catches: the regression S1 must not cause. A room whose recipe declares NO
// affordances (every chat, profile, theme and settings page today) offers her core hands, the
// same on a message turn as on a work turn, and never her whole hands (the rest are one
// commands/help away; Joel, 2026-11-04: "Core hands in prompt, rest on demand").
#[test]
fn a_room_that_declares_nothing_offers_her_hands_on_every_turn() {
    let faculty = faculty_on(65_536);
    let mut work = Workspace::new("fix the failing test");
    work.workspace_deliverable = true;
    for ws in [Workspace::new("anything open?"), work] {
        assert!(ws.room_affordances.is_empty(), "the ordinary room declares none");
        let selected = faculty.prompt_view(&ws).tools;
        assert_eq!(selected.reason, SurfaceReason::Hands);
        let core = core_hands(&persona_tools::native_offers());
        let sent: Vec<&str> = selected.specs.as_deref().into_iter().flatten().map(|s| s.name.as_str()).collect();
        assert_eq!(sent.len(), core.len(), "her core hands, no more: {sent:?}");
        assert!(sent.len() < faculty.tool_surface().hands().len(), "her on-demand verbs stay out: {sent:?}");
    }
}

// what this catches: a command's declared disclosure drifting from what her work needs: the
// verbs of choosing, doing and landing her own work (find it, take it, write it, commit, push,
// open the PR; Joel, 2026-11-04: "git is perfect for all project work") must be core hands; the
// discovery pair and her verdict verbs ride every surface (`Always`), or the rest of her hands are
// unreachable and she cannot pass without prose; her core hands are an exact, declared set; `code/shell` and
// `code/shell-poll` travel together (Idris, 2026-09-29: shell's own description says to poll with
// it); a peer's diff (`code/git/apply`) is never among her hands; and some verbs are on demand,
// or "rest on demand" would be a promise nothing declares.
#[test]
fn the_real_registry_declares_her_own_work_as_core_hands() {
    use continuum_base::tool_disclosure::Disclosure;
    let offers = persona_tools::native_offers();
    if offers.is_empty() {
        return; // no registry in this test build: nothing to assert against
    }
    let declared = |name: &str| offers.iter().find(|o| o.spec.name == name).map(|o| o.disclosure);
    for verb in ["commands/list", "commands/help", persona_tools::VERDICT_YIELD_TURN] {
        assert_eq!(declared(verb), Some(Disclosure::Always), "{verb} rides every surface");
    }
    // The EXACT set, not a lower bound: a native command declares nothing and is `Core`, so a new
    // one would grow her core hands, and with them the smallest window that admits her, with no
    // test failing (Cormac on #5763). Declaring a core hand is this one-line diff.
    let mut core: Vec<&str> = offers
        .iter()
        .filter(|o| o.disclosure == Disclosure::Core)
        .map(|o| o.spec.name.as_str())
        .collect();
    core.sort_unstable();
    assert_eq!(
        core,
        [
            "code/edit", "code/git/add", "code/git/commit", "code/git/diff", "code/git/push",
            "code/git/status", "code/github/pr-comment", "code/github/pr-create", "code/list",
            "code/read", "code/run", "code/search", "code/shell", "code/tree",
            "code/write", "work/claim", "work/create", "work/get", "work/heartbeat", "work/list",
            "work/note", "work/release", "work/state", "work/submit",
        ],
        "her core hands are exactly these; a new native command declares where it sits"
    );
    assert_eq!(declared("code/git/apply"), Some(Disclosure::Withheld));
    // A command's end is pushed to her [tasks], so a read of a running one is withheld (#5939).
    assert_eq!(declared("code/shell-poll"), Some(Disclosure::Withheld));
    let hands = hands_surface(&offers);
    assert!(hands.len() > core_hands(&offers).len(), "some of her hands are on demand");
}

// what this catches: a rule that binds nothing must never bind to NOTHING. A
// recipe naming only verbs this node has no native tool for (`mail/send` before
// mail exists) falls back to her hands and says so in the ledger —
// never an empty offer (the #206 cliff: 14/14 acts on commands/help, 0 edits).
#[test]
fn a_room_whose_affordances_match_no_native_verb_falls_back_never_to_nothing() {
    let faculty = faculty_on(65_536);
    let mut ws = Workspace::new("chase the recruiter");
    ws.room_affordances = vec!["mail/send".to_string()];
    let selected = faculty.prompt_view(&ws).tools;
    assert_eq!(selected.reason, SurfaceReason::Hands, "fell through to her core hands");
    let core = faculty_on(65_536).prompt_view(&Workspace::new("anything open?")).tools;
    assert_eq!(selected.specs, core.specs, "the same core hands an ordinary room sends");
    assert!(selected.specs.as_deref().is_some_and(|specs| !specs.is_empty()), "never nothing");
}

// what this catches: the shipped benchmark recipe is now the RULE for every
// benchmark room, so it must authorize at least her core hands — every verb an
// ordinary work turn offers — or S1 would take tools away from the citizens
// solving cards right now. If a new core hand is declared without being
// authored here, this names it. A verb that declares `Always` (the discovery
// pair, her verdict verbs) rides every surface, a recipe's included, so no
// recipe has to author it.
#[test]
fn the_shipped_benchmark_recipe_authorizes_every_hand_a_work_turn_offers() {
    let raw = persona_tools::native_offers();
    for (file, json) in [
        (
            "benchmark.json",
            include_str!("../../../../../../continuum-experience/src/experience/recipes/benchmark.json"),
        ),
        (
            "benchmark-round.json",
            include_str!("../../../../../../continuum-experience/src/experience/recipes/benchmark-round.json"),
        ),
    ] {
        let recipe = crate::experience::recipe::ExperienceRecipe::from_json(json)
            .expect("shipped benchmark recipe parses");
        let authorized: std::collections::HashSet<&str> = recipe
            .affordances
            .iter()
            .map(|a| a.command.as_str())
            .collect();
        let missing: Vec<String> = raw
            .iter()
            .filter(|offer| offer.disclosure.in_core() && !offer.disclosure.in_every_surface())
            .map(|offer| offer.spec.name.clone())
            .filter(|n| !authorized.contains(n.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "{file} must authorize every hand; missing: {missing:?}"
        );
    }
}
