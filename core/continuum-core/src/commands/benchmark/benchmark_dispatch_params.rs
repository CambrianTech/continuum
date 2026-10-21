use super::*;

// ───────────────────────── benchmark/dispatch ─────────────────────
//
// #346 (Joel, 2026-08-07): benchmarks delivered as WORK CARDS — measured for
// what citizens DO through the ordinary kanban loop, not what a harness drives
// them through. The empirical trigger: the live board's cards are substrate
// diagnosis narratives (cross-grid network debugging) a coding citizen cannot
// act on — Anwen held one, accurately reported "no progress", and paid 15,816
// prefill tokens to conclude it. Coders code when the board contains coding
// work sized to them; THIS is the verb that puts it there.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkDispatchParams.ts"
)]
pub struct BenchmarkDispatchParams {
    /// The benchmark name (see `benchmark/list`), e.g. `tool-bugfix-rs`.
    /// Optional when `recipe` is given (the recipe's rows carry the names).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// Execute a stored RECIPE by name instead of hand-assembled flags: a row
    /// in the `benchmark_recipes` collection (author with `data/create`)
    /// carrying the model to serve and the dispatches to fire. Dispatch pins
    /// the model (fit-gated), awaits lane readiness, then fires every entry —
    /// the whole experiment is two commands: `reboot` + `dispatch --recipe X`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub recipe: Option<String>,
    /// Parameters for a TEMPLATE recipe: every `{key}` placeholder in the
    /// row's string fields is substituted from this map before execution, so
    /// one recipe ("challenge: candidate takes the incumbent's misses") serves
    /// every model — the model is an ARGUMENT, never data baked into the row.
    /// Unresolved placeholders fail loud, naming the missing key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "Record<string, string>")]
    pub params: Option<std::collections::BTreeMap<String, String>>,
    /// TEAM solve: citizen display names joined to every dispatched card's
    /// solve room beside its claimer, each charged to cross-review before
    /// submit (#team-proof gap 1). Recipe entries may override per dispatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub teammates: Option<Vec<String>>,
    /// How many tasks (from the top) to post as cards. Omit for all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
    /// Board repo key the cards land under, e.g. `CambrianTech/continuum`.
    /// Omit to reuse the repo of the cards already on the board — data-driven,
    /// no baked-in default. Required only when the board is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repo: Option<String>,
    /// Citizen display names to direct the work at, round-robin. Each card gets
    /// an addressed kickoff message in-room naming the assignee + card id — the
    /// empirically proven activation path (an addressed imperative in its OWN
    /// message block actuates; a card sitting silently on the board does not).
    /// Every name MUST be a citizen currently online (dispatch fails loud on an
    /// unknown name, listing who is online) — never our specific roster, always
    /// this machine's live citizens. OMIT to dispatch to the WHOLE live roster
    /// (whoever this repo user has spawned) — the general default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assignees: Option<Vec<String>>,
    /// Restrict a SWE-class dispatch to these exact `instance_id`s (e.g.
    /// `sympy__sympy-24152`), in this order — instead of taking the first `limit` from the
    /// dataset. Substring match, so a short id (`sympy-24152`) also selects. Omit to dispatch
    /// the dataset head. Lets a caller target a KNOWN-buildable instance rather than whatever
    /// sits first in the dataset (astropy's C-extension build is the hard tail, #383, and it
    /// leads swe-bench-lite). Ignored for gym-class benchmarks.
    #[serde(default)]
    pub instances: Option<Vec<String>>,
    /// Deterministic RANDOM SAMPLE: take this many instances chosen by `seed`
    /// instead of the dataset head. `(dataset, seed, sample)` fully determines
    /// the list on every machine — the flag pair IS the replication recipe, so
    /// publish both alongside the score. Combines with `limit` (sample wins),
    /// refused alongside explicit `instances`. SWE-class benchmarks only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub sample: Option<u32>,
    /// RNG seed for `sample` (default 0). Same LCG as the generated gyms — no
    /// platform rand, byte-stable selection forever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub seed: Option<u64>,
    /// The room this run lives in. Omit to get a FRESH one per run, named
    /// `bench-<benchmark>-<epoch>`.
    ///
    /// **A run is an activity, and an activity is a room.** Before this existed,
    /// dispatch had no way to say where — so every suite, every run, forever, piled
    /// into whichever room the curator happened to be standing in. Measured on one
    /// 37-minute window of that pile: 136 cards with 66 already CLOSED and still
    /// resident, and 5,336 of 5,345 inbound events discarded as bookkeeping —
    /// ~48 wake-ups per minute per citizen, of which NINE in 37 minutes were
    /// something a mind could actually read. Claim heartbeats scale as
    /// `O(claims × citizens)`, so the more work a citizen held, the less capacity
    /// it had to do any of it.
    ///
    /// Also CLOSE this benchmark's redundant duplicate cards, converging the board
    /// to one live card per task. Off by default — a dispatch that silently closed
    /// cards would be a surprising verb.
    ///
    /// A card someone is genuinely working (a live claim) is never closed; if two
    /// citizens hold the SAME task, both are kept and the contention is reported
    /// rather than resolved by cancelling one of them. Pair with `limit=0` to prune
    /// without dispatching anything new.
    #[serde(default)]
    pub prune: Option<bool>,
    /// Who works this round's cards: `detached_solve` (default) or `citizen`.
    ///
    /// - `detached_solve` — a forked copy of the citizen solves each card through
    ///   `agent/solve`, with an exclusive warm slot. Proven; it produced our one SWE
    ///   pass. It also produces no room turn, so the round teaches nobody (#456).
    /// - `citizen` — nothing detached fires. The kickoff drives her to claim, and she
    ///   works the card on her own held-work turn: hands rooted at the staged checkout,
    ///   acts radiating into the run room, and the turn feeding the training producer.
    ///
    /// The score and the learning are both the objective, and only `citizen` can
    /// deliver the second one — but it depends on the kickoff→claim hop that used to
    /// stall rounds, so it is opt-in until that hop is proven under residency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub drive: Option<crate::cognition::bench_round::WorkDriver>,
    /// The round's STANDING RULES — published as the run room's operating doctrine
    /// so every resident sees it in her grounding on every turn (the `[Room
    /// operating doctrine]` block), not as a message that scrolls out of view.
    /// This is where a recipe puts role prompts and the collaboration nudge
    /// ("others are working cards on this board — say what you found when it
    /// helps them"). Omit for the control arm: no doctrine, no nudge.
    pub doctrine: Option<String>,
    /// Stage the round even though serving is NOT decode-verified (#442).
    ///
    /// Off by default, and the default is the point: dispatch refuses to post cards no
    /// citizen can work, because a round staged into a dead lane looks dispatched and is
    /// inert (#455). This is the explicit operator override — same contract as
    /// `start --force` (#420) — and it announces itself in the log rather than passing
    /// silently, since a gate that can be skipped without a trace is not a gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub force: Option<bool>,
    /// THE REVIEW GATE: an owner's `done` becomes `review` + a sibling review card a
    /// NON-owner pulls; only the reviewer's `done` closes the card and fires its
    /// grade. Off = the control arm (a citizen's own `done` grades directly).
    pub review_gate: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkDispatchResult.ts"
)]
pub struct BenchmarkDispatchResult {
    pub benchmark: String,
    /// The room this run lives in — where its board, its kickoffs and its citizens are.
    /// Returned so the caller never has to guess where the work went.
    pub room: String,
    /// That room's airc channel id — the TYPE, not a uuid-shaped string. See
    /// `ActivitySpawnResult::room_id`; this field is that value passed through.
    #[ts(type = "string")]
    pub room_id: airc_core::RoomId,
    /// The wall post binding that room to the benchmark recipe — passed straight through
    /// from the `spawn_activity_room` this dispatch performed.
    ///
    /// A run ROOTS AN ACTIVITY; this is the receipt that says so, and returning it is what
    /// lets `activity/spawn` route a benchmark recipe here and still answer in its own
    /// shape. Without it the two verbs could not report the same spawn, which is the seam
    /// where "benchmarks are a parallel runner" creeps back in.
    #[ts(type = "string")]
    pub binding_post_id: uuid::Uuid,
    /// Cards actually posted to the board.
    #[ts(type = "number")]
    pub dispatched: u32,
    /// Short ids of the posted cards, in task order.
    pub card_ids: Vec<String>,
    /// Tasks NOT dispatched because they need harness-side orchestration a
    /// claimed card cannot provide yet (setup_shell workspace restoration).
    /// Reported, never silently dropped — a partial dispatch that reads as
    /// full coverage is the lie this field exists to prevent.
    #[ts(type = "number")]
    pub skipped_needs_setup: u32,
    /// Instances withheld because THIS box already proved their (repo, era) env
    /// class red via `benchmark/validate`. Reported, never silent — the operator
    /// must see that the round is smaller than requested and WHY (the named wall
    /// rides in `kickoff_errors`).
    #[serde(default)]
    pub skipped_known_red: u32,
    /// Tasks NOT dispatched because a LIVE card for that exact task is already on
    /// the board (same `[bench <name>] <task_id>:` key, in any non-terminal state).
    /// Dispatch is idempotent per task: re-running it tops the board up to one card
    /// per task instead of posting a second copy.
    ///
    /// Why this exists: without it, every re-dispatch re-posted the dataset head as
    /// brand-new cards. Measured on the live board 2026-08-13 — 124 bench cards for
    /// 51 distinct tasks, `sympy__sympy-24152` alone holding 15 copies, with two
    /// citizens solving the SAME instance in parallel. That wastes scarce lanes and
    /// leaves the pass rate with no honest denominator.
    #[ts(type = "number")]
    pub skipped_already_on_board: u32,
    /// Instances a citizen's card already resolved: they stay in the round's sample and
    /// its score, and are not offered as work again (2026-09-08).
    #[serde(default)]
    pub skipped_already_resolved: u32,
    /// Redundant duplicate cards CLOSED by this call (only when `prune` was set).
    /// Cards under a live claim are never counted here because they are never
    /// closed — see `contended_tasks`.
    #[ts(type = "number")]
    pub pruned_duplicates: u32,
    /// Tasks where MORE THAN ONE citizen holds a live claim on a duplicate card.
    /// The prune leaves all of them alone: cancelling one would destroy real
    /// in-flight work, so this is surfaced as a coordination fact for the room to
    /// settle rather than resolved silently.
    #[ts(type = "number")]
    pub contended_tasks: u32,
    /// Addressed kickoff messages actually delivered (one per dispatched card —
    /// every card is directed at a live citizen). A kickoff that failed to send is
    /// reported via `kickoff_errors`, never silently counted as delivered.
    #[ts(type = "number")]
    pub kickoffs: u32,
    /// Scored solves FIRED directly for directed SWE assignees — the loop starting work
    /// without waiting on a cognitive `work/claim` (the hop that stalls under warm-slot
    /// starvation). One per staged directed SWE card. `dispatched - solves_fired` are the
    /// gym / undirected cards that still start on organic claim.
    #[ts(type = "number")]
    pub solves_fired: u32,
    /// Send failures for kickoff messages, card-id-prefixed. The cards are ON
    /// the board regardless — a failed kickoff is reported, not unwound; the
    /// citizen can still find and claim the card off the board.
    pub kickoff_errors: Vec<String>,
}

/// Which of a task's duplicate cards to CLOSE, and whether the duplication is
/// contended. Pure over the claim states so the rule is testable without a
/// board — the caller maps the returned indices back to cards.
///
/// The rule, in priority order:
///  • A card someone is genuinely ON (`Hold::Held`) is NEVER closed. Duplicates
///    are board litter; an in-flight claim is a citizen's work, and destroying
///    it to tidy up would cost more than the duplication does.
///  • If no card is held, keep the FIRST and close the rest — they are
///    interchangeable, so the choice only has to be deterministic.
///  • If MORE THAN ONE card is held, every held card is kept and the caller is
///    told (`contended`). Two citizens really are on the same task; that is a
///    coordination fact for them to settle, not something a prune should
///    silently resolve by cancelling someone.
pub(super) fn duplicates_to_close(holds: &[crate::persona::card_holder::Hold]) -> (Vec<usize>, bool) {
    use crate::persona::card_holder::Hold;
    if holds.len() <= 1 {
        return (Vec::new(), false);
    }
    let held: Vec<usize> = holds
        .iter()
        .enumerate()
        .filter(|(_, h)| matches!(h, Hold::Held))
        .map(|(i, _)| i)
        .collect();
    let to_close = if held.is_empty() {
        (1..holds.len()).collect()
    } else {
        (0..holds.len()).filter(|i| !held.contains(i)).collect()
    };
    (to_close, held.len() > 1)
}

/// Compose the card BODY: the full prompt plus a definition of done a citizen
/// can act on with her own hands. Grading inputs that must stay held out
/// (`expect`, the harness `test`) are deliberately NOT written to the card —
/// the DoD names the artifact, not the answer key. `solution_file` and
/// `dod_shell` are legitimately visible: real work has a visible definition
/// of done.
pub(crate) fn dispatch_card_body(bench: &str, t: &crate::cognition::eval::EvalTask) -> String {
    let mut body = format!(
        "benchmark: {bench}\ntask: {}\n\n{}\n",
        t.id,
        t.prompt.trim()
    );
    if let Some(f) = &t.solution_file {
        body.push_str(&format!(
            "\nWrite your solution to `{f}` in your workspace (code/write)."
        ));
    }
    if let Some(dod) = &t.dod_shell {
        body.push_str(&format!(
            "\nDefinition of done: `{dod}` exits 0 in your workspace — run it \
             yourself (code/shell) and iterate until green."
        ));
    }
    body.push_str(
        "\n\nWhen your artifact is in place, mark the card done (work/state). \
         Your work is graded from what you actually wrote and ran.",
    );
    body
}

/// Compose the card BODY for a REAL SWE-bench project instance — a live open-source issue
/// on a real repo. The issue text plus a definition of done a citizen acts on with her own
/// git hands. The gold `patch` and the held-out `test_patch` are deliberately NOT written:
/// the card names the repo + commit + which tests must pass, never the answer key (same
/// held-out discipline as [`dispatch_card_body`]). This is the real-project tier — "full
/// projects, not stupid homework" (Joel, 2026-08-10).
pub(crate) fn dispatch_swe_card_body(
    bench: &str,
    i: &crate::cognition::swe_bench::SweInstance,
) -> String {
    let f2p = i.f2p();
    let tests = if f2p.is_empty() {
        "the repo's held-out failing tests".to_string()
    } else {
        f2p.join(", ")
    };
    // THE DEFINITION OF DONE LEADS. The card is a wall post, and a wall render keeps a
    // body PREFIX (wall_source: max_chars − reserved) — with the tests named after a long
    // problem statement, every holder's first turns went to "determine the FAIL_TO_PASS
    // tests" by reading the dataset (Kira, Atlas, 2026-09-13 11:2xZ) for a fact the card
    // already carried, below the cut.
    format!(
        "benchmark: {bench}\ninstance: {}\nrepo: {} @ {}\n\
         DEFINITION OF DONE: these tests pass — {}. THOSE TESTS ARE NOT IN YOUR CHECKOUT: the \
         grader adds them from a held-out test patch at verdict time, so do not search the tree \
         for them or try to run them by name — write your own repro from the issue text, and use \
         the tests that already exist in that file/module as the regression set. Do not edit \
         existing tests; your DIFF is graded against the repo's held-out suite.\n\n{}\n\n\
         This is a REAL open-source issue. When you CLAIM this card, the repo is staged in \
         your workspace at `swe/{}/` (checked out at the buggy commit), and while you hold the \
         card your hands (files, shell) are rooted AT that repo's root: paths are repo-relative \
         and `ls` lists the repo itself. Nothing starts by itself — you fix the bug IN PLACE with \
         your tools and prove it with your own repro. END EVERY WORK TURN with work/note (the \
         card's ledger: what you established, the competing hypotheses with the test that settles \
         each, the one unknown, the next test) — your next turn opens from it instead of \
         re-orienting.",
        i.instance_id,
        i.repo,
        i.base_commit,
        tests,
        i.problem_statement.trim(),
        i.instance_id,
    )
}

/// `benchmark/dispatch` — post a benchmark's tasks as claimable work cards on
/// the shared board. Citizens claim and work them through the SAME kanban loop
/// as any other work; nothing about the turn is exam-shaped.
pub struct BenchmarkDispatch {
    pub registry: crate::persona::PersonaAircRuntimeRegistry,
    /// Late-bound substrate executor (ChatModule pattern) — the recipe path
    /// composes `data/list` (load the recipe row) and `serving/pin` (re-home
    /// the lane, with pin.rs's full fit-gating) as COMMANDS, the universal
    /// primitive, never cross-module state threading.
    pub executor_slot: std::sync::Arc<
        crate::runtime::LateBound<crate::runtime::command_executor::CommandExecutor>,
    >,
}

/// A stored EXPERIMENT — one row in the `benchmark_recipes` collection,
/// authored through the data layer (`data/create`), executed by
/// `benchmark/dispatch --recipe <name>`. DATA, not code: fields grow
/// (serde-tolerant) without touching this file, and the sophistication lives
/// in the row — which model to serve, which dispatches to fire, and later
/// caps, condition labels, team shapes. Nothing model-specific is ever
/// hardcoded here; serving behavior comes from the model's own catalog row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRecipe {
    /// The name `--recipe` selects.
    pub name: String,
    /// One line of intent, shown when listing/erroring.
    #[serde(default)]
    pub description: String,
    /// Model this run must be SERVING before any card fires. `None` = run on
    /// whatever is live. `Some` = dispatch pins it (fit-gated by serving/pin)
    /// and awaits lane readiness before the first card.
    #[serde(default)]
    pub model_id: Option<String>,
    /// The dispatches to fire, in order — one experiment may span datasets.
    pub dispatches: Vec<RecipeDispatch>,
}

/// One dispatch inside a recipe: a benchmark plus the exact instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeDispatch {
    /// Benchmark name (see `benchmark/list`).
    pub benchmark: String,
    /// Exact instance ids. Empty = the dataset head up to `limit`.
    #[serde(default)]
    pub instances: Vec<String>,
    /// Optional per-dispatch card cap.
    #[serde(default)]
    pub limit: Option<u32>,
    /// TEAM solve (#team-proof gap 1): citizens joined to each card's solve
    /// room beside the claimer, carrying the cross-review charge. Names, like
    /// `assignees`; non-resident names are skipped with a probe.
    #[serde(default)]
    pub teammates: Vec<String>,
    /// ROLE-shaped team (recipe = content-type + RULES, #371): how many live
    /// residents to enroll as reviewers per card, resolved against the roster
    /// AT DISPATCH — a recipe that names citizens is brittle across machines;
    /// a recipe that names ROLES ships anywhere. Resolved reviewers merge with
    /// any explicit `teammates`; the assignee is never their own reviewer.
    #[serde(default)]
    pub reviewers: Option<u32>,
    /// Standing rules for the run room (see `BenchmarkDispatchParams::doctrine`).
    #[serde(default)]
    pub doctrine: Option<String>,
    /// The review gate (see `BenchmarkDispatchParams::review_gate`).
    #[serde(default)]
    pub review_gate: Option<bool>,
}

/// The room a run lands in when the caller names none: `bench-<benchmark>-<epoch>`.
///
/// Named for the ACTIVITY and stamped so it is THIS run — the naming rule
/// `activity/spawn` documents, and the reason it matters here specifically: a room named
/// for a SUBSYSTEM (`#academy`, `#benchmarks`) never finishes, so it reads as a permanent
/// place and quietly becomes the room every run reuses forever. That is precisely the
/// 136-card pile this parameter exists to end.
///
/// Flattened with `-` rather than the `academy/bench/<run>` path form the design of record
/// uses, because airc channel names accept only `[a-z0-9_-]` (`ChannelName::new` rejects
/// `/`). The tree is a naming convention waiting on a channel-name grammar, not something
/// this function can invent unilaterally.
pub(super) fn default_run_room_name(benchmark: &str, epoch_secs: u64) -> String {
    let slug: String = benchmark
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("bench-{slug}-{epoch_secs}")
}

/// Resolve the citizens a directed dispatch addresses — GENERALIZED for any repo user's
/// roster, never our specific names. Pure over the snapshots so it is unit-testable
/// without a running airc daemon (a real `PersonaSlot` needs one); the wrapper in `run`
/// feeds `registry.resident_snapshot()` and `registry.roster_snapshot()` in.
///
/// - `requested` empty → the WHOLE RESIDENT roster (whoever THIS machine has in the room).
///   This is the "dispatch to my citizens, whoever they are" default: a fresh clone runs
///   `benchmark/dispatch --name=…` with no `--assignees` and it targets their own resident
///   citizens. Directed dispatch is what actuates (a silent card does not — measured
///   2026-08-07), so defaulting to the resident roster keeps the loop autonomous everywhere.
/// - `requested` non-empty → every name MUST resolve to a RESIDENT citizen; anything else
///   FAILS LOUD listing who is resident (never silently addresses a citizen who cannot
///   hear, and never silently skips SWE staging). Order is preserved for round-robin.
/// - nobody resident → `Denied`, and the message distinguishes "not spawned" (fix:
///   `persona/spawn`) from "spawned but not hosted yet" (fix: wait — see below).
///
/// `live` is the RESIDENT snapshot (service loop attached and running), NOT the registered
/// roster — `registered` is passed alongside it purely so a refusal can tell the operator
/// WHICH of the two states they are in. That distinction is the whole point:
///
/// - registered ∧ ¬resident → she exists but has no perception stream. Hosting is waiting
///   on something (usually a serving lane proving it can decode, #363). Work staged now is
///   posted into an empty room and never worked.
/// - ¬registered → nobody spawned her. The fix is `persona/spawn`, a different action.
///
/// Measured 2026-08-18: dispatching against the REGISTERED roster in the first state
/// reported `dispatched: 2, kickoffs: 2, kickoff_errors: []` and produced zero turns.
pub(super) fn resolve_dispatch_roster(
    live: &[(String, uuid::Uuid)],
    registered: &[(String, uuid::Uuid)],
    requested: &[String],
) -> Result<Vec<(String, uuid::Uuid)>, CommandError> {
    if live.is_empty() {
        if registered.is_empty() {
            return Err(CommandError::Denied(
                "no citizens are online to work the cards — spawn a persona (persona/spawn) \
                 first, then dispatch."
                    .to_string(),
            ));
        }
        let names: Vec<&str> = registered.iter().map(|(n, _)| n.as_str()).collect();
        return Err(CommandError::Denied(format!(
            "citizen(s) [{}] are registered but NOT RESIDENT — no service loop, so no \
             perception stream, so nothing dispatched here would be heard. Hosting is \
             normally waiting on the serving lane to prove it can decode; watch \
             `inference.lane_relaunch_retry` and `persona.inbound.subscribe_opened`, and \
             re-dispatch once `persona/roster` reports resident_count > 0. Staging a round \
             into this window posts cards nobody can see.",
            names.join(", "),
        )));
    }
    if requested.is_empty() {
        return Ok(live.to_vec());
    }
    let mut resolved = Vec::with_capacity(requested.len());
    let mut unknown = Vec::new();
    for name in requested {
        match live.iter().find(|(n, _)| n == name) {
            Some(pair) => resolved.push(pair.clone()),
            None => unknown.push(name.clone()),
        }
    }
    if !unknown.is_empty() {
        let online: Vec<&str> = live.iter().map(|(n, _)| n.as_str()).collect();
        // Name the registered-but-not-resident case separately: "not online" reads as a
        // typo, and sending an operator hunting for a misspelling when the real answer is
        // "she is here but not hosted yet" is the same lie in a smaller box.
        let not_resident: Vec<&str> = unknown
            .iter()
            .filter(|n| registered.iter().any(|(r, _)| r == *n))
            .map(|s| s.as_str())
            .collect();
        let residency_note = if not_resident.is_empty() {
            String::new()
        } else {
            format!(
                " NOTE: [{}] are registered but not resident — they exist, they just have \
                 no service loop yet (hosting is likely waiting on a serving lane). That \
                 is a wait, not a typo.",
                not_resident.join(", ")
            )
        };
        return Err(CommandError::Invalid(format!(
            "assignee(s) not resident: {}. Citizens resident right now: [{}]. Pass names \
             from that list, or omit --assignees to dispatch to all of them.{}",
            unknown.join(", "),
            online.join(", "),
            residency_note,
        )));
    }
    Ok(resolved)
}

    pub(crate) enum CardWork {
        /// Gym: write a solution file; DoD is a compile/test shell.
        Gym { solution_file: String },
        /// SWE: the pulled instance. The CLAIM stages it into the claimer's
        /// workspace/swe/<instance> (`card_staging`); a detached-solve round stages
        /// its directed assignee at dispatch through the same function and fires
        /// her solve (#346) — the loop closes with nobody in it.
        Swe {
            instance: Box<crate::cognition::swe_bench::SweInstance>,
        },
    }

    pub(crate) struct PreparedCard {
        pub(crate) title: String,
        pub(crate) body: String,
        pub(crate) work: CardWork,
        /// Gym-only: a task needing a workspace re-break the card can't orchestrate yet.
        pub(crate) needs_setup: bool,
        /// The gym task's workspace-preparation shell, staged into the ASSIGNEE's
        /// workspace at dispatch (same contract as SWE checkout staging below) --
        /// idempotent by adapter convention (mkdir -p + overwrite-decode).
        pub(crate) setup_shell: Option<String>,
    }

/// What one dispatch SELECTS from a suite. Pure data, so the same selection can be
/// replayed: `(suite, seed, sample)` is the replication contract the score is
/// published with, and `instances` is the explicit alternative to it.
#[derive(Debug, Clone, Default)]
pub(crate) struct CardSelection {
    pub(crate) instances: Option<Vec<String>>,
    pub(crate) sample: Option<u32>,
    pub(crate) seed: Option<u64>,
    /// Keep only instances whose harness speaks this language (`rust`, `go`, `php`,
    /// `ruby`, `javascript`, `java`, `c`; `python` for the SWE-bench family) — the
    /// Multilingual dataset mixes nine, and a round is one language's board.
    /// `None`/empty = every language.
    pub(crate) language: Option<String>,
}

/// THE ONE WRITER of benchmark cards: task + oracle only, projected into the
/// title/body a citizen sees and the grader keys on (`[bench <suite>] <task>: …`).
/// No side effects — no room, no board, no env pre-warm — so `benchmark/import`
/// (the recipe path, S4) and `benchmark/dispatch` (the Rust path) post IDENTICAL
/// cards for the same selection by construction, not by a test that compares them.
pub(crate) async fn prepare_cards(
    spec: &BenchmarkSpec,
    selection: &CardSelection,
) -> Result<Vec<PreparedCard>, CommandError> {
    let prepared: Vec<PreparedCard> = if let Some(dataset) = spec.swe_dataset() {
        // Real-project tier: pull instances on demand (cached on first use), one
        // full-project card each. Reuses the SAME loader agent/solve grades against —
        // no second source of truth. THIS is what killed the "no runnable eval_set"
        // refusal that had blocked every SWE dispatch (Joel: "fix the goddamn thing").
        let mut instances = crate::cognition::swe_bench::load_dataset(dataset)
            .await
            .map_err(|e| {
                CommandError::Internal(format!("swe dataset '{dataset}' load failed: {e}"))
            })?;
        if let Some(lang) = selection.language.as_deref().filter(|l| !l.is_empty()) {
            let before = instances.len();
            instances.retain(|i| i.language() == lang);
            if instances.is_empty() {
                return Err(CommandError::Invalid(format!(
                    "no instance in '{dataset}' is `{lang}` (of {before}) — a language is \
                     what an instance's harness speaks: rust, go, php, ruby, javascript, \
                     java, c; python for the SWE-bench family"
                )));
            }
        }
        // Deterministic sample: (dataset, seed, n) → the same list on every
        // machine. Fisher-Yates over the dataset order with the shared LCG —
        // the command IS the replication recipe (no operator-side scripts).
        if let Some(n) = selection.sample.filter(|n| *n > 0) {
            if selection.instances.as_ref().is_some_and(|w| !w.is_empty()) {
                return Err(CommandError::Invalid(
                    "pass either `sample` (seeded random) or `instances` (explicit list),                          not both — they are competing selection recipes"
                        .into(),
                ));
            }
            let n = (n as usize).min(instances.len());
            let mut rng = crate::cognition::gym_rng::Lcg::new(selection.seed.unwrap_or(0)); // documented default seed: 0 is part of the replication contract, not a guess
            for i in 0..n {
                let j = i + rng.next(instances.len() - i);
                instances.swap(i, j);
            }
            instances.truncate(n);
            let list: Vec<&str> =
                instances.iter().map(|i| i.instance_id.as_str()).collect();
            tracing::info!(
                probe_class = "benchmark.dispatch.sample",
                dataset,
                n,
                seed = selection.seed.unwrap_or(0), // same documented default as the draw above
                instances = ?list,
                "seeded sample selected — publish (dataset, seed, n) with the score"
            );
        }
        // Caller-targeted instances win over dataset order — select by substring (so a
        // short id resolves) and preserve the CALLER's ordering, fail loud on a miss so a
        // typo never silently dispatches the wrong (or whole) set.
        if let Some(wanted) = selection.instances.as_ref().filter(|w| !w.is_empty()) {
            let mut picked: Vec<crate::cognition::swe_bench::SweInstance> = Vec::new();
            for want in wanted {
                match instances
                    .iter()
                    .position(|i| i.instance_id.contains(want.as_str()))
                {
                    Some(idx) => picked.push(instances.remove(idx)),
                    None => {
                        return Err(CommandError::Invalid(format!(
                            "no instance in '{dataset}' matches '{want}' — check the id (e.g. sympy__sympy-24152)"
                        )));
                    }
                }
            }
            instances = picked;
        }
        // ENV PRE-WARM (background): a cold native build (scikit's cython,
        // matplotlib's freetype) mid-round burns a solve attempt on an ENV
        // failure and reads as a model miss. Build every instance's env
        // AHEAD of the driver, in REVERSE card order so the warmer and the
        // solver approach from opposite ends (ensure_env itself holds the
        // per-instance lock, so even a meeting in the middle is safe).
        // Fire-and-forget: a prewarm failure is probed — the SAME failure
        // the solve would hit, surfaced hours earlier and attributable.
        instances
            .into_iter()
            .map(|i| PreparedCard {
                title: dispatch_card_title(spec.name, &i.instance_id, &i.problem_statement),
                body: dispatch_swe_card_body(spec.name, &i),
                needs_setup: false,
                setup_shell: None,
                work: CardWork::Swe {
                    instance: Box::new(i),
                },
            })
            .collect()
    } else {
        let reference = spec.eval_set.ok_or_else(|| {
            CommandError::Invalid(format!(
                "benchmark '{}' has no runnable eval_set yet — it is catalogued but its \
                 task collection hasn't been pulled/committed (see benchmark/list `runnable`)",
                spec.name
            ))
        })?;
        prepare_gym_cards(spec.name, reference)?
    };
    Ok(prepared)
}

/// The eval set a gym suite's cards resolve their oracle from, by the suite name a card title
/// carries: a registered suite's `eval_set`, or a coursework set's content-addressed file
/// (`cognition::coursework`). The ONE resolver import, preparation and the grader share, so a
/// coursework card is graded from its title exactly like a registered gym card. `None` = not a
/// gym suite this node can resolve.
pub(crate) fn eval_set_reference(suite: &str) -> Option<String> {
    if let Some(spec) = known_benchmarks().iter().find(|b| b.name == suite) {
        return spec.eval_set.map(str::to_string);
    }
    let home = crate::paths::continuum_home().ok()?;
    crate::cognition::coursework::set_path(&home, suite).map(|p| p.display().to_string())
}
