//! Wire types for `benchmark/dispatch`: params, result, and the recipe/dispatch descriptors it projects into the work board.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
