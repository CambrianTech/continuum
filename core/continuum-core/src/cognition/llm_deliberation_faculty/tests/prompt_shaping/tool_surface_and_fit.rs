#![cfg(test)]

use super::*;

// what this catches: progressive disclosure — the per-turn tool PAYLOAD is the
// two-tool DISCOVERY PAIR (`commands/list` + `commands/help`), not the whole
// authorized registry, and the system prompt carries only a CATEGORY INDEX, not
// every tool. The old dump injected ~150 full schemas / one-liners (~4–5k tokens)
// into EVERY turn, overflowing n_ctx → 400 "exceeds context size" → mute. Now the
// surface is a tiny category index inside the system prompt + the two-tool native
// offering, so even a huge tool set leaves system + user + the offered tools well
// within the served window. Invariant: the category index (not tool names) rides
// the system prompt, the native offering is exactly the discovery pair, and the
// whole prompt + its tools + reserve fit the window. A regression means the dump
// came back.
#[test]
fn tool_surface_is_a_category_index_plus_discovery_pair() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    // A tool set whose FULL schemas would dwarf the window — the live shape
    // (whole authorized set ≫ whole slot), scaled down. With progressive
    // disclosure it no longer matters: only names+summaries ride the prompt.
    // 8192 = tight-but-SERVABLE: the un-amputated native surface (~4.1k
    // tokens of specs) must fit beside a real prompt + reserve. 4096 was
    // the old cliff-era fixture — below what the full surface can ever
    // serve in, i.e. a window the plan would never hand a tool-using
    // persona. The budget trims VOLATILE context to fit; specs are
    // reserved up front, never amputated.
    let window: u32 = 8192;
    let tools: Vec<NativeToolSpec> = (0..60)
        .map(|i| NativeToolSpec {
            name: format!("cat/command_{i}"),
            description:
                "A registry command projected as a tool with a structured argument schema; \
             its full parameter description rides the chat-template injection."
                    .to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: json!({
                    "path": { "type": "string", "description": "workspace-relative path" },
                    "mode": { "type": "string", "enum": ["read", "write", "append"] },
                    "limit": { "type": "integer", "description": "max rows returned" }
                }),
                required: Some(vec!["path".to_string()]),
                definitions: None,
            },
        })
        .collect();
    // Cloned before the original moves into `faculty` — the roomy control below needs
    // the IDENTICAL surface, or the comparison measures two different things.
    let roomy_tools = tools.clone();

    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Asha",
        "You are Asha, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(window)
    .with_tools(tools);

    // The native surface is NEVER window-amputated. The old tight-window
    // "discovery pair only" cliff was a clamp (glass-boxed #206): a
    // native-tool-call model can only emit calls for offered specs, so the
    // amputated surface stranded it in a commands/help loop with 0 edits —
    // and the threshold sat on the served window's knife-edge, flipping
    // 10/10 ↔ 0/6 on a token. The budget (`prompt_view_within`) owns the
    // fit now: it reserves these specs' tokens up front and trims VOLATILE
    // context, protecting the hands. This test USED to pin the cliff; now
    // it pins its absence.
    let native: Vec<&str> = faculty
        .native_specs
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    // Names on the wire ride the DIALECT (tool_dialect): the conventional,
    // charset-legal aliases tool-trained models actually saw in training.
    for must in ["list_commands", "help", "edit_file", "bash", "grep"] {
        assert!(
            native.contains(&must),
            "tight window must NOT amputate the native surface — {must} missing: {native:?}"
        );
    }
    // On a ROOMY window the CORE CODING ARC rides natively too — so a
    // tool-call-trained model acts directly instead of looping on
    // `commands/help{code/search}` (glass-boxed: 14/14 SWE acts were help-loops,
    // 0 edits, before this). Bounded working set, never the ~150-tool dump.
    let roomy = LlmDeliberationFaculty::new(
        persona,
        "Asha",
        "You are Asha, a thoughtful engineer on the grid.",
        Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_context_window(32_768)
    .with_tools(vec![]);
    let _ = roomy; // (surface empty without tools — the arc requires authorization)
    let roomy_native: Vec<String> = {
        let f = LlmDeliberationFaculty::new(
            persona,
            "Asha",
            "You are Asha.",
            Arc::new(HeuristicInferenceAdapter::new()),
        )
        .with_context_window(32_768)
        .with_tools(persona_tools::native_tool_specs());
        f.native_specs.iter().map(|s| s.name.clone()).collect()
    };
    // Wire names are the DIALECT aliases (tool_dialect) — the coding arc
    // rides under the names the model was trained on, never our slashes.
    for must in ["grep", "read_file", "edit_file", "bash"] {
        assert!(
            roomy_native.iter().any(|n| n == must),
            "roomy window ⇒ {must} offered natively: {roomy_native:?}"
        );
    }

    // The bookmarked MENU rode into the system prompt. An EXPANDED category
    // lists the BARE verb of every tool it holds (`category: verb, …`) — names
    // only since the 2026-07-10 prompt diet; args live in commands/help + the
    // #1916 inline error-manual, never an 8k menu wall.
    let expanded = BTreeSet::from(["cat".to_string()]);
    let framing = faculty.compose_system("", &expanded, false, false, None);
    assert!(
        framing.contains("[Your tools]") && framing.contains("cat: command_0"),
        "an expanded category must name each verb under its header: {framing}"
    );
    assert!(
    !framing.contains("cat/command_0"),
    "the full slash-path form must NOT be dumped — verbs render bare under the category header"
);
    // Collapsed (nothing expanded) the same category renders as a one-line
    // bookmark — the spine still NAMES it, so she can open it on demand.
    let collapsed = faculty.compose_system("", &BTreeSet::new(), false, false, None);
    assert!(
    collapsed.contains("cat (60 — commands/list --filter cat)"),
    "a collapsed category is a one-line bookmark naming it + its verb count: {collapsed}"
);

    // Under real burst pressure the whole prompt + the one tool + reserve fit
    // the served window — the exact condition llama-server checks before 400.
    let mut ws = history_with_stimulus("LATEST: did the deploy fix land?");
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        &"deploy pipeline observation; ".repeat(2000),
        0.9,
        "recalled",
    ));
    let view = faculty.prompt_view(&ws);
    let reserve = view.completion_reserve as usize;
    let prompt =
        est_tokens(&view.system) + LlmDeliberationFaculty::messages_cost(&view.messages);

    // This assertion used to demand that framing + the FULL tool surface + the
    // reply reserve all fit an 8192 window. That is arithmetically impossible and
    // always was: the surface alone is ~4.6k, the framing floor ~1.6k, and the
    // reserve is window/4 — 8300 into 8192. It only ever "passed" while the
    // deleted #206 cliff amputated the tools, which is the clamp that stranded a
    // native-call model in a help loop. Asserting an impossibility does not make
    // it true; it just hides WHICH constraint is binding.
    //
    // The honest invariants are two, and they are what this now pins:
    //   1. the budget did its job — the prompt itself fits the window minus reply
    //   2. the substrate KNOWS when the window cannot host the hands, and says so
    //      with a real number instead of shipping a citizen who silently goes mute
    assert!(
        prompt + reserve <= window as usize,
        "the trimmed prompt ({prompt}) + reply reserve ({reserve}) must fit {window} \
         — the budget owns this, and it trims volatile context, never the hands"
    );

    // The bound is a real derivation — at the window it names, the arithmetic for
    // bare framing + tools + reply actually closes.
    let needed = faculty.min_window_for_agentic_surface();
    assert!(
        faculty.describe_tool_tokens()
            + faculty.framing_floor_tokens() as usize
            + LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS as usize // the reply's flex floor — the smallest grant production gives a reply
            <= needed as usize,
        "min_window_for_agentic_surface({needed}) must clear its own arithmetic"
    );
    // …and here is the part that matters, measured rather than assumed. This
    // assertion USED to read `needed <= window` — "8192 clears the bound (8040)
    // and she is STILL starved" — and it went red on its own terms: the message
    // said "if it no longer does, the surface or framing grew and the story
    // changed", and it did. The bound moved 8040 → 9348 (+1308) as framing
    // accreted turn-fact by turn-fact (#151, #152, #144, #303 …), each one
    // individually justified.
    //
    // So the story is now STRONGER, not broken: at 8192 an agentic citizen no
    // longer fits AT ALL — the necessary condition itself fails, before we even
    // get to "necessary but not sufficient". An 8k-window model cannot host this
    // surface, full stop. That is #333 (the surface is paid twice) stated as a
    // number instead of a complaint.
    //
    // Pinned as a CEILING rather than relaxed to pass: this is the ratchet that
    // makes the next +1308 fail loudly instead of accruing silently. Lower it
    // when the surface actually shrinks; never raise it to make a red go green
    // without saying what grew and why.
    //
    // 9400 → 9600, and what grew, stated plainly as the ratchet demands: the
    // `yield_turn` VERDICT VERB (+144 tokens, 9348 → 9544). It is the structured
    // channel for choosing silence, which `Decision::Pass` never had — the
    // absence is what forced months of prose phrase-matching, and Joel killed the
    // last of that on 2026-08-07 ("string matches for semantic understanding is
    // not good for reliability"). Its schema is argument-free; the 144 tokens are
    // almost entirely the DESCRIPTION, which is the part that teaches her the
    // silence is free and the announcement is the noise.
    //
    // Judged worth it against #333 rather than waved through: a single avoided
    // pass-cascade costs the room more than 144 tokens (the 2026-08-01 one ran
    // ~30 minutes across every peer), and this replaces matcher code that could
    // only ever be beaten by the next phrasing.
    //
    // 9600 → 9800, and what grew, stated plainly as the ratchet demands: PR #2162
    // grew the native payload 20 → 23 schemas (+212 tokens, 9544 → 9756) and said
    // so in its own commit message — the #339 lifecycle write-back verbs
    // (REACHABLE-VERIFIED: they existed and had never declared NATIVE, so a
    // native-call citizen could claim work she could never update or close) plus
    // `room/members` (pinned into the native-surface test after #358 proved a
    // correct roster read). Each verb closes a measured live defect; shrinking
    // here would undo verified fixes. What #2162 did NOT do was re-pin this
    // ceiling, so canary sat red for a day and every branch inherited it —
    // which is itself the ratchet working: the growth got named here instead
    // of accruing silently.
    // 9800 → 10350, stated plainly as the ratchet demands: three NATIVE room
    // membership verbs — `room/list`, `room/join`, `room/leave` (+492 tokens,
    // 9756 → 10292).
    //
    // Why they earn it: nothing in continuum could put a citizen in a second
    // room AT ALL. Her rooms were whatever bootstrap seeded, forever — the last
    // of four narrowings behind operator messages being structurally invisible
    // to personas (measured on two machines: every citizen subscribed to exactly
    // one room while the operator held several, so the daemon's own channel
    // filter correctly dropped everything). Perception and reply were fixed
    // first; without membership those fixes have nothing to act on. And they are
    // HERS to call, not only an operator's to apply to her, which is the whole
    // difference between a citizen and a managed resource.
    //
    // SHRUNK FIRST, then re-pinned — the ratchet's first branch before its
    // second. The initial draft cost 688 tokens; trimming verbose DESCRIPTIONs
    // and dropping alias overflow (4→2, 3→2) recovered 196 of them. What is left
    // is the irreducible cost of three discoverable verbs. Descriptions were cut
    // to the point where more cutting would recreate #358 — a citizen who cannot
    // find the verb reaches for the wrong one and reads her own looping as having
    // nothing to contribute.
    // 10350 → 11300, stated plainly as the ratchet demands, and SHRUNK FIRST
    // as its own first branch requires — this is the first re-pin where the
    // shrink was MEASURED rather than eyeballed.
    //
    // WHAT GREW: three NATIVE activity verbs — `activity/spawn`, `activity/archive`,
    // `activity/protect` (+1,273 gross). They are not new commands; they routed
    // all along and had simply never shipped a DESCRIPTOR, so `activity/spawn` —
    // the verb that mints every room, benchmark rooms included — was absent from
    // `commands/list`, the ACL, codegen and every citizen's tool surface. Same
    // defect class as #339, now impossible: `ModuleRegistry::register` panics on
    // a constructor with no descriptor.
    //
    // WHAT SHRANK: 728 tokens, from making the schema projection stop shipping
    // MAINTAINER RATIONALE to citizens. schemars lifts `///` verbatim, so one
    // doc comment served two readers with opposite needs — a citizen deciding
    // whether to spawn a room was billed for `schemars(with = "String") describes
    // the WIRE (a uuid string, per #[serde(transparent)])…`. The projection now
    // keeps the LEAD PARAGRAPH (see `persona_tools::lead_paragraph_descriptions`);
    // rationale stays in the file where it belongs. Not a length cap — truncating
    // the ANSWER is #358's shape.
    //
    // NET 11,972 → 11,244. Design + full measurement:
    // docs/cognition/TOOL-DISCLOSURE-LADDER.md. That doc also retracts the
    // "the schemas are 3-5x bloated" reading this ceiling invited: measured, the
    // 26-verb surface is 6,935 tokens, mean 266/verb — leaner per verb than the
    // frontier agent runtimes it was being unfavourably compared to. The surface
    // is not obese; it is paid TWICE (#333) and it is charged against a 16k lane.
    // The next real shrink is structural — rung 2 of the ladder becoming the
    // TURN's working set instead of a static list — after which this ceiling
    // guards the SPINE and stops taxing capability.
    // The guard is about SURFACE GROWTH, so it measures the SURFACE. It used to assert
    // on `min_window_for_agentic_surface()`, which was then `fixed * D/(D-1)` — the
    // surface AND the reserve policy multiplied together (it is now `fixed + the
    // reply floor`, still a reply term this guard does not own). Changing the reserve split therefore
    // moved a ceiling whose own message says "framing/tools grew again. Shrink the
    // surface", and the D=4→2 experiment tripped it at 16,864 with the surface
    // completely unchanged. Two concerns in one assertion; the reserve half was never
    // this guard's business. `fixed` is tools + bare framing and nothing else, so this
    // now moves if and only if the surface actually grows.
    // 8500 → 8650, stated plainly as the ratchet demands, and SHRUNK FIRST:
    // one NATIVE verb — `perception/hot-edit` (+191 tokens, 8432 → 8623). The
    // TWEAK half of the design loop (render → observe → hot-edit → re-grade,
    // DESIGN-BENCH-VISUAL-CRAFT.md): apply a CSS patch to a live page and
    // re-observe, no deployment — offered beside `perception/observe` because a
    // citizen who can see a page but not iterate on it styles blind. The first
    // draft cost 238 tokens; trimming the DESCRIPTION to its contract (full
    // accumulated CSS, replace-wholesale layer, delta meaning) recovered 47.
    // What is left is the irreducible cost of one discoverable verb.
    //
    // 8650 → 9400, stated plainly (2026-08-25): two NATIVE web verbs —
    // `web/search` + `web/fetch` (+727 tokens even after trimming both
    // DESCRIPTIONs to one line each; the rest is their irreducible param
    // schemas). Web FORAGING is a deliberate capability-parity add (Joel: "make
    // sure we have it" — the operator uses web lookup constantly on SWE/task
    // work, and a native-call model like Ornith can ONLY emit calls for tools in
    // its offered specs, so catalog-only web was unreachable to her hands). The
    // surface already exceeded the 8192 tight-test window before this (8623 >
    // 8192) — the budget trims VOLATILE context (recall/RAG) to fit and reserves
    // specs up front, so this raises the reserved floor, it does not introduce a
    // new overflow. Ornith serves at 166k where this ceiling is irrelevant; the
    // tight-window LCD persona (which does not do web research) pays with less
    // recall room, a conscious trade. If a third addition wants in, SHRINK first.
    //
    // 9400 → 10150, stated plainly (2026-08-25): three NATIVE GitHub-collaboration
    // verbs — `code/github/pr-create` + `pr-comment` + `issue-create` (+748 tokens,
    // their irreducible param schemas). This is the executor→TEAMMATE layer (Joel:
    // "friendly in how code and GitHub work are managed"): she had LOCAL git but
    // could not open/comment a PR or file an issue, and a native-call model can only
    // emit calls for OFFERED tools, so catalog-only collaboration verbs were
    // unusable by her hands. Same trade as the web add: Ornith serves at 166k where
    // this is irrelevant; the tight-window LCD persona pays with less recall room.
    // The shrink-first debt is now REAL at 10k — the next capability MUST shrink the
    // surface (a category-index/discovery split for the rarely-used verbs, #333), or
    // the LCD persona gets a reduced surface; do not keep bumping this.
    //
    // 10150 → 10400, stated plainly (2026-08-26): ONE native vision verb —
    // `vision/look` (+191 tokens, its param schema). This is a citizen's EYES
    // as an act: workspace images (screenshots, charts, benchmark PNGs) were
    // structurally invisible — cognition/vision-describe is Internal, and a
    // native-call model can only emit calls for OFFERED tools. Sight is a
    // sensory-parity capability (every persona sees, per the sensory
    // architecture), and the vision-qa benchmark grades exactly this loop.
    // Same conscious trade as the web/github adds: the tight-window LCD
    // persona pays with less recall room; Ornith at 166k doesn't notice.
    // If another addition wants in, SHRINK first (#333).
    //
    // 10400 → 10700, stated plainly (2026-09-01): TWO native activity verbs —
    // `activity/recipes` + `activity/invite` (+~565 tokens, their param
    // schemas). A citizen with an idea could already SPAWN a room
    // (activity/spawn has been native since #274) but could neither
    // discover what recipes exist nor staff the room she made — spawn →
    // invite → say-what-it's-about is ONE flow (Joel, 2026-08-31:
    // starting an activity must be "easy and common", and citizens mint
    // their own activities per [[activities-are-self-hosting]]). Same
    // conscious trade as vision/look above.
    // 10700 -> 12000, stated plainly (#4102): three native reviewed-work
    // verbs, work/submit + work/review + work/submission. Their compact,
    // documented schemas cost 1286 guard tokens: exact artifact identity,
    // submission/review claims, and evidence must be callable by native
    // models. The full catalog plus framing is 11974 (10688 without
    // these three). This is a test receipt, not a runtime budget increase;
    // per-turn selection and whole-request window accounting stay intact.
    // 12000 -> 12100, stated plainly (2026-09-17, the reviewer's twin of #4148):
    // work/review gained `evidence_text` — the one field a reviewer can actually
    // fill (what she ran and saw) in place of an evidence hash nothing in her
    // world produced — and work/submit's derivable fields turned optional.
    // Net +27 guard tokens on the full catalog (12027).
    // 2026-09-17, #4158: work/list, work/similar and work/duplicates gained `room`
    // (boards are PER ROOM; a citizen seated in a triage room saw an EMPTY board).
    // First cut cost +117 guard tokens and tripped this ceiling AND the 8192
    // survival check below by TWO tokens; the docs were made terse instead of
    // moving either number. The surface is within the ceiling as it stood.
    // 12100 -> 13200, stated plainly (#4532, Joel 2026-09-28: "give them more not
    // less"): a capable citizen's work turn now carries the hands a Claude or Codex
    // session has — code/shell-poll, code/git/add, code/git/push,
    // code/github/pr-create, code/github/pr-comment — as natives. Measured 13145
    // on the full catalog (+1045 over the 12100 surface as merged). The citizen's
    // own terminal-level hands are the point of the change, not framing growth;
    // per-turn selection and whole-request window accounting stay intact.
    // 13200 -> 13500, stated plainly (2026-09-28, Joel: making cards toward an
    // activity's goals is every participant's work): work/create joined the native
    // surface (repo, title, body, priority, and a REQUIRED room). Measured 13499 on
    // CI before its docs were trimmed. Before it a citizen could claim and move cards
    // but not create one, so a project stayed one card.
    // 13500 -> 13610 (measured on CI over work/create), stated plainly (2026-09-28, Joel:
    // citizens need to drive and screenshot their own sites, as Playwright automation
    // does): perception/interact
    // joined the native surface (session, target, viewport, actions, selector), the
    // DRIVE half of the observe loop (card 3569675f).
    // 13610 -> 13624 (measured on canary ac65b92c7), stated plainly (2026-09-28, Joel:
    // a citizen's confusion is a tool-feedback defect): #4557 rewrote work/submit's
    // artifact help to lead with "Omit: computed from your checkout. Not a git sha."
    // after Kimi stalled supplying a hash the verb computes. The words are the fix.
    // 13624 -> 15119 (CI, PR #4603): provided perception commands now expose
    // typed required inputs and nested action schemas instead of Null. This is
    // actual tool demand, including target/session recovery descriptions; keep
    // accounting for it rather than hiding schemas from the persona or budget.
    // 15119 -> 15179 (CI, PR #4615): work/submit adds the optional
    // generation_request_id selector and its exclusivity help. Kimi's
    // inspected staging UUID expired before publication; this stable
    // causal selector adds 60 measured guard tokens. Account for the
    // real schema; retain whole-request budgeting and this growth guard.
    // 15179 -> 15731 (CI, PR #4626): screenshot now exposes its existing
    // selector, encoding, dimensions and delivery parameters instead of
    // an empty schema. The real typed contract adds 552 measured guard
    // tokens; this test ceiling is not a runtime context-budget change.
    // 15731 -> 17087 (PR #4758): focus/continue, focus/nudge and focus/mute join
    // the offer, her self-determination verbs (Joel 2026-10-05: agency; her loop
    // is her own). ~450 guard tokens each, in line with the ~330 average of the
    // other hands: a continuation carries a note, a room, an expectation and a
    // verdict, each described. Real demand, accounted for, not hidden.
    // 17087 -> 17150 (PR #4782): focus/continue gains `private`, the act that
    // takes her into her mind room (PRIVACY-OF-THOUGHT.md). Its text was cut
    // to one line each (it measured 17230 first); the 63 left are the param's
    // schema itself, the feature. Measured, not guessed.
    // 17150 -> 17238 (PR #5059, 2026-10-15): code/edit now advertises
    // its required typed four-operation contract instead of an untyped,
    // optional operation that generated an invalid path-only retry. Hosted
    // run 38738554029 on 6aa0cbe21 measured 17238; deleting duplicate legacy
    // fallback fields first saved 280 tokens from 17518. Retain the remaining
    // 88 for the callable contract, with no slack or production budget change.
    // 17238 -> 17298 (PR #5112, 2026-10-16): work/list gains `limit` (default 40,
    // claimable first) so a board of 776 cards is never one 198k-char line again;
    // one optional field and its one-line doc, measured locally at 17298.
    const AGENTIC_SURFACE_CEILING: u32 = 17298;
    let surface = faculty.describe_tool_tokens() as u32 + faculty.framing_floor_tokens();
    println!("agentic surface: {surface} guard tokens; ceiling {AGENTIC_SURFACE_CEILING}");
    assert!(
        surface <= AGENTIC_SURFACE_CEILING,
        "the agentic surface is now {surface} tokens (ceiling \
         {AGENTIC_SURFACE_CEILING}) — framing/tools grew. Shrink the surface (#333) \
         or state plainly what was added and re-pin the ceiling"
    );
    assert!(
        needed > window,
        "8192 no longer hosts the agentic surface ({needed} needed) — if this \
         flips back the surface genuinely shrank, which is GOOD news: restore the \
         original `needed <= window` narrative and drop the ceiling"
    );
    // #327, and the assertion here is now the SURVIVAL check rather than its
    // inverse. It used to read `!contains(…)` — "at 8192 the newest burst line
    // does NOT survive; the framing is intact and the hands are intact, the
    // CONVERSATION is what got zero" — with a note to restore the survival check
    // if it ever flipped. It flipped. Card dec1a7ff flipped it.
    //
    // Note WHAT changed, because the old note guessed wrong about the cause: the
    // surface did NOT shrink and the framing did NOT shrink. The ceiling
    // assertion above still passes and `needed > window` still holds, both
    // measuring the same surface as before. What changed is that the budget
    // stopped RESERVING for a surface it does not send. This turn narrows to her
    // hands, and the message budget used to subtract the whole registry anyway —
    // so the conversation was trimmed to fit room that was never occupied, and
    // the newest line fell off the end of a window that had space for it.
    //
    // That is the difference between "this window is too small for a citizen"
    // and "we were spending her window on tools we withheld". Only the second
    // was ever true here.
    assert!(
        view.user_text()
            .contains("LATEST: did the deploy fix land?"),
        "the newest burst line must survive once the budget prices the surface it \
         actually sends — if this regresses, the accounting is double-counting the \
         withheld registry again (card dec1a7ff); selected={}, estimated={}, reserve={}, capacity={:?}\n{}",
        faculty.select_tool_surface(&ws, window).tokens,
        view.estimated_prompt_tokens,
        view.completion_reserve,
        view.capacity_error,
        &view.user_text()[..view.user_text().len().min(600)]
    );
    assert!(
        view.system.contains("Taking your turn"),
        "framing survives regardless"
    );

    // …and at a window that CAN host the surface, the conversation is heard. Same
    // faculty, same burst, same tools — only the window differs, which is what makes
    // this a capacity fact rather than a cognition bug.
    let roomy = LlmDeliberationFaculty::new(
        persona,
        "Asha",
        "You are Asha.",
        Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_tools(roomy_tools)
    .with_context_window(16_384);
    let roomy_view = roomy.prompt_view(&ws);
    assert!(
        roomy_view
            .user_text()
            .contains("LATEST: did the deploy fix land?"),
        "given a window that fits the surface, the newest message reaches her"
    );
}

// what this catches: THE echo-loop fix. A mixed thread (peer → self → peer)
// must reach the model as ROLE-ATTRIBUTED messages — the persona's own earlier
// posts as `assistant`, peers' as `user` — not flattened into one `user` blob.
// The old single-`user`-message assembly fed the persona its own words back as
// if a peer had said them, so it re-explained / re-proposed and looped ("Would
// you like me to start?"). Role separation is what lets the model see "I already
// said that" and move on. Regressing to a flat blob brings the loop back.
#[test]
fn mixed_thread_attributes_self_to_assistant_and_peers_to_user() {
    use crate::cognition::workspace::Burst;
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Asha",
        "You are Asha, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(8192);

    let room = crate::identity::ActivityRoom::mint();
    let turns = vec![
        BurstTurn::attributed(false, "Operator", "can you summarize the thread?", Some(1)),
        BurstTurn::attributed(true, "Asha", "I propose using bart-large-cnn.", Some(2)),
        BurstTurn::attributed(false, "Operator", "go ahead.", Some(3)),
    ];
    let ws = Workspace::new(Burst::from_turns(room, turns));
    let view = faculty.prompt_view(&ws);

    // Three turns alternate roles → three messages, user/assistant/user.
    let roles: Vec<&str> = view.messages.iter().map(|m| m.role.as_str()).collect();
    // Trailing extra user turn = the #152 [context] bounds fact
    // (always present). The steps-ledger is gated on a wired
    // WorkingMemory (this harness faculty has none), so no ledger
    // message here — the ledger-specific behavior is pinned in
    // steps_ledger_renders_receipts_and_explicit_zero_case below.
    // …plus the volatile presence-framing turn (2026-09-01: it rides the
    // facts phase instead of churning the system prefix), still ask-last.
    assert_eq!(
        roles,
        vec!["user", "assistant", "user", "user", "user"],
        "view: {view:?}"
    );
    assert!(
        view.messages
            .iter()
            .any(|m| matches!(&m.content, crate::ai::types::MessageContent::Text(t) if t.contains("[context] 3 input turns were available before prompt fitting"))),
        "the [context] bounds fact states the visible window"
    );

    // The persona's own line is the `assistant` turn and carries NO name prefix
    // (her own voice; the system prompt forbids self-prefixing). Peers' lines are
    // `user` turns prefixed with the author so several speakers stay distinct.
    let assistant = &view.messages[1];
    assert_eq!(assistant.role, "assistant");
    assert_eq!(
        assistant.content_text(),
        "[occurred 1970-01-01T00:00:00.002Z] I propose using bart-large-cnn."
    );
    assert!(
        !assistant.content_text().contains("Asha:"),
        "the persona's own turn must not be self-prefixed: {assistant:?}"
    );
    assert!(view.messages[0]
        .content_text()
        .starts_with("[occurred 1970-01-01T00:00:00.001Z] Operator: "));
    // Perception facts are GROUNDING inserted BEFORE the final ask (the last user
    // turn), so the ask stays LAST where the model answers it — not the bracketed
    // meta, which it would otherwise parrot (2026-07-20 humaneval parrot fix). Here
    // the layout is [Joel, Asha(assistant), presence (her epoch's standing framing),
    // [context]-fact, Joel-ask]: the fact sits just before the ask, the ask is last.
    let before_ask = &view.messages[view.messages.len() - 2];
    assert!(
        before_ask.content_text().starts_with("[context]"),
        "grounding fact sits just before the ask: {before_ask:?}"
    );
    assert!(
        view.messages
            .last()
            .unwrap()
            .content_text()
            .starts_with("[occurred 1970-01-01T00:00:00.003Z] Operator: "),
        "the ask (last peer turn) stays LAST, after the grounding facts"
    );
}

#[test]
// what this catches (card d33e928a, Kimi 2026-09-26): the front-drop emptied a
// turn's whole optional history with no receipt — room messages, operator
// direction included, gone before compose and nothing said. The fit now accounts
// for what it kept and dropped, and says when it emptied the history.
fn fit_receipt_names_what_it_dropped_and_when_it_emptied_the_history() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let history: Vec<ChatMessage> = (0..12)
        .map(|i| ChatMessage::text("user", format!("room {i} {}", "word ".repeat(95))))
        .collect();
    let stimulus = ChatMessage::text("user", "operator: do the thing ".repeat(40));
    let prompt = |history: Vec<ChatMessage>| PromptMessages {
        body: Vec::new(),
        headroom: 0,
        result_pointers: Vec::new(),
        input_identity: [0; 32],
        grounding_tokens: 0,
        grounding_at: 0,
        history,
        stimulus: Some(stimulus.clone()),
        latest_result: Vec::new(), action_statement: None,
        room_updates: Vec::new(), ledger: None, notices: Vec::new(),
    };
    let required = prompt(Vec::new()).required_tokens();
    let total: usize = history.iter().map(LlmDeliberationFaculty::message_cost).sum();

    // Room for everything: nothing dropped, the receipt says so.
    let roomy = faculty.fit_messages(prompt(history.clone()), required + total).expect("fits");
    assert_eq!(roomy.receipt.dropped_messages, 0);
    assert_eq!(roomy.receipt.kept_messages, 12);
    assert!(!roomy.receipt.emptied());

    // A budget that holds the stimulus and a little history: some dropped, some kept.
    let tight = faculty.fit_messages(prompt(history.clone()), required + total / 2).expect("fits");
    assert!(tight.receipt.dropped_messages > 0 && tight.receipt.kept_messages > 0, "{:?}", tight.receipt);
    assert_eq!(tight.receipt.dropped_messages + tight.receipt.kept_messages, 12);
    assert!(tight.receipt.dropped_tokens >= total - total / 2, "{:?}", tight.receipt);

    // A budget that holds only the stimulus: the whole history goes, and the
    // receipt says emptied (the turn Kimi could not see).
    let starved = faculty.fit_messages(prompt(history.clone()), required).expect("the stimulus fits");
    assert!(starved.receipt.emptied(), "{:?}", starved.receipt);
    assert_eq!(starved.receipt.dropped_tokens, total);
    assert_eq!(starved.receipt.history_budget, 0);
    assert!(starved.messages.iter().any(|m| m.content_text() == stimulus.content_text()),
        "the stimulus itself is never optional");
}

// what this catches: a turnover fit that leaves her new epoch no room to grow (the
// next act turns over again, every act a cold prefill), shears a message, or opens
// on an answer without its question.
#[test]
fn a_turnover_fit_frees_her_growth_on_whole_messages_and_clean_clusters() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let msgs: Vec<ChatMessage> = (0..40)
        .map(|i| ChatMessage::text("user", format!("m{i} {}", "word ".repeat(95))))
        .collect();
    let budget = 2000;
    let history = |history: Vec<ChatMessage>| PromptMessages {
        body: Vec::new(),
        headroom: 0,
        result_pointers: Vec::new(),
        input_identity: [0; 32],
        grounding_tokens: 0,
        grounding_at: 0,
        history,
        stimulus: None,
        latest_result: Vec::new(), action_statement: None,
        room_updates: Vec::new(), ledger: None, notices: Vec::new(),
    };
    // Her measured growth per act: what a turnover must leave free.
    let growth = 300;
    let fit = PromptMessages { headroom: growth, ..history(msgs.clone()) };
    let last_fit = faculty.fit_messages(fit, budget).expect("a suffix fits").messages;
    assert!(
        LlmDeliberationFaculty::messages_cost(&last_fit) + growth <= budget,
        "a turnover leaves her measured growth free: {} of {budget}",
        LlmDeliberationFaculty::messages_cost(&last_fit)
    );
    let first = last_fit.first().map(|m| m.content_text());
    assert!(
        msgs.iter().any(|m| Some(m.content_text()) == first),
        "front message must be a whole original message"
    );
    // CLUSTER GRANULARITY (Joel 9/1): the window must open on a clean
    // opener — when the cut lands on an assistant answer or a
    // tool-result continuation, the front advances past the fragment.
    let mut clustered: Vec<ChatMessage> = Vec::new();
    for i in 0..30 {
        clustered.push(ChatMessage::text(
            "user",
            format!("q{i} {}", "word ".repeat(60)),
        ));
        clustered.push(ChatMessage::text(
            "assistant",
            format!("a{i} {}", "word ".repeat(60)),
        ));
    }
    let fit = faculty.fit_messages(history(clustered), budget).expect("a suffix fits").messages;
    assert_eq!(
        fit.first().unwrap().role,
        "user",
        "the window must never open on an answer without its question"
    );
    // A quantum larger than the optional history must not evict every
    // message on a quiet self-tick with no external stimulus or result.
    let ambient = || history(vec![
        ChatMessage::text("user", "old situation ".repeat(30)),
        ChatMessage::text("user", "current situation ".repeat(20)),
    ]);
    let ambient_cost =
        LlmDeliberationFaculty::message_cost(ambient().history.last().unwrap());
    let fitted = faculty
        .fit_messages(ambient(), ambient_cost)
        .unwrap()
        .messages;
    assert_eq!(fitted.len(), 1);
    assert_eq!(fitted[0].content_text(), "current situation ".repeat(20));
    assert!(faculty.fit_messages(ambient(), ambient_cost - 1).is_err());
}

// what this catches: the steps ledger placed where its per-act change forfeits the
// stable prefix (card 5b09111e), or placed after the ask where it gets parroted (the
// 2026-07-20 humaneval fix). With a pinned result it rides after the room updates and
// the result stays last; with none it keeps its place ahead of the ask.
#[test]
fn the_steps_ledger_rides_before_the_pinned_result_and_never_after_the_ask() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let prompt = |result: bool| PromptMessages {
        body: Vec::new(),
        headroom: 0,
        result_pointers: Vec::new(),
        input_identity: [0; 32], grounding_tokens: 0, grounding_at: 0,
        history: vec![ChatMessage::text("user", "[work turn] fix the bug")],
        stimulus: Some(ChatMessage::text("user", "the ask")),
        latest_result: result.then(|| ChatMessage::text("user", "Full result of your most recent action")).into_iter().collect(), action_statement: None,
        room_updates: vec![ChatMessage::text("user", "[Room message received during this turn] hi")],
        notices: Vec::new(),
        ledger: Some(ChatMessage::text("user", "[steps taken this session]\n[action #1832] code/read")),
    };
    let order = |result: bool| -> Vec<String> {
        faculty.fit_messages(prompt(result), 100_000).expect("test: fits").messages.iter()
            .map(|m| m.content_text().lines().next().unwrap_or("").to_string()).collect()
    };
    assert_eq!(order(true), vec![
        "[work turn] fix the bug", "the ask", "[Room message received during this turn] hi",
        "[steps taken this session]", "Full result of your most recent action",
    ]);
    assert_eq!(order(false), vec![
        "[work turn] fix the bug", "[steps taken this session]", "the ask",
        "[Room message received during this turn] hi",
    ]);
}

// Regression for #4290: a follow-up must not erase protection for the act it asks about.
#[test]
fn continuity_followup_keeps_completed_action_evidence() {
    let history = vec![
        ChatMessage::text("user", "Write the requested file."),
        ChatMessage::text("assistant", "calling code/write: completed.py"),
        ChatMessage::text("user", "Full result of code/write: file written successfully."),
        ChatMessage::text("user", "Now review the file you just wrote."),
    ];
    let expected = LlmDeliberationFaculty::messages_cost(&history);
    let prompt = PromptMessages {
        body: Vec::new(),
        headroom: 0,
        result_pointers: Vec::new(),
        input_identity: [0; 32], grounding_tokens: 0, grounding_at: 0,
        history, stimulus: None, latest_result: Vec::new(), action_statement: None, room_updates: Vec::new(), ledger: None, notices: Vec::new(),
    };
    assert!(prompt.recent_move_evidence().tokens >= expected,
        "a new follow-up cannot make the preceding action and result disposable");
}

// Regression for #4290: insufficient capacity must refuse, not silently forget a completed act.
#[test]
fn continuity_insufficient_capacity_refuses_instead_of_dropping_evidence() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let prompt = PromptMessages {
        body: Vec::new(),
        headroom: 0,
        result_pointers: Vec::new(),
        input_identity: [0; 32], grounding_tokens: 0, grounding_at: 0,
        history: vec![
            ChatMessage::text("user", "Write the requested file. ".repeat(30)),
            ChatMessage::text("assistant", "calling code/write: completed.py ".repeat(30)),
            ChatMessage::text("user", "Full result of code/write: file written successfully."),
        ],
        stimulus: None, latest_result: Vec::new(), action_statement: None, room_updates: Vec::new(), ledger: None, notices: Vec::new(),
    };
    let insufficient = prompt.recent_move_evidence().tokens - 1;
    assert!(faculty.fit_messages(prompt, insufficient).is_err(),
        "required action/result continuity must not become optional when it exceeds capacity");
}

// what this catches: a faculty bound with `model: None` (the normal boot binding)
// reading the prefill-rate ladder under no name and taking the unmeasured floor
// while the served model's rate sat measured in the ladder (the M5, 2026-10-18).
#[test]
fn a_binding_without_a_model_name_carries_the_adapters_served_model() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let served = adapter.default_model().to_owned();
    assert!(!served.is_empty(), "the fixture adapter names a model");
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    assert_eq!(faculty.binding.load().model.as_deref(), Some(served.as_str()),
        "a bound faculty always names the model it is bound to");
}
