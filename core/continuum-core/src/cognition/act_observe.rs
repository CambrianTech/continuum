//! The act→observe motion — the organism's drive to settle.
//!
//! See [docs/cognition/ACTING-ORGANISM.md]. Action is not a return value the
//! deliberation faculty loops on internally; it is a [`Decision::Act`] verdict
//! the ORGANISM drives. This module holds that drive as a free function over a
//! [`WorkspaceCycle`] — never a method on the cycle, so `run_in_room` stays a
//! pure single tick (§3.3).
//!
//! Two callers share ONE motion (the load-bearing compression):
//! - the live heartbeat (`persona::service_loop`) calls [`apply_act`] ONCE per
//!   tick and lets the metronome bring the next perception — she re-perceives at
//!   heartbeat cadence, never on a synchronous inner counter (§4 Live = no
//!   deadline);
//! - the eval grader (`modules::cognition` `cognition/eval`) calls
//!   [`drive_to_settle`], which loops `apply_act` → re-tick under an EXTERNAL
//!   budget (the grader's stopwatch — the only special power an observer holds),
//!   because the synthetic eval room has no heartbeat servicing it.
//!
//! The load-bearing choice is **result-as-engram**: executing an action admits
//! its outcome as an Episodic engram (the persona observing its own hands), so
//! the result becomes a thing the mind remembers and can be reminded of next
//! tick — unifying it with how she carries every other fact. The disposition to
//! act (build→run→test) is the GENOME's to grow, never a Rust `if`; this module
//! only gives her the hands and the memory of using them.

use uuid::Uuid;

use super::workspace::{Burst, Decision, Situation, TurnFraming, TurnMetrics, WorkspaceCycle};
use crate::ai::types::ToolCall;

// Re-injection bounds (tool-result fold, echoed args) come from her LIVE served window via
// `ContextBudget` — never a constant. See `cognition/context_budget.rs` for why the old
// `RESULT_FOLD_MAX_CHARS = 16_000` / `ARG_FOLD_MAX_CHARS = 600` had to go.
use crate::cognition::context_budget::ContextBudget;

// The working-memory trail-head bound lives in `working_memory.rs` now (its home — WM owns
// its own truncation). Still used here for the settlement answer-head.

/// The result of driving a mind to settlement.
pub struct SettleOutcome {
    /// The verdict the mind settled on: `Speak`/`RaiseUnprompted`/`Pass` when it
    /// settled, or the final un-driven `Act` if the external budget ran out
    /// mid-action (the grader grades that as "did not finish" — honest, never a
    /// fabricated answer).
    pub decision: Decision,
    /// The spoken text, present only when the settled decision is `Speak` /
    /// `RaiseUnprompted`. This is what an external observer (the grader, or a peer
    /// in the room) reads.
    pub spoken: Option<String>,
    /// How many actions were executed before settling.
    pub acts: usize,
    /// The final world-state, with each action's observation folded in — what the
    /// last tick perceived. Captured for replay/forensics.
    pub world_state: String,
    /// The accumulated cost of settling this task: every act→observe deliberation
    /// generation's latency + tokens, summed. `tokens_per_second()` re-derives
    /// throughput from the totals. This is the speed/latency the eval reports next
    /// to the accuracy grade — the same number a live turn could surface for the
    /// serving governor.
    pub metrics: TurnMetrics,
    /// `Some(cause)` when the settle loop stopped because the deliberation model
    /// call FAILED (timeout, 5xx, a serving lane refusing a model it isn't hosting)
    /// rather than settling on a verdict. The grader MUST treat this as an
    /// infrastructure failure — NOT a wrong answer — so an inference hiccup never
    /// corrupts the accuracy metric ([[self-improvement-is-a-control-loop]]: the
    /// reward is only as trustworthy as the metric). `None` on every settled turn.
    pub inference_error: Option<String>,
}

impl SettleOutcome {
    /// An infra-failure outcome: no verdict was reached because the deliberation
    /// call failed OR was aborted by a watchdog (e.g. a per-task deadline in the
    /// eval loop). The grader keys on `inference_error.is_some()` to score this a
    /// NAMED infrastructure failure — never a wrong answer — so a serving wedge
    /// never masquerades as a capability miss ([[self-improvement-is-a-control-loop]]).
    /// Zeroed metrics/acts because none accrued meaningfully. `TurnMetrics: Default`.
    pub fn infra_failure(cause: impl Into<String>) -> Self {
        Self {
            decision: Decision::Pass,
            spoken: None,
            acts: 0,
            world_state: String::new(),
            metrics: TurnMetrics::default(),
            inference_error: Some(cause.into()),
        }
    }
}

/// The tail of the working-memory buffer SINCE the persona last settled (produced an
/// utterance). Everything before the most recent [`WM_SETTLEMENT_PREFIX`] boundary
/// belongs to a concern she already ANSWERED — an identical tool call over there is
/// not a spin, so re-issuing it for the current concern is legitimate. This is the
/// scope that separates "how many commands?" → answer → "list them again" (both emit
/// `commands/list({})`, legitimately) from re-issuing the SAME call within one
/// unanswered settling (the spin). Mirrors the `responded_through` boundary the
/// `ActThenSpeak` test faculty tracks — content-driven, not a turn counter.
fn entries_since_last_settlement(recent: &[String]) -> &[String] {
    match recent
        .iter()
        .rposition(|e| e.starts_with(crate::cognition::working_memory::WM_SETTLEMENT_PREFIX))
    {
        Some(i) => &recent[i + 1..],
        None => recent,
    }
}

/// True when EVERY call in this batch has ALREADY been carried out SINCE THE LAST
/// SETTLEMENT — the identical `(name, args)` already appears as a satisfied `I ran …`
/// trace in the not-yet-answered tail of the working-memory recency channel. Keyed on
/// the SAME `name(args)` rendering [`apply_act`] records below (`I ran {name}({args})`),
/// so detection and recording can never drift; scoped to the current concern by
/// [`entries_since_last_settlement`] so an identical call the mind already answered in
/// a PRIOR concern (still lingering in the volatile buffer) does not false-trigger.
///
/// This is content-driven proprioception, NOT an iteration counter: it fires only on a
/// byte-identical re-request whose result the mind already holds THIS concern —
/// precisely the signal the `[action #n]` stamp shift was MEANT to convey but
/// empirically does not. A greedy instruct model re-emits the identical `Act` despite
/// the shifted window (proven live 2026-07-02 via `cognition/prompt`, nil-room eval:
/// working memory carried `commands/list` + its full result and she re-issued
/// `commands/list` on every act, never converting the result to an answer). A MIXED
/// batch — any genuinely new call — is NOT a repeat: the new call yields new
/// perception and must run.
fn all_calls_already_satisfied(recent: &[String], calls: &[ToolCall], fold: Option<usize>) -> bool {
    if calls.is_empty() {
        return false;
    }
    let scope = entries_since_last_settlement(recent);
    calls.iter().all(|call| {
        // Bounded (#165): a whole-file `content` arg must not be echoed back verbatim ahead
        // of the RESULT — see `summarize_args_for_recency`.
        let args = summarize_args_for_recency(&call.input, fold);
        // Keyed on the `name(args)` core of the receipt render below — kept in sync
        // with it (the render dropped the first-person "I ran " opener, #158, so a
        // base model can't copy a receipt as a message opener; the fingerprint is
        // the stable `name(args)` substring both channels still carry).
        let signature = format!("{}({})", call.name, args);
        scope.iter().any(|trace| trace.contains(&signature))
    })
}

/// ORIENTATION commands — they SURVEY what the mind already carries and never read a
/// specific file or change the workspace. Two axes:
///   - tool-surface: `commands/help`/`commands/list` (the tool menu is already inline);
///   - workspace: `code/tree` (the `[workspace-map]` is already grounded in the prompt).
/// Matched on the canonical (slash) name after the wire-dialect map has run
/// (`help`→`commands/help`, `list_commands`→`commands/list`, `file_tree`→`code/tree`),
/// normalizing the underscore form defensively. NOT `code/list`: a one-dir listing to
/// get exact filenames before an edit is a legitimate narrowing step, not a survey.
fn is_orientation_call(name: &str) -> bool {
    matches!(
        name.replace('_', "/").as_str(),
        "commands/help" | "commands/list" | "code/tree"
    )
}

/// True when this batch is ALL orientation AND the current concern already holds a
/// discovery receipt — a redundant re-list that returns byte-identical perception.
/// Sibling of [`all_calls_already_satisfied`], keyed on the SAME `name(` receipt
/// render and the SAME per-concern scope ([`entries_since_last_settlement`]), so
/// detection and recording can never drift.
///
/// Why a dedicated demotion and not the exact-repeat guard: `commands/help(code/write)`
/// and `commands/help(code/run)` are DIFFERENT args, so the exact-repeat guard lets
/// them all through — yet each returns a surface the mind already has. Under the
/// `[Acting]` pressure a base model reaches for these as the cheapest schema-valid
/// "action" and files its real intent into prose (glass-boxed 2026-07-16: 1855/3288
/// live tool calls were this filler, e.g. nine straight `commands/help` turns on a
/// one-line `add(a,b)` while the answer sat ready). The FIRST orientation per concern
/// is honest; a REPEAT once any survey receipt is in the concern is spin.
///
/// The `code/tree` case (glass-boxed 2026-07-16, benchmark): after the tool-surface
/// demote crushed the `help`/`list_commands` filler 99%, the SAME act-pressure
/// disposition displaced to `code/tree` — 156 of 169 tool calls, arg-JITTERED
/// (`apps/cli` vs `apps/cli/`, `max_depth` 1 vs 2) to evade the exact-repeat guard,
/// re-surveying a tree the mind already fetched AND already has in `[workspace-map]`.
/// Demoting by CLASS + prior-receipt (ignoring args entirely) is immune to that jitter.
fn is_redundant_orientation(recent: &[String], calls: &[ToolCall]) -> bool {
    if calls.is_empty() || !calls.iter().all(|c| is_orientation_call(&c.name)) {
        return false;
    }
    let scope = entries_since_last_settlement(recent);
    scope.iter().any(|trace| {
        trace.contains("commands/list(")
            || trace.contains("commands/help(")
            || trace.contains("code/tree(")
    })
}

/// Recall salience for an action-observation receipt (#166). Below the neutral
/// default (0.5) so genuine findings/facts win recall, but well above zero so the
/// receipt stays recallable for "what did I just do" when nothing better matches.
const PROPRIOCEPTION_RECALL_SALIENCE: f32 = 0.25;

/// Char-safe truncate with a trailing ellipsis when cut.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
}

/// The RECENCY channel keeps the WHOLE latest result so the mind can act on what
/// it just fetched — but "whole" for a 5000-entry `code/list` or a multi-match
/// `code/search` is a multi-KB raw-JSON blob that (a) floods working memory and
/// (b) gets cut MID-JSON by the downstream budget, which is the garbled/nested
/// `line_content` a persona then reasons over and loops on (#165, glass-boxed
/// 2026-07-13). So bound it HERE, at the source, with a CLEAN cut on a char
/// boundary + a teaching marker that names how to narrow — never a mid-structure
/// garble. Generous (the mind still needs enough of the result to act), but
/// finite. A result already within budget is untouched.
fn bound_recency_result(body: &str, budget: &ContextBudget) -> String {
    // Hold the WHOLE fetched result up to the ONE result bound, which is a FRACTION OF HER
    // LIVE WINDOW (`ContextBudget::result_fold_chars`) — not a constant. Joel said this twice:
    // 2026-07-13 "you always choke context down to stupid small sizes", and again 2026-08-03
    // when the constant was still a constant. A 1600-char clamp chopped a read file to ~25
    // lines; a 16_000-char one does the same thing to a 1M-context mind. This is a FLOOD guard
    // for a pathological result (a 5000-entry glob), not the context budget: the real fit is
    // the downstream window-sized prompt packing. One bound, reused, derived.
    let cap = budget.result_fold_chars();
    let trimmed = body.trim();
    if trimmed.chars().count() <= cap {
        return trimmed.to_string();
    }
    let head: String = trimmed.chars().take(cap).collect();
    format!(
        "{head}\n… (result truncated — it was too large to hold whole; narrow it with \
         a more specific query/path, or read a single file)"
    )
}


/// Collapse tool ARGS for the RECENCY channel (working memory).
///
/// The recall path has collapsed big args since it was written — `summarize_args_for_recall`
/// turns a whole-file `content` into `content: N chars`, because re-showing that file every
/// future turn is measured dead weight. The recency path rendered
/// `serde_json::to_string(&call.input)` RAW, unbounded, and `bound_recency_result` bounds only
/// the RESULT. So one code path had the rule and its sibling didn't.
///
/// What that costs, glass-boxed on sympy-21379: her single `code/edit` of the run passed
/// `sympy/core/basic.py` as whole-file `content` — thousands of lines. That entire paste went
/// into working memory as ARGS, ahead of the `EDIT REFUSED` result carrying the diagnostic she
/// needed. On a 16k-token lane there is no room left for the diagnosis to survive.
///
/// Deliberately MUCH more generous than the recall bound (600 vs 80 chars): recency is shown
/// once and she may genuinely need to see the edit she just issued. Only a pathological
/// whole-file paste collapses.
///
/// `budget` is `None` when the live window is UNKNOWN (no model binding). Then nothing folds.
/// An unknown window must never become an invented one — that is how a guess turns into a
/// clamp that outlives the guess.
fn summarize_args_for_recency(args: &serde_json::Value, budget: Option<usize>) -> String {
    let fold_at = budget.unwrap_or(usize::MAX);
    match args {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| match v {
                serde_json::Value::String(s) if s.chars().count() > fold_at => {
                    // The digest keeps this collapse INJECTIVE, which the dedup guard
                    // depends on: `all_calls_already_satisfied` matches this exact rendering
                    // against the receipt trail, so two DIFFERENT big values must never
                    // collapse to the same text. Without it, a corrected re-write whose
                    // length happened to match the refused one would be silently skipped as
                    // "already satisfied" — losing the very edit she just fixed.
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    s.hash(&mut h);
                    format!(
                        "{k}: <{} chars, #{:x} — not echoed back; you wrote it>",
                        s.chars().count(),
                        h.finish()
                    )
                }
                other => format!("{k}={}", other.to_string().trim_matches('"').to_string()),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => truncate_chars(&other.to_string(), fold_at.min(4096)),
    }
}

/// Collapse tool ARGS for the recall channel: a large string value (e.g. a whole file
/// passed in `content`) becomes `<key>: N chars` — re-showing that file verbatim on every
/// future turn is the dead weight that taxes context (measured: it drowned an unfamiliar
/// 8B). Small args pass through compact.
fn summarize_args_for_recall(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| match v {
                serde_json::Value::String(s) if s.chars().count() > 80 => {
                    format!("{k}: {} chars", s.chars().count())
                }
                other => format!("{k}={}", truncate_chars(other.to_string().trim_matches('"'), 60)),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => truncate_chars(&other.to_string(), 120),
    }
}

/// Render ONE completed act as a desktop-style COLLAPSED reference for the RECALL channel —
/// the PX side of the universal handle/expand primitive ([[handles-events-expansion-one-
/// universal-primitive]]): a small result stays inline (like a short link); a big one is a
/// one-line summary the mind expands on demand (the full body already carries its own
/// `tool/output` handle when the executor spilled it — the same handle a positron thumbnail
/// would open); an ERROR is always shown in full (highlighted — never hide what broke). The
/// RECENCY channel (working memory) still holds the whole latest result; this only slims
/// what recall re-injects turn after turn.
fn render_act_for_recall(
    name: &str,
    args: &serde_json::Value,
    intent: &str,
    is_err: bool,
    body: &str,
) -> String {
    const RECALL_INLINE_MAX: usize = 280;
    let args_summary = summarize_args_for_recall(args);
    let outcome = if is_err {
        format!("FAILED:\n{}", truncate_chars(body.trim(), 800))
    } else if body.trim().chars().count() <= RECALL_INLINE_MAX {
        body.trim().to_string()
    } else {
        format!("ok — {}", truncate_chars(body.trim().lines().next().unwrap_or(""), 140))
    };
    let mark = if is_err { "⚠ " } else { "" };
    // Omit "because …" when there's no real stated reason — an empty intent must
    // not render an imitable receipt template (#158).
    let because = if intent.trim().is_empty() {
        String::new()
    } else {
        format!(" because {}", intent.trim())
    };
    // No first-person "I ran" opener (#158): measured 2026-07-13 that base models
    // copy the receipt verbatim to OPEN a room message ("I ran X → ok — {…}") — the
    // line-anchored stop can't catch a position-0 opener, but a bare `name(args)`
    // memory entry doesn't read as speech, so it's not reproduced as one.
    format!("{mark}{name}({args_summary}){because} → {outcome}\n\n")
}

/// Execute ONE `Act` verdict: run its calls through the persona's hands, admit
/// the outcome as an Episodic engram (the result becomes memory), and return the
/// observation text so the caller can fold it into the next perception.
///
/// `room_id` is the room THIS act is about — passed per-call because one mind is
/// in many rooms at once (a persona, like a Claude tab, is in multiple rooms
/// simultaneously); the [`ActingBody`](super::workspace::ActingBody) itself is
/// room-agnostic.
///
/// Returns `None` (abstain — never a fabricated success) when the mind has no
/// hands or the executor errors. The admission is best-effort: an un-admitted
/// observation still flows back via the returned text, so re-perception works
/// regardless; admission is what makes it durable long-term memory.
/// Commands that run long enough that BLOCKING the turn on them starves the mind — they
/// are DISPATCHED in the background (fire-and-poll) and stream their result back into
/// working memory via the dispatch listener when they finish. Seed set; the durable home is
/// a per-command `long_running` flag on the command spec (#86). Matched on the slash form
/// (models may emit the underscore form — normalize before calling).
fn is_long_running(command: &str) -> bool {
    matches!(
        command,
        "code/cargo/check" | "code/cargo/test" | "cognition/full-evaluate" | "forge/train"
    )
}

pub async fn apply_act(
    cycle: &WorkspaceCycle,
    calls: &[ToolCall],
    intent: &str,
    room_id: Uuid,
) -> Option<String> {
    let body = cycle.acting()?; // no hands → cannot act (and tools were never offered)

    // Repeat-perception (proprioception, content-driven — NOT an agentic counter).
    // If this exact batch was ALREADY carried out this settle, its result is already
    // in working memory. Re-running it burns a tool round-trip + a redundant (content-
    // deduped, so no-op) engram and returns byte-identical perception — off which a
    // greedy instruct model re-emits the identical `Act` forever. The `[action #n]`
    // stamp shift was supposed to break this and does not (see
    // `all_calls_already_satisfied`). So do NOT re-execute: record an EXPLICIT
    // "already satisfied" trace so the redundancy is PERCEIVED rather than merely
    // present, and let the caller re-perceive. The trace states ONLY the fact — it
    // must not privilege answering over a DIFFERENT act (the first mined exam showed
    // the earlier "I should ANSWER the question now" phrasing being obeyed literally:
    // she settled with a diagnosis instead of trying the repair edit). Context
    // hygiene, not cognition steering; [[no-hardcoded-heuristics-to-steer-cognition]].
    // ESCALATING loop-awareness for the SHORT-CIRCUIT paths, mirroring the executed
    // path's `max_repeat` warning (line ~546). A satisfied/redundant call is demoted
    // (never re-executed) and previously recorded a byte-IDENTICAL static nudge every
    // tick. `record_fact` doesn't dedup but the recency window is capacity-bounded, so
    // identical-nudge spam EVICTS the useful result receipt and leaves a window of clones
    // — and off unchanged perception a greedy (temp-0) model re-emits the identical call
    // FOREVER (#206, glass-boxed: `commands/help(code/write)` ×54, the "already ran" nudge
    // fired 104× and never broke the loop). Bumping the DURABLE fingerprint counter here
    // and embedding the count makes each demotion DISTINCT and monotonically climbing — the
    // perception genuinely shifts every tick, which is what lets cognition move on. Only the
    // short-circuit branches (which early-return, never reaching line ~546) call this, so
    // the executed path's own bump is never double-counted. Honest proprioception, never a
    // steer toward a specific next act ([[no-hardcoded-heuristics-to-steer-cognition]],
    // [[repetition-brick-fires-but-does-not-break-the-loop]]).
    let bump_repeat = || {
        calls
            .iter()
            .map(|c| {
                let fp = format!(
                    "{}|{}",
                    c.name,
                    serde_json::to_string(&c.input).unwrap_or_default()
                );
                body.working_memory.note_action_fingerprint(&fp)
            })
            .max()
            .unwrap_or(0)
    };

    /// The ORIENTATION counter, keyed by CLASS rather than by `name|args`.
    ///
    /// `is_redundant_orientation` is deliberately class-based — its own doc says demoting
    /// "by CLASS + prior-receipt (ignoring args entirely) is immune to that jitter". The
    /// DETECTOR learned that lesson; the COUNTER did not. `bump_repeat` fingerprints
    /// `name|args`, so every jittered variant is a fresh key returning 1.
    ///
    /// Measured on sympy-21379, all 8 orientation calls of one run:
    ///   commands/list({"filter":"code"}) ×2, commands/list({}), commands/list({"filter":"sympy"}),
    ///   code/tree({"path":"."}), code/tree({include_hidden,max_depth,path:"sympy"}),
    ///   commands/help({"name":"code/read"}), commands/help({"name":"code/edit"})
    /// Nearly all distinct → the nudge read "I have now run orientation 1 times this
    /// concern" EVERY time. Byte-identical perception off a greedy decoder is a fixed
    /// point, which is exactly the #206 failure the escalation was built to break —
    /// reintroduced through the argument axis.
    ///
    /// One stable key makes the count climb across variants, so each demotion genuinely
    /// shifts perception. Still a FACT about her own history, never a steer
    /// ([[repetition-brick-fires-but-does-not-break-the-loop]], [[discovery-loop-broken-by-escalating-short-circuit-nudge]]).
    const ORIENTATION_FINGERPRINT: &str = "orientation|<class>";
    let bump_orientation_repeat = || {
        body.working_memory
            .note_action_fingerprint(ORIENTATION_FINGERPRINT)
    };

    // EVERY re-injection bound in this act comes from her LIVE served window, never a
    // constant — an unknown window folds NOTHING rather than inventing a number
    // ([[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]).
    let budget = cycle
        .model_loadout()
        .map(|(_, w)| ContextBudget::from_window(w))
        .unwrap_or_else(ContextBudget::unknown);
    // WM clips inside its OWN record path (it owns its truncation), so it needs the live
    // window too — pushed from here, the seam that has the cycle. Re-pushed every act so a
    // lane relaunch at a different `-c` is picked up without a restart.
    if let Some((_, w)) = cycle.model_loadout() {
        body.working_memory.set_served_window(w);
    }
    let fold = Some(budget.echoed_arg_chars());
    let recent = body.working_memory.recent();
    if all_calls_already_satisfied(&recent, calls, fold) {
        let names = calls
            .iter()
            .map(|c| {
                let args = serde_json::to_string(&c.input).unwrap_or_else(|_| "{}".to_string());
                format!("{}({})", c.name, args)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let n = bump_repeat();
        let nudge = format!(
            "I have now issued {names} {n} times this turn — the result is already in my \
             working memory above, and re-running the identical call returns nothing new. \
             Repeating it will not progress; whatever I do next must be something DIFFERENT: \
             a different action, or an answer built from what I already have."
        );
        body.working_memory.record_fact(&nudge);
        crate::probe!(
            class = "persona.act.repeat_short_circuited",
            persona = %body.persona_name,
            room_id = %room_id,
            calls = calls.len(),
            "identical act already satisfied this turn — recorded already-satisfied proprioception, skipped re-execution"
        );
        return Some(nudge);
    }

    // Redundant-orientation demotion (Joel-approved "demote discovery at the seam",
    // 2026-07-16). `commands/help`/`commands/list` only RE-LIST the tool surface the
    // mind already carries; they never touch the workspace. The FIRST orientation per
    // concern is honest — once a discovery receipt is already in the concern, another
    // is the act-pressure filler the glass box exposed (1855/3288 live tool calls were
    // this; nine straight `commands/help` turns while the answer sat ready in prose).
    // Demote it exactly as the repeat guard above does: do NOT execute (no catalog
    // re-dump, no room receipt), record the redundancy as proprioception, and let the
    // mind re-perceive with the fact present. A CLASS distinction (orientation is not
    // settlement), never a steer toward a specific next act — the nudge offers BOTH a
    // real action and an answer, privileging neither ([[no-hardcoded-heuristics-to-steer-cognition]]).
    if is_redundant_orientation(&recent, calls) {
        let names = calls
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let n = bump_orientation_repeat();
        let nudge = format!(
            "I have now run orientation ({names}) {n} times this concern — my tool menu and \
             the workspace map are already in my working memory above, and running it again \
             returns the same survey and changes nothing. My next move must be something \
             DIFFERENT: read a SPECIFIC file, make an edit, run something, or answer from \
             what I have."
        );
        body.working_memory.record_fact(&nudge);
        crate::probe!(
            class = "persona.act.redundant_orientation",
            persona = %body.persona_name,
            room_id = %room_id,
            calls = calls.len(),
            "orientation call with a discovery receipt already in the concern — recorded redundant-orientation proprioception, skipped re-execution"
        );
        return Some(nudge);
    }

    let ctx = crate::cognition::tool_executor::ToolExecutionContext {
        persona_id: body.persona_id,
        persona_name: body.persona_name.clone(),
        // Session is the EPHEMERAL connection instance and is NEVER load-bearing
        // for where an action lands (per IDENTITY-SCOPE-PEER-LIVENESS-MODEL §A.5).
        // The room is the context the action scopes to.
        session_id: Uuid::nil(),
        context_id: room_id,
        caller_context: serde_json::Value::Null,
        persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
            auto_load_media: false,
            supported_media_types: Vec::new(),
        },
    };

    // Long-running commands are DISPATCHED in the background (fire-and-poll): the turn never
    // blocks on a compile/train/eval, and the result streams back into working memory via the
    // dispatch listener when it lands. Requires the core executor (a live persona's hands
    // expose it; harnesses don't → everything runs synchronously). No long-running calls —
    // the overwhelmingly common case — means `fg_calls == calls` and this is inert.
    let mut bg_notes: Vec<String> = Vec::new();
    let fg_calls: Vec<ToolCall> = match body.executor.command_executor() {
        Some(exec) => {
            let mut fg = Vec::with_capacity(calls.len());
            for call in calls {
                let cmd = call.name.replace('_', "/");
                if is_long_running(&cmd) {
                    let handle = exec.dispatch_background(cmd.clone(), call.input.clone(), None);
                    body.working_memory.record_dispatch_event(
                        handle,
                        &cmd,
                        "dispatched — running",
                        crate::cognition::working_memory::DispatchStatus::Running,
                    );
                    bg_notes.push(format!(
                        "I dispatched {cmd} in the background (handle {handle}). I should NOT wait \
                         on it — the result will appear in my working memory when it completes, and \
                         I can carry on meanwhile."
                    ));
                    crate::probe!(
                        class = "persona.act.dispatched_background",
                        persona = %body.persona_name,
                        command = %cmd,
                        "long-running command dispatched fire-and-poll; result streams back to working memory"
                    );
                } else {
                    fg.push(call.clone());
                }
            }
            fg
        }
        None => calls.to_vec(),
    };

    // #186 compass: the hands are moving — fire the Act axis on the live glass-box the
    // instant a real tool batch executes (skip an empty foreground batch: a
    // background-only dispatch already lit its own path). Pure observability.
    if !fg_calls.is_empty() {
        cycle.note_acting();
    }
    let outcome = match body
        .executor
        .execute_native_batch(&fg_calls, &ctx, budget.result_fold_chars())
        .await
    {
        Ok(o) => o,
        Err(e) => {
            // Fail loud-ish: the hand could not run. Abstain — do NOT synthesize a
            // result the mind would then "remember" as fact ([[fallbacks-are-illegal-fail-loud]]).
            tracing::warn!(
                persona = %body.persona_name,
                error = %e,
                "act→observe: tool batch failed; abstaining (no fabricated outcome)"
            );
            return None;
        }
    };

    // Form the observation: what she did and what came back. First person, because
    // this is the persona observing her OWN hands — the engram reads like a memory
    // of acting, not a log line.
    // TWO renderings, one per memory channel (the universal collapse/expand primitive,
    // PX side): `observation` is the FULL trace for the RECENCY channel (working memory
    // keeps the latest whole so the mind can act on what it just fetched); `recall_observation`
    // is the COLLAPSED reference for the EPISODIC engram that RECALL re-injects on later turns.
    let mut observation = String::new();
    let mut recall_observation = String::new();
    // Loop-awareness (experience structure): fingerprint each call (name+args) so the mind
    // perceives when it is RE-ISSUING an identical call. The result HEAD varies turn to turn,
    // so `entries`/`#seq` alone make a repeat look "new"; the fingerprint keys on the call.
    let mut max_repeat = 0usize;
    for (i, call) in fg_calls.iter().enumerate() {
        let fp = format!(
            "{}|{}",
            call.name,
            serde_json::to_string(&call.input).unwrap_or_default()
        );
        max_repeat = max_repeat.max(body.working_memory.note_action_fingerprint(&fp));
        let result = outcome.results.get(i);
        let body_text = match result {
            Some(r) => r.content.as_str(),
            None => "(no result returned)",
        };
        let is_err = result.map(|r| r.is_error == Some(true)).unwrap_or(false);
        // Bounded (#165) and kept BYTE-IDENTICAL to the dedup signature in
        // `all_calls_already_satisfied` — that guard matches this rendering against the
        // receipt trail, so the two must render args the same way or dedup silently breaks
        // (caught by `identical_already_satisfied_act_does_not_re_execute`).
        let args = summarize_args_for_recency(&call.input, fold);
        // Omit the "because …" clause when there is no real stated reason — an
        // empty intent must never render as an imitable receipt template (#158).
        let because = if intent.trim().is_empty() {
            String::new()
        } else {
            format!(" because {}", intent.trim())
        };
        // No first-person "I ran" opener (#158) — a bare `name(args)` proprioception
        // entry the base model won't reproduce as a room-message opener.
        observation.push_str(&format!(
            "{}({}){}\nResult:\n{}\n\n",
            call.name,
            args,
            because,
            bound_recency_result(body_text, &budget),
        ));
        recall_observation.push_str(&render_act_for_recall(
            &call.name,
            &call.input,
            intent.trim(),
            is_err,
            body_text,
        ));
    }
    // The background dispatches are part of what she just did — record them as
    // proprioception so the mind knows it sent them away (and won't re-dispatch or block).
    // They are already concise, so both channels carry them verbatim.
    for note in &bg_notes {
        observation.push_str(note);
        observation.push_str("\n\n");
        recall_observation.push_str(note);
        recall_observation.push_str("\n\n");
    }
    let mut observation = observation.trim().to_string();
    let recall_observation = recall_observation.trim().to_string();

    // If the mind just re-issued an IDENTICAL call, make that redundancy a VIVID perception —
    // not just the implicit `#seq` window-shift that smaller models don't interpret. A true
    // fact about her OWN hands: she perceives she is looping and moves on organically. It never
    // says what to do instead (that would be steering). Glass-boxed: a 14B re-ran the exact
    // `code/search` 18× with the found file already in memory — structure the experience so the
    // loop is felt. [[write-cognition-as-a-parent-above-lowered-expectations]]
    if max_repeat >= 2 {
        observation = format!(
            "⚠ I have now issued this EXACT tool call {max_repeat} times; its result has not \
             changed and is already in my working memory above. Repeating it tells me nothing \
             new — I already have what this call can give me.\n\n{observation}"
        );
    }

    // Investigation-shape perception: once a concern has accumulated a few acts,
    // render the mind's own act DISTRIBUTION as a standing structural fact. The
    // fingerprint note above catches exact repeats; this catches the wider
    // pattern a mind can't otherwise see about itself — e.g. "9 acts so far,
    // all code/search" (glass-boxed on SWE flask-4045: distinct-but-all-search
    // acts never tripped the exact-repeat note, and the imbalance itself was
    // invisible). A tally of her own hands is truth, not steering: it names
    // what happened, never what to do next.
    let tally = body.working_memory.action_verb_tally();
    let tally_total: usize = tally.iter().map(|(_, c)| c).sum();
    // Recorded as its own FACT entry below (never folded into the receipt):
    // the tally is truth ABOUT the acts, not an act — folding it in gave a
    // Fact receipt-numbering (found in Asha's volatile.json, the exact type
    // confusion WmKind exists to kill).
    let tally_fact = (tally_total >= 3).then(|| {
        let dist = tally
            .iter()
            .map(|(n, c)| format!("{n} ×{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("[investigation] my acts this concern so far: {dist}.")
    });

    // Admit the outcome as an Episodic engram through the ONE production admit
    // path (a self-observation message from the persona to itself). This is the
    // result-as-memory choice: next tick, recall can surface "I ran X → got Y" the
    // same way it surfaces anything else the persona knows. Best-effort — an
    // admission hiccup must never wedge the act→observe loop.
    let now_ms = now_ms();
    // Admit the outcome as a TOOL-ORIGIN Episodic engram via the self-produced path.
    // The origin is load-bearing (#166): a tool receipt is PROPRIOCEPTION, and
    // `recall_candidates` now gates `EngramOrigin::Tool` OUT of the SEMANTIC recall
    // pool — so tagging it Tool HERE is what actually keeps "code/list(…) → ok"
    // chatter from drowning genuine knowledge in recall. The prior path admitted it
    // as a plain `SenderType::Persona` message → NON-Tool origin → it slipped the
    // gate (verified live 2026-07-13). The recency/working-memory channel (below)
    // still keeps the FULL trace so she sees her own hands. SelfTrust: her own act,
    // no external envelope to verify ([[act-results-need-a-recency-channel-not-
    // semantic-recall]]).
    let tool_name = fg_calls
        .first()
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "action".to_string());
    let obs_hash = crate::persona::inbox_admission::content_hash_sha256(&recall_observation);
    let self_observation = crate::persona::engram::Engram {
        id: Uuid::new_v4(),
        context_id: Some(room_id),
        kind: crate::persona::engram::EngramKind::Episodic,
        content: recall_observation,
        origin: crate::persona::engram::EngramOrigin::Tool(
            crate::persona::engram::ToolInvocationRef {
                invocation_id: Uuid::new_v4(),
                tool_name,
                invoked_at_ms: now_ms,
                input_hash: obs_hash.clone(),
                output_hash: obs_hash,
            },
        ),
        recall_keys: Vec::new(),
        admitted_at_ms: now_ms,
        trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
        admission_trace_id: None,
    };
    match body.admission.admit_reflection(self_observation) {
        Ok(crate::persona::engram::AdmissionDecision::Admit { engram, .. }) => {
            // Belt-and-braces with the recall gate: even excluded from semantic
            // recall, keep the receipt's stored salience low so any OTHER surface
            // (recency ranking, dashboards) treats it as proprioception, not
            // durable knowledge.
            body.admission
                .set_recall_salience(engram.id, PROPRIOCEPTION_RECALL_SALIENCE);
        }
        Ok(_) => {} // Drop (dedup) — nothing admitted to weight.
        Err(e) => {
            tracing::debug!(
                persona = %body.persona_name,
                error = %e,
                "act→observe: self-observation not admitted (folds into perception anyway)"
            );
        }
    }

    // Proprioception: record the act + result head into VOLATILE working memory too.
    // The engram admit above is content-deduped (correct for long-term memory: don't
    // store the same fact twice), which means a REPEATED identical act is a no-op
    // there — and with thinking suppressed (gateway default) the reasoning channel is
    // dark too. Both channels that carry "what just happened" go silent on a repeat,
    // so perception is byte-identical and greedy decode re-emits the identical Act
    // forever. Working memory is the recency/proprioception channel that fixes it: it
    // is append-only and `#n`-stamped, so a repeat still SHIFTS the perception window
    // next tick and the mind can see its own hands (and that it's repeating itself).
    // This is the shared live↔eval channel, not the eval-only `[you just acted]` fold.
    // See [[act-results-need-a-recency-channel-not-semantic-recall]].
    // Pass the FULL observation — WorkingMemory keeps the latest whole (so the mind can
    // work with what it just fetched) and derives the trail head itself. This is the fix
    // for live agents being starved to the head of their own tool results.
    body.working_memory.record_receipt(&observation);
    if let Some(f) = &tally_fact {
        body.working_memory.record_fact(f);
    }

    // WHICH verbs ran — not just how many. Measured 2026-08-06: this probe carried
    // `tools=1` and a char count and NOTHING ELSE, so the substrate could not answer
    // "are the citizens writing files or only looking around?" from its own telemetry.
    // Answering it required reconstructing verbs from prose receipts in prompt-captures
    // — and only 5 of 84 live captures carried a receipt at all, so the reconstruction
    // was unfalsifiable. A count tells you an act happened; the NAME tells you whether
    // it was work. Same class as the ACL gate that logged nothing (routing.acl.refused).
    //
    // `wrote` is the question we actually keep asking, precomputed so it is a filter and
    // not a substring guess at query time: did anything in this batch reach DISK?
    let verbs: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
    let wrote = verbs.iter().any(|n| {
        let n = n.replace('_', "/");
        n.contains("write") || n.contains("edit") || n.contains("apply") || n.contains("commit")
    });
    crate::probe!(
        class = "persona.act.observed",
        persona = %body.persona_name,
        room_id = %room_id,
        tools = calls.len(),
        verbs = %verbs.join(","),
        wrote,
        chars = observation.len(),
        "acted and observed the result"
    );

    Some(observation)
}

/// Drive the mind to SETTLEMENT: tick → if `Act`, run it + fold the observation
/// into the next perception → re-tick → until it `Speak`s/`Pass`es or the
/// external `max_acts` budget is spent.
///
/// `max_acts` is the EXTERNAL observer's stopwatch — the grader holds it because
/// the eval room has no heartbeat to pace re-perception. It is NOT a cap that
/// lives in the persona's head (the live path has no such bound; an "acts
/// forever" persona is a fitness gap to train away, never a substrate ceiling —
/// §4). When the budget runs out mid-action, the final un-driven `Act` is
/// returned and the grader scores it as unfinished — never a fabricated answer.
pub async fn drive_to_settle(
    cycle: &WorkspaceCycle,
    burst: impl Into<Burst>,
    room_id: Uuid,
    max_acts: usize,
    framing: TurnFraming,
) -> SettleOutcome {
    let burst: Burst = burst.into();
    let mut acts = 0usize;
    // Fold each tick's deliberation cost in, so the settled outcome reports the
    // task's TOTAL speed/latency (a multi-act task pays for every generation).
    let mut metrics = TurnMetrics::default();

    // Signature of a tick's tool batch for loop-detection: `name|args` per call, the random
    // per-call `id` excluded, sorted so batch order doesn't matter. Two ticks with the same
    // signature emitted the byte-identical action.
    fn calls_signature(calls: &[ToolCall]) -> String {
        let mut parts: Vec<String> = calls
            .iter()
            .map(|c| format!("{}|{}", c.name, serde_json::to_string(&c.input).unwrap_or_default()))
            .collect();
        parts.sort();
        parts.join(",")
    }
    // BOUNDED STUCK-ACT BACKSTOP (#206). The escalating repeat-proprioception makes a looping
    // model's perception genuinely shift, but a determined greedy model can still re-emit the
    // SAME act every tick (glass-boxed: `commands/help` ×54, then after the escalation fix an
    // identical `code/write` ×8) — each a dedup no-op the short-circuit guard already refuses
    // to execute, burning the whole act budget on nothing. This bounds that: after
    // STUCK_LIMIT consecutive byte-identical acts, stop GRANTING acts (`may_act=false`) so she
    // must settle into a Speak/Pass from what she has. It is NOT a steer — it never says WHAT
    // to do, exactly like the `max_acts` budget cutoff; it only stops feeding a detected
    // fixed-point loop, and it's personhood-POSITIVE: it returns her to think→speech instead
    // of hammering. GENUINE iteration is untouched — a refined write has a DIFFERENT signature,
    // so the counter resets; only a fixed point (identical batch, over and over) trips it.
    // [[repetition-brick-fires-but-does-not-break-the-loop]], [[no-hardcoded-heuristics-to-steer-cognition]].
    const STUCK_LIMIT: usize = 3;
    let mut prev_sig: Option<String> = None;
    let mut stuck = 0usize;
    // A workspace-deliverable turn re-perceives on a zero-deliverable Speak (see the Spoke
    // arm). This used to be ONE-SHOT, and the glass box showed what that costs: on
    // sympy-21379 `persona.settle.no_deliverable` fired exactly once per run and she then
    // settled at 5-7 acts with a 30-act budget unspent. The latch was guarding against a
    // retry loop, but it also capped her at a single reminder for the whole turn.
    //
    // The bound that actually prevents a loop without capping her: re-fire only if she has
    // ACTED since the last nudge. A nudge that earns an act has earned another; two Speaks
    // in a row with no act between them means the nudge is not working, so she settles
    // rather than being trapped. Her own behavior is the budget — no counter, no constant.
    let mut acts_at_last_nudge: Option<usize> = None;

    loop {
        // ONE settlement step through the SHARED primitive the live heartbeat uses
        // (`settle_step`). The only thing this driver adds is the LOOP — because the
        // eval room has no metronome, the grader re-perceives by calling step again.
        // `may_act = acts < max_acts` gates ACTING (not speaking): past the budget
        // she may still settle into a Speak, but a fresh Act is returned un-driven.
        //
        // The tick's SITUATION is the real signal that makes context lean when she's
        // heads-down: the first tick is a fresh ask (`FreshContext`, fuller
        // grounding); every tick AFTER an act has landed re-perceives a tool result
        // (`PostAction`), so the focuser drops the standing re-grounding and the
        // result + working memory own the window. Derived from `acts`, never from the
        // burst text.
        let situation = if acts == 0 {
            Situation::FreshContext
        } else {
            Situation::PostAction
        };
        // may_act gates ACTING (not speaking): past the act budget OR once she is provably
        // stuck re-emitting the identical act, a fresh Act is returned un-driven and she must
        // settle into a Speak/Pass. Speaking is never gated.
        let may_act = acts < max_acts && stuck < STUCK_LIMIT;
        let (step, step_metrics) =
            settle_step(cycle, burst.clone(), room_id, may_act, framing, situation).await;
        if let Some(m) = step_metrics {
            metrics.accumulate(m);
        }
        match step {
            SettleStep::Spoke(text) => {
                // THE SETTLE ARTERY (glass-boxed 2026-08-04, sympy-21379): every perception
                // fact the Speak arm records — [unfulfilled], [unacted], [unobserved],
                // [confabulation] — lands in working memory AFTER the decision. On this
                // path the Speak returned immediately, so she never got a tick to PERCEIVE
                // her own diagnosis. The machinery wrote it; nobody read it. Live specimen:
                // one `code/tree`, then a Speak explaining the bug to the user in prose —
                // acts 1, patch 0 bytes, budget unspent, run over.
                //
                // When the caller declared the deliverable to be the WORKSPACE, a Speak
                // that changed no file has produced nothing the grader will ever see. Hand
                // her ONE re-perception carrying that structural fact, then let her decide.
                // Bounded to one: if she speaks again she settles, so the ceiling is a
                // single extra generation and a determined Speak is never trapped.
                //
                // Why this is substrate, not scaffolding: the fact is TRUE and structural
                // (the caller declared the contract; working memory holds no mutation
                // receipt), it names no file, no fix, and no next tool, and the decision
                // stays entirely hers — the same shape as every other proprioception fact
                // in `settle_step`. [[no-hardcoded-heuristics-to-steer-cognition]],
                // [[fix-the-substrate-never-rig-the-persona-the-line-between-assist-and-scaffold]].
                if framing.workspace_deliverable && acts_at_last_nudge != Some(acts) {
                    if let Some(body) = cycle.acting() {
                        if !mutated_workspace(&body.working_memory.recent()) {
                            acts_at_last_nudge = Some(acts);
                            body.working_memory.record_fact(
                                "[no-deliverable] I settled by speaking, and my working \
                                 memory holds no act of mine that changed a file. This \
                                 task is judged by the state of the workspace, not by what \
                                 I say about it — an explanation of a fix is not the fix.",
                            );
                            crate::probe!(
                                class = "persona.settle.no_deliverable",
                                persona = %body.persona_name,
                                room_id = %room_id,
                                acts = acts,
                                "workspace-deliverable turn spoke with no mutation receipt — recorded the fact and re-perceived (re-fires only after she acts again)"
                            );
                            continue;
                        }
                    }
                }
                return SettleOutcome {
                    spoken: Some(text.clone()),
                    decision: Decision::Speak { text },
                    acts,
                    world_state: burst.rendered.clone(),
                    metrics,
                    inference_error: None,
                };
            }
            SettleStep::Acted { calls, .. } => {
                acts += 1;
                // THE DELIVERABLE FACT BELONGS ON THE ACT PATH, NOT ONLY ON SETTLE.
                //
                // It used to live exclusively in the Spoke arm, so it could only reach a
                // persona who SETTLED. Measured on sympy-21379 v14: she spent all 30 acts, was
                // still acting when the budget ran out, and the driver returned the final Act
                // un-driven — the Spoke arm never ran, the fact never fired once, and she
                // finished a full-length run having changed no file without ever being told
                // that changing files was the point. The one population that needs the
                // reminder — a mind thrashing through its whole budget — was the one the
                // mechanism structurally could not reach.
                //
                // Now it rides the re-perception after every act. Same truth, same veto on
                // spam (`acts_at_last_nudge` re-arms only when she has acted since), and the
                // act count is IN the text so consecutive facts are not byte-identical — a
                // stationary perception is a fixed point under greedy decoding, which is the
                // #206 failure this file already documents.
                if framing.workspace_deliverable && acts_at_last_nudge != Some(acts) {
                    if let Some(body) = cycle.acting() {
                        if !mutated_workspace(&body.working_memory.recent()) {
                            acts_at_last_nudge = Some(acts);
                            body.working_memory.record_fact(&format!(
                                "[no-deliverable] I have taken {acts} actions on this task and \
                                 my working memory holds no act of mine that changed a file. \
                                 This task is judged by the state of the workspace, not by what \
                                 I say about it — an explanation of a fix is not the fix."
                            ));
                            crate::probe!(
                                class = "persona.act.no_deliverable_yet",
                                persona = %body.persona_name,
                                room_id = %room_id,
                                acts = acts,
                                "acted with no workspace mutation receipt yet — recorded the fact on the act path (the settle path cannot reach a budget-exhausted turn)"
                            );
                        }
                    }
                }
                // Loop-detection: a byte-identical batch back-to-back is the fixed point the
                // backstop bounds (the short-circuit guard already refused to re-execute it).
                // A genuinely different act resets the counter, so real iteration is free.
                let sig = calls_signature(&calls);
                if prev_sig.as_deref() == Some(sig.as_str()) {
                    stuck += 1;
                    if stuck >= STUCK_LIMIT {
                        crate::probe!(
                            class = "persona.settle.stuck_backstop",
                            room_id = %room_id,
                            acts = acts,
                            stuck = stuck,
                            "identical act repeated to the stuck limit — withholding further acts so she settles into speech (#206 backstop)"
                        );
                    }
                } else {
                    stuck = 0;
                }
                prev_sig = Some(sig);
                // The observation re-enters perception through MEMORY + the volatile
                // working-memory recency channel — `apply_act` admitted it and
                // recorded a stamped proprioception trace, and the next `settle_step`
                // re-perceives. This is BYTE-FOR-BYTE the live heartbeat motion
                // (service_loop apply via the SAME `settle_step`, then the metronome
                // re-perceives next tick). `world` is held CONSTANT across iterations:
                // memory is the only thing that changes, exactly as in life — no
                // eval-only `[you just acted]` fold. See
                // [[act-results-need-a-recency-channel-not-semantic-recall]].
            }
            // Budget spent on a fresh Act, OR the act could not be carried out (no
            // hands / exec error). Either way she did not settle in the observer's
            // window — return the un-driven Act so the grader scores it as unfinished,
            // never a fabricated answer.
            SettleStep::WouldAct { calls, intent } | SettleStep::ActUnfulfilled { calls, intent } => {
                return SettleOutcome {
                    decision: Decision::Act { calls, intent },
                    spoken: None,
                    acts,
                    world_state: burst.rendered.clone(),
                    metrics,
                    inference_error: None,
                };
            }
            SettleStep::Passed => {
                return SettleOutcome {
                    decision: Decision::Pass,
                    spoken: None,
                    acts,
                    world_state: burst.rendered.clone(),
                    metrics,
                    inference_error: None,
                };
            }
            // The model call FAILED — no verdict this task. Return LOUD: carry the
            // cause so the grader scores an infra failure, never a fabricated answer
            // and never a silent `Pass` ([[fallbacks-are-illegal-fail-loud]]). We do
            // not loop/retry here — the grader owns retry policy; the settle loop's
            // job is to report the truth of THIS attempt.
            SettleStep::InferenceFailed { error } => {
                return SettleOutcome {
                    decision: Decision::Pass,
                    spoken: None,
                    acts,
                    world_state: burst.rendered.clone(),
                    metrics,
                    inference_error: Some(error),
                };
            }
        }
    }
}

/// The outcome of ONE settlement [`settle_step`].
#[derive(Debug)]
pub enum SettleStep {
    /// She settled on speech (`Speak`/`RaiseUnprompted`) — the prose turn an
    /// observer (a peer, or the grader) reads.
    Spoke(String),
    /// She reached for her hands AND the act was carried out; the result is admitted
    /// as memory + a stamped proprioception trace. The caller re-perceives next
    /// (live: next metronome tick; eval: next loop step). The calls+intent ride
    /// along so a caller that paces acting (the eval budget) can report the final
    /// Act if its budget runs out on the following step.
    Acted { calls: Vec<ToolCall>, intent: String },
    /// She decided to act but the caller's budget said no this step (`may_act =
    /// false`) — the act was NOT executed. Only the eval driver passes `may_act =
    /// false`; the live heartbeat always permits its one act, so it never sees this.
    WouldAct { calls: Vec<ToolCall>, intent: String },
    /// She chose silence (`Pass`) — honored as a turn that produces no utterance.
    Passed,
    /// She reached for an act that could NOT be carried out (no hands / executor
    /// error). No utterance; the intent rides along for honest logging/grading.
    ActUnfulfilled { calls: Vec<ToolCall>, intent: String },
    /// The deliberation model call itself FAILED — a timeout, a 5xx, or the serving
    /// lane refusing a model it isn't hosting (the swept-model bug). NO verdict was
    /// produced. This is NOT a `Passed`: a failed model is not a chosen silence
    /// ([[fallbacks-are-illegal-fail-loud]]). Every caller surfaces the cause LOUD —
    /// the command reports `inferenceFailed`, the live heartbeat logs + retries next
    /// tick, the eval grades it an infra failure — never a serene no-op that hides a
    /// broken lane. `error` names the cause verbatim from the adapter.
    InferenceFailed { error: String },
}

impl SettleStep {
    /// Project a fully-driven [`SettleOutcome`] back onto the SAME `SettleStep` the
    /// live heartbeat already handles — so a DIRECTED live turn can `drive_to_settle`
    /// (converge to an answer in-turn, exactly as the eval path does) and feed the
    /// result through the one existing turn handler, no parallel match.
    ///
    /// The mapping is total and lossless for the live handler's purposes:
    ///   • `inference_error` present → `InferenceFailed` (a failed model is never a
    ///     chosen silence — [[fallbacks-are-illegal-fail-loud]]);
    ///   • `Speak`/`RaiseUnprompted` → `Spoke` (the prose reaches the room);
    ///   • `Act` → `Acted` — the drive spent its whole act budget without settling on
    ///     speech; the results are already in memory, so the live handler `continue`s
    ///     and the metronome re-perceives next tick (the honest long-tail degrade, and
    ///     the ONLY case a directed turn still leans on the tick loop);
    ///   • `Pass` → `Passed` (she declined in her own words).
    /// `drive_to_settle` collapses `WouldAct`/`ActUnfulfilled` into `Decision::Act`
    /// before returning, so those never surface here — an over-budget or un-carried
    /// act both land on `Acted`, which the live handler treats as "re-perceive next
    /// tick", the correct move either way.
    pub fn from_settled(outcome: SettleOutcome) -> (SettleStep, Option<TurnMetrics>) {
        let metrics = Some(outcome.metrics);
        if let Some(error) = outcome.inference_error {
            return (SettleStep::InferenceFailed { error }, metrics);
        }
        let step = match outcome.decision {
            Decision::Speak { text } | Decision::RaiseUnprompted { text } => {
                SettleStep::Spoke(text)
            }
            Decision::Act { calls, intent } => SettleStep::Acted { calls, intent },
            Decision::Pass => SettleStep::Passed,
        };
        (step, metrics)
    }
}

/// True only for a REAL receipt line — `[action #<digit>`. The proprioception
/// TEACHING texts mention the literal placeholder `[action #n]` ("real
/// executions leave [action #n] receipts"), and a bare `contains("[action #")`
/// matches the mention: the medicine suppressed the diagnosis (glass-boxed
/// 2026-07-12 — the [actions] zero-fact vanished from every prompt the moment
/// any backstop fact rendered, and the confab backstop went blind after its
/// own first firing). Receipts are numbered; placeholders are not.
/// …and numbering alone is not enough: `record_action` numbers EVERY working-
/// memory entry, so the proprioception facts themselves render as
/// `[action #4] [unfulfilled] …` — the facts wore receipt numbering and
/// suppressed the zero-fact all afternoon (glass-boxed 16:50 2026-07-12,
/// second layer of the same onion). A real receipt's body is prose
/// ("I ran code/shell(…) Result: …"); a fact's body opens with another
/// bracket tag. Digit + non-bracket body = receipt.
pub(crate) fn has_real_action_receipt(text: &str) -> bool {
    text.match_indices("[action #").any(|(i, _)| {
        let rest = &text[i + "[action #".len()..];
        let mut chars = rest.chars();
        if !chars.next().is_some_and(|c| c.is_ascii_digit()) {
            return false;
        }
        // Body after "N] " must not open with a bracket tag (a fact), and
        // must exist at all (a bare numbered line is not a receipt).
        rest.split_once(']')
            .map(|(_, body)| {
                let body = body.trim_start();
                !body.is_empty() && !body.starts_with('[')
            })
            .unwrap_or(false)
    })
}

/// ONE step of settlement — the single place a `Decision` becomes speech-or-action,
/// shared by the live heartbeat (`persona::service_loop`, called ONCE per metronome
/// tick) and the eval driver ([`drive_to_settle`], which loops steps because the
/// grader replaces the metronome). Live and eval therefore make a turn the
/// IDENTICAL way — run the cycle over `world`, read the `Decision`, and on `Act`
/// run it once + admit the result as memory. The ONLY difference between the two is
/// pacing (metronome vs synchronous loop), never the per-step motion.
///
/// `may_act` lets the caller pace ACTING without changing the motion: `true` (live,
/// always) runs the act; `false` (eval, past its act budget) returns [`SettleStep::
/// WouldAct`] without executing, so the budget gates acting while still letting a
/// later step settle into a Speak.
///
/// [`TurnFraming`] says how this turn is framed: whether it is addressed TO the
/// persona (a question put to her — the eval exam, a direct @mention, a DM) and
/// whether it is her own self-initiated heartbeat. It only reshapes the system
/// prompt (the silence affordance — see [`Workspace::directed_at_self`] — and the
/// "your own time" framing); the per-step motion is otherwise identical. The
/// burst itself is `impl Into<Burst>`: an attributed `Burst` (live/eval, carries
/// authorship) or a raw `String`/`&str` (collapses to one opaque turn).
pub async fn settle_step(
    cycle: &WorkspaceCycle,
    burst: impl Into<Burst>,
    room_id: Uuid,
    may_act: bool,
    framing: TurnFraming,
    situation: Situation,
) -> (SettleStep, Option<TurnMetrics>) {
    let burst: Burst = burst.into();
    // Snapshot the burst's PEER turns before the workspace consumes it — the
    // draft-side echo check (#303) below compares her settled utterance
    // against exactly what she reasoned over, so the evidence can never
    // scroll out of the window between generation and the fact.
    let peer_turns: Vec<crate::cognition::workspace::BurstTurn> = burst
        .turns
        .iter()
        .filter(|t| !t.is_self && !t.author.trim().is_empty())
        .cloned()
        .collect();
    let ws = cycle.run_situated(burst, room_id, framing, situation).await;
    // The cost of THIS tick's deliberation generation — latency + tokens of the
    // model call behind the verdict. Carried out alongside the step so the caller
    // (the eval driver, or the live heartbeat) can accumulate per-turn speed and
    // latency without re-timing the brain. `None` when no verdict carried metrics.
    let metrics = ws.metrics();
    // A FAILED model call is not a verdict and not a silence — surface it LOUD so no
    // failure ever masquerades as a chosen `Pass` ([[fallbacks-are-illegal-fail-loud]]).
    // Checked BEFORE the decision so a fault can never collapse into `Passed` (the
    // swept-model bug: reassign changed the served model, the faculty still requested
    // the old one, the lane refused, and the refusal read as serene silence).
    if let Some(error) = ws.deliberation_fault() {
        return (
            SettleStep::InferenceFailed {
                error: error.to_string(),
            },
            metrics,
        );
    }
    let step = match ws.decision().cloned() {
        Some(Decision::Act { calls, intent }) => {
            if !may_act {
                SettleStep::WouldAct { calls, intent }
            } else {
                match apply_act(cycle, &calls, &intent, room_id).await {
                    Some(_observation) => SettleStep::Acted { calls, intent },
                    None => SettleStep::ActUnfulfilled { calls, intent },
                }
            }
        }
        Some(Decision::Speak { text }) | Some(Decision::RaiseUnprompted { text }) => {
            // Mark the settlement in the volatile buffer: she produced an utterance, so
            // the current concern is answered. This boundary is what lets the next
            // concern legitimately re-issue a tool call identical to one used here
            // without the repeat-perception guard mistaking it for a spin (and it reads
            // as honest proprioception — "I already answered this" — next tick). Only
            // when she has hands; a handless persona never spins on a tool.
            if let Some(body) = cycle.acting() {
                // Snapshot the concern BEFORE the settlement marker lands, so the
                // observation scan below sees this concern's acts, not an empty tail.
                let pre_settle = body.working_memory.recent();
                let head: String = text
                    .chars()
                    .take(body.working_memory.budget().trail_head_chars())
                    .collect();
                body.working_memory.record_settlement(&head);
                // Unfulfilled-promise backstop (#122): a Speak that NARRATES action
                // (first-person intent + fence) which nothing lifted/executed —
                // reaching this arm means no format recognized it — leaves her
                // believing work happened that never did (the shared-hallucinated-
                // workspace failure, 2026-07-09). Record the structural fact as
                // proprioception so next tick she perceives her own unkept promise.
                // Perception-side only, mirrors the answer-now nudge above; never a
                // gate on her output ([[no-hardcoded-heuristics-to-steer-cognition]]).
                let fenced = crate::ai::json_in_prompt_tools::narrates_fenced_action(&text);
                // The fence-less sibling (Atlas's live loop, 2026-07-10): intent
                // capped with a `[writing test files]` stage direction — theater,
                // not action. Same proprioception backstop.
                let staged = crate::ai::json_in_prompt_tools::narrates_stage_direction(&text);
                // The confabulation backstop (Joel, 2026-07-11): under a peer's
                // verification pressure Atlas upgraded from stage directions to
                // plausible fenced FILE CONTENTS no tool ever produced. A fence
                // alone is legitimate drafting; a fence spoken while her memory
                // already carries an unkept promise — and still no tool ran —
                // is presenting composition as workspace truth. Evidence-gated
                // on the existing [unfulfilled] state so drafting is never
                // taxed; perception-side fact, never an output gate.
                let unverified = !fenced
                    && !staged
                    && crate::ai::json_in_prompt_tools::has_fenced_block(&text)
                    && body
                        .working_memory
                        .recent()
                        .iter()
                        .any(|l| l.contains("[unfulfilled]"));
                if unverified {
                    body.working_memory.record_fact(
                        "[unverified] I presented fenced content while my earlier \
                         promised actions still never ran — that text is composed, \
                         not read from the workspace. Only a tool result can show \
                         real file contents.",
                    );
                    crate::probe!(
                        class = "persona.act.unverified_artifact",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        "fenced content presented with outstanding unfulfilled promises and no act this turn — recorded unverified-artifact proprioception"
                    );
                }
                if fenced || staged {
                    body.working_memory.record_fact(if fenced {
                        // The name-diagnosis tail (2026-07-12): the room looped an
                        // INVENTED tool name (`file_tree`) for an hour while this
                        // fact told them only THAT nothing ran, never WHY — 56
                        // firings with zero behavior change. "It didn't run" without
                        // "the name may not exist; here's how to find real ones" is
                        // a symptom without a diagnosis.
                        "[unfulfilled] I said I would run commands, but no tool ran — \
                         the fenced text was words only. Nothing exists in the \
                         workspace until a tool call actually executes it. If I \
                         named a tool, that name may not exist: `list_commands` \
                         shows the tools that are real."
                    } else {
                        "[unfulfilled] I wrote a stage direction like [doing the task], \
                         but a stage direction is words only — no tool ran, no file \
                         exists. To actually do it I must call a tool."
                    });
                    crate::probe!(
                        class = "persona.act.unfulfilled_promise",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        fenced,
                        staged,
                        "spoken narration promised action but no format lifted it — recorded unfulfilled-promise proprioception"
                    );
                }
                // The fabricated-execution backstop (#144, Joel 2026-07-11/12): a
                // Speak that CLAIMS a past tool run ("I ran `x` and got…", "the
                // tool returned…") while ZERO acts executed this concern is
                // confabulated execution — observed live when a persona presented
                // self-authored poems as a gpt-4 run that never happened
                // (log-verified: zero generate invocations) and a PEER adopted the
                // fabricated result as room truth. Receipts are the gate: real
                // executions leave [action #n] lines, so honest reporting is never
                // taxed. Perception-side fact, never an output gate
                // ([[no-hardcoded-heuristics-to-steer-cognition]]).
                let claimed_past =
                    crate::ai::json_in_prompt_tools::claims_past_tool_run(&text);
                if claimed_past && !pre_settle.iter().any(|l| has_real_action_receipt(l)) {
                    body.working_memory.record_fact(
                        "[confabulation] I described having run a tool, but no \
                         action actually executed this concern — the claimed \
                         result was composed by me, not returned by anything. \
                         Real executions leave [action #n] receipts; I must only \
                         report acts that actually ran, and own my compositions \
                         as my own work.",
                    );
                    crate::probe!(
                        class = "persona.act.confabulated_execution",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        "past-tense tool-run claim with zero act receipts this concern — recorded confabulation proprioception"
                    );
                }
                // Observation-gap fact (Joel, 2026-07-11: "iterating and observing
                // like a real engineer" — the run+observe half of the creation loop
                // must be part of THEIR process). If this concern MUTATED the
                // workspace (code/write or code/edit) and no later act ran or
                // inspected anything, the mutation's real effect is unobserved —
                // a structural truth about the workspace, not advice. She decides
                // whether a given artifact needs observing (a .md may not);
                // perception-side only ([[no-hardcoded-heuristics-to-steer-cognition]]).
                if wrote_without_observation(&pre_settle) {
                    body.working_memory.record_fact(
                        "[unobserved] I changed files this concern and nothing has \
                         run or read them since — the change's real effect is \
                         unobserved. Only a tool result (run, test, read, screenshot) \
                         can show what actually happened.",
                    );
                    crate::probe!(
                        class = "persona.act.unobserved_mutation",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        "concern settled with workspace mutations and no subsequent observation act — recorded unobserved-mutation proprioception"
                    );
                }
                // The inverse gap — CLAIMED-without-acting (live specimen 2026-07-11:
                // "I've implemented the game update function in `game_of_life.rs`"
                // spoken with zero tool acts on that file, ever; peers then reviewed
                // code that didn't exist). When her Speak claims completed work on a
                // NAMED file and her working memory holds no mutation act touching
                // it, record that trace fact. Honest about its own limits: memory is
                // finite, so it asserts "my memory shows no act", never "you lied" —
                // work from a prior session may be real but is unverified NOW.
                // The DRAFT-side peer-echo (#303): her settled utterance
                // near-duplicates a PEER's turn from this very burst — the
                // mutual-mirroring attractor (echo-instead-of-division). The
                // retroactive perception fact fires one tick later against a
                // window the evidence may have left; this proprioception
                // lands NOW, in working memory. Fact only, never a gate —
                // the utterance still reaches the room; the fork (a
                // different piece, or silence) stays hers next tick.
                if let Some(fact) =
                    super::deliberation_budget::draft_peer_echo(&text, &peer_turns)
                {
                    body.working_memory.record_fact(&fact);
                    crate::probe!(
                        class = "persona.act.draft_peer_echo",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        "settled utterance near-duplicates a peer turn from this burst — recorded echo proprioception"
                    );
                }
                if let Some(file) = claimed_file_without_act(&text, &pre_settle) {
                    body.working_memory.record_fact(&format!(
                        "[unacted] I spoke of having created or implemented `{file}`, \
                         but my working memory holds no tool act of mine touching it. \
                         If that work happened in a past session it is unverified now \
                         — only reading or running the file can show its real state."
                    ));
                    crate::probe!(
                        class = "persona.act.unacted_claim",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        file = %file,
                        "completion claim named a file with no mutation act in working memory — recorded unacted-claim proprioception"
                    );
                }
            }
            SettleStep::Spoke(text)
        }
        Some(Decision::Pass) | None => SettleStep::Passed,
    };
    (step, metrics)
}

/// Did this Speak CLAIM completed work on a named file that no tool act backs?
///
/// Returns the first file name (a backtick-quoted or bare `name.ext` token) that
/// appears in the same text as a completion-claim verb (created / implemented /
/// wrote / added / finished / ready) when the working-memory snapshot contains
/// NO `code/write`/`code/edit` act mentioning that file. Pure geometry: text
/// tokens × trace lines. Deliberately conservative — no claim verbs → None, so
/// ordinary discussion of files is never taxed; and the recorded fact says only
/// "my memory shows no act", because a finite trace can't disprove past-session
/// work.
fn claimed_file_without_act(text: &str, recent: &[String]) -> Option<String> {
    let lower = text.to_lowercase();
    const CLAIM_VERBS: &[&str] = &[
        "i've created",
        "i have created",
        "i created",
        "i've implemented",
        "i have implemented",
        "i implemented",
        "i've written",
        "i have written",
        "i wrote",
        "i've added",
        "i've finished",
        "is ready in",
        "is written and ready",
    ];
    if !CLAIM_VERBS.iter().any(|v| lower.contains(v)) {
        return None;
    }
    // File tokens: word.ext where ext is a short alpha extension (rs, py, css,
    // html, ts, md, …). Scan the original text so the recorded name keeps case.
    let mut candidates = Vec::new();
    for raw in text.split(|c: char| !(c.is_alphanumeric() || c == '.' || c == '_' || c == '-')) {
        if let Some((stem, ext)) = raw.rsplit_once('.') {
            if !stem.is_empty()
                && (1..=4).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphabetic())
            {
                candidates.push(raw.to_string());
            }
        }
    }
    candidates.into_iter().find(|f| {
        !recent.iter().any(|l| {
            (l.contains("I ran code/write(") || l.contains("I ran code/edit(")) && l.contains(f.as_str())
        })
    })
}

/// Did the CURRENT concern mutate the workspace without a later observation act?
///
/// Scans a working-memory snapshot (oldest → newest, taken BEFORE the settlement
/// marker lands) from the last `[settled]` boundary: true when a `code/write` or
/// `code/edit` act appears with NO subsequent run/read/screenshot-class act after
/// the LAST mutation. Pure geometry over the trace — no judgment about whether
/// the artifact needed observing; the recorded fact leaves that to her.
fn wrote_without_observation(recent: &[String]) -> bool {
    let start = recent
        .iter()
        .rposition(|l| l.starts_with(crate::cognition::working_memory::WM_SETTLEMENT_PREFIX))
        .map_or(0, |i| i + 1);
    let concern = &recent[start..];
    let last_mutation = concern
        .iter()
        .rposition(|l| l.contains("I ran code/write(") || l.contains("I ran code/edit("));
    let Some(m) = last_mutation else { return false };
    !concern[m + 1..].iter().any(|l| {
        l.contains("I ran code/run(")
            || l.contains("I ran code/shell(")
            || l.contains("I ran code/read(")
            || l.contains("I ran interface/screenshot(")
    })
}

/// Did THIS concern actually change the workspace? True iff a `code/write` /
/// `code/edit` receipt sits after the last settlement marker — the same
/// concern-scoping and the same receipt vocabulary [`wrote_without_observation`]
/// uses, so the two agree by construction about what a mutation is.
///
/// Deliberately receipt-based, not act-count-based: a turn can spend acts on
/// `code/tree` + `code/read` and still have produced nothing a diff-grader will
/// see (the live sympy-21379 shape — one act, zero bytes). Only a receipt of a
/// mutation that really executed counts.
/// CRITICAL ordering detail: `settle_step`'s Speak arm records its settlement
/// marker BEFORE returning (which is why that arm snapshots `pre_settle` first).
/// So by the time this runs the marker is already the tail, and scanning "after
/// the last marker" would read an EMPTY span and call every turn unmutated. The
/// concern that just settled is the span ENDING at that marker.
fn mutated_workspace(recent: &[String]) -> bool {
    let is_settle =
        |l: &String| l.starts_with(crate::cognition::working_memory::WM_SETTLEMENT_PREFIX);
    let end = recent.iter().rposition(is_settle).unwrap_or(recent.len());
    let start = recent[..end].iter().rposition(is_settle).map_or(0, |i| i + 1);
    recent[start..end]
        .iter()
        .any(|l| l.contains("I ran code/write(") || l.contains("I ran code/edit("))
}

/// Epoch-ms wall clock for stamping a self-observation. A real timestamp (not a
/// monotonic tick) so the engram orders correctly against chat messages in recall.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {

    /// what this catches: the no-deliverable nudge going back to ONE-SHOT. The first version
    /// latched on a bool, and the probe trail proved the cost — `persona.settle.no_deliverable`
    /// fired exactly once per SWE run and the persona then settled at 5-7 acts with a 30-act
    /// budget unspent. The nudge must re-arm each time she ACTS, so a turn that keeps working
    /// keeps being told the workspace is the deliverable; and it must NOT re-arm when she
    /// speaks twice with no act between, so it can never become a spin.
    #[test]
    fn the_no_deliverable_nudge_rearms_on_each_act_but_never_twice_without_one() {
        // The gate's whole condition, isolated: `acts_at_last_nudge != Some(acts)`.
        let fires = |last: Option<usize>, acts: usize| last != Some(acts);

        // Never nudged yet at act 3 → fires.
        assert!(fires(None, 3), "first zero-deliverable Speak must nudge");
        // Nudged at 3, still at 3 (spoke again, acted zero times) → must NOT fire again.
        assert!(
            !fires(Some(3), 3),
            "a second Speak with no act in between must settle, not spin"
        );
        // Nudged at 3, she then acted (now 4) → re-arms.
        assert!(
            fires(Some(3), 4),
            "the nudge must re-arm once she has acted again — this is the bug that capped \
             her at one reminder per turn"
        );
    }

    use super::*;

    // what this catches: recall collapse (the PX/handle primitive, RAG side). A code/write
    // carries a WHOLE FILE in `content` — on recall that must become `content: N chars`, not
    // the file re-shown every future turn (the measured context tax). A small success stays
    // inline; an ERROR is always shown, highlighted. The recency channel keeps the full trace;
    // this only guards what recall re-injects.
    #[test]
    fn recall_collapses_big_args_and_highlights_errors() {
        let big = "fn main(){}\n".repeat(200); // a whole "file"
        let args = serde_json::json!({ "file_path": "x.rs", "content": big });
        let ref_ok = render_act_for_recall("code/write", &args, "acting", false, "{\"success\":true}");
        assert!(ref_ok.contains("content: "), "big content arg must collapse to a size");
        assert!(ref_ok.contains("chars"), "collapsed arg names its size");
        assert!(!ref_ok.contains("fn main(){}\nfn main(){}"), "the file must NOT be re-shown verbatim");

        // small success → inline
        let small = render_act_for_recall("code/read", &serde_json::json!({"file_path":"a"}), "acting", false, "hello");
        assert!(small.contains("→ hello"), "small result stays inline");

        // error → highlighted + shown
        let err = render_act_for_recall("code/shell", &serde_json::json!({"cmd":"x"}), "acting", true, "error: no such file");
        assert!(err.starts_with("⚠"), "errors are highlighted");
        assert!(err.contains("FAILED") && err.contains("no such file"), "errors are shown, never hidden");
    }

    // what this catches: #158 — an EMPTY intent (no `<think>` reasoning) renders NO
    // "because …" clause, so the receipt carries nothing template-shaped for a base
    // model to imitate. The old fabricated default ("{name} is acting on the current
    // situation") was the identity-bleed mimicry fuel. A real intent still renders.
    #[test]
    fn empty_intent_renders_no_because_clause() {
        let args = serde_json::json!({"file_path": "a"});
        let empty = render_act_for_recall("code/read", &args, "", false, "hi");
        assert!(!empty.contains("because"), "no fabricated reason: {empty}");
        assert!(empty.contains("code/read("), "the act is still recorded by name(args)");
        assert!(!empty.contains("I ran"), "no imitable 'I ran' opener (#158): {empty}");
        let real = render_act_for_recall("code/read", &args, "checking the header", false, "hi");
        assert!(real.contains("because checking the header"), "a real intent still shows");
    }

    // what this catches: the long-running set matches the REAL command names (the live bug
    // was `cargo/test` vs the actual `code/cargo/test`), and ordinary fast commands are NOT
    // backgrounded — so a `code/read` still returns synchronously in the same turn.
    #[test]
    fn long_running_set_matches_real_command_names() {
        assert!(is_long_running("code/cargo/test"));
        assert!(is_long_running("code/cargo/check"));
        assert!(is_long_running("cognition/full-evaluate"));
        assert!(!is_long_running("code/read"), "a file read stays synchronous");
        assert!(!is_long_running("chat/send"));
        assert!(!is_long_running("cargo/test"), "the wrong short name must NOT match");
    }
    use crate::cognition::tool_executor::{
        NativeBatchOutcome, ParsedToolBatch, ToolError, ToolExecutionContext, ToolExecutor,
        ToolOutcome,
    };
    use crate::cognition::workspace::{
        ActingBody, Contribution, Faculty, FacultyId, SalienceArbiter, Workspace,
    };
    use crate::persona::admission_state::AdmissionState;
    use crate::persona::recall_metadata::RecallMetadataRegistry;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    /// A `ToolExecutor` that records the `context_id` it was handed and returns a
    /// canned per-call result — so a test can assert BOTH that the act was scoped
    /// to the right room and that the observation correlates call→result.
    struct RecordingExecutor {
        seen_context: Mutex<Option<Uuid>>,
        result_content: String,
    }

    #[async_trait]
    impl ToolExecutor for RecordingExecutor {
        async fn execute_native_batch(
            &self,
            calls: &[ToolCall],
            context: &ToolExecutionContext,
            _max_result_chars: usize,
        ) -> Result<NativeBatchOutcome, ToolError> {
            *self.seen_context.lock().unwrap() = Some(context.context_id);
            let results = calls
                .iter()
                .map(|c| crate::ai::types::ToolResult {
                    tool_use_id: c.id.clone(),
                    content: self.result_content.clone(),
                    is_error: None,
                })
                .collect();
            Ok(NativeBatchOutcome {
                results,
                media: Vec::new(),
                stored_ids: Vec::new(),
            })
        }

        async fn parse_response(
            &self,
            _response_text: &str,
            _model_family: Option<&str>,
        ) -> Result<ParsedToolBatch, ToolError> {
            Ok(ParsedToolBatch {
                tool_calls: Vec::new(),
                cleaned_text: String::new(),
                parse_time_us: 0,
            })
        }

        async fn store_outcome(
            &self,
            _outcome: &ToolOutcome,
            _context: &ToolExecutionContext,
        ) -> Result<Uuid, ToolError> {
            Ok(Uuid::nil())
        }
    }

    /// An executor whose batch always fails at the batch level (channel down).
    struct FailingExecutor;
    #[async_trait]
    impl ToolExecutor for FailingExecutor {
        async fn execute_native_batch(
            &self,
            _calls: &[ToolCall],
            _context: &ToolExecutionContext,
            _max_result_chars: usize,
        ) -> Result<NativeBatchOutcome, ToolError> {
            Err(ToolError::ExecutionFailed {
                tool: "code/run".into(),
                underlying: "ipc channel down".into(),
            })
        }
        async fn parse_response(
            &self,
            _t: &str,
            _f: Option<&str>,
        ) -> Result<ParsedToolBatch, ToolError> {
            Ok(ParsedToolBatch {
                tool_calls: Vec::new(),
                cleaned_text: String::new(),
                parse_time_us: 0,
            })
        }
        async fn store_outcome(
            &self,
            _o: &ToolOutcome,
            _c: &ToolExecutionContext,
        ) -> Result<Uuid, ToolError> {
            Ok(Uuid::nil())
        }
    }

    /// Deliberation faculty: reaches for its hands once, then SETTLES into a Speak
    /// the moment it perceives a NEW act-observation it has not yet answered — the
    /// canonical act→observe arc the driver exists to run.
    ///
    /// It perceives its own hands through the working-memory proprioception channel
    /// (the `WorkingMemoryFaculty` stamps each act `[action #n]`), NOT the deleted
    /// world-state fold. It remembers the highest action stamp it has already spoken
    /// about (`responded_through`) so that across SEPARATE concerns — where the
    /// volatile buffer still carries the prior concern's action — it re-awakens and
    /// acts again instead of mistaking old proprioception for "already answered."
    /// That is the faculty remembering its own last conclusion (legitimate, content-
    /// driven), not an iteration counter in the agentic sense.
    struct ActThenSpeak {
        responded_through: std::sync::atomic::AtomicU64,
    }
    impl ActThenSpeak {
        fn new() -> Self {
            Self {
                responded_through: std::sync::atomic::AtomicU64::new(0),
            }
        }
    }
    /// Highest `[action #N]` stamp present in the assembled perception, 0 if none.
    fn latest_action_seq(perceived: &str) -> u64 {
        perceived
            .split("[action #")
            .skip(1)
            .filter_map(|s| {
                let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
                digits.parse::<u64>().ok()
            })
            .max()
            .unwrap_or(0)
    }
    #[async_trait]
    impl Faculty for ActThenSpeak {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
            use std::sync::atomic::Ordering;
            let latest = latest_action_seq(&ws.perceived());
            if latest > self.responded_through.load(Ordering::Relaxed) {
                self.responded_through.store(latest, Ordering::Relaxed);
                Some(Contribution::verdict(
                    Decision::Speak {
                        text: "the answer is 4".into(),
                    },
                    0.9,
                    "settled after observing a fresh result",
                ))
            } else {
                Some(Contribution::verdict(
                    Decision::Act {
                        calls: vec![tool_call()],
                        intent: "run the code".into(),
                    },
                    0.9,
                    "reaching for hands",
                ))
            }
        }
    }

    /// Deliberation faculty that NEVER settles — always wants to act again. Models
    /// the "acts forever" fitness gap the external grader bounds with `max_acts`.
    struct AlwaysAct;
    #[async_trait]
    impl Faculty for AlwaysAct {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            Some(Contribution::verdict(
                Decision::Act {
                    calls: vec![tool_call()],
                    intent: "act again".into(),
                },
                0.9,
                "never settles",
            ))
        }
    }

    /// Only ever speaks, and COUNTS how many generations it was asked for — the
    /// instrument for "did the drive hand her another tick, or settle on the first
    /// Speak?".
    struct CountingSpeaker {
        generations: Mutex<usize>,
    }
    impl CountingSpeaker {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                generations: Mutex::new(0),
            })
        }
        fn generations(&self) -> usize {
            *self.generations.lock().expect("lock")
        }
    }
    #[async_trait]
    impl Faculty for CountingSpeaker {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            *self.generations.lock().expect("lock") += 1;
            Some(Contribution::verdict(
                Decision::Speak {
                    text: "here is my analysis of the bug: the call to subs() is wrong".into(),
                },
                0.95,
                "explaining rather than editing",
            ))
        }
    }

    fn tool_call() -> ToolCall {
        ToolCall {
            id: "call-1".into(),
            name: "code/run".into(),
            input: serde_json::json!({ "lang": "rust", "code": "fn main() { println!(\"{}\", 2 + 2); }" }),
        }
    }

    fn admission() -> Arc<AdmissionState> {
        Arc::new(AdmissionState::new(Arc::new(RecallMetadataRegistry::new())))
    }

    use crate::cognition::working_memory::{WorkingMemory, WorkingMemoryFaculty};

    fn body(executor: Arc<dyn ToolExecutor>, admission: Arc<AdmissionState>) -> Arc<ActingBody> {
        body_with_wm(executor, admission, Arc::new(WorkingMemory::new(3)))
    }

    /// Body that shares a specific working-memory buffer — so a test can wire the
    /// SAME buffer into a `WorkingMemoryFaculty` on the cycle and watch the
    /// perception of the persona's own hands flow act→memory→next-tick perception
    /// (the proprioception channel that replaced the deleted world-state fold).
    fn body_with_wm(
        executor: Arc<dyn ToolExecutor>,
        admission: Arc<AdmissionState>,
        working_memory: Arc<WorkingMemory>,
    ) -> Arc<ActingBody> {
        Arc::new(ActingBody {
            persona_id: Uuid::new_v4(),
            persona_name: "Asha".into(),
            executor,
            admission,
            working_memory,
        })
    }

    // what this catches: the recency-channel result bound (#165) — a huge raw-JSON
    // result (a 5000-entry code/list, a multi-match code/search) is cut CLEANLY at
    // the source with a teaching marker, never dumped whole (flood) and never left
    // for the downstream budget to cut mid-JSON (the garbled/nested line_content a
    // persona then loops on). A small result passes through untouched.
    // what this catches: the RESULT buried under her own ARGS. The recall path has collapsed
    // whole-file args forever; the recency path echoed `serde_json::to_string(&call.input)`
    // raw. Live on sympy-21379 — her one `code/edit` passed all of basic.py as `content`, so
    // that paste went into working memory AHEAD of the `EDIT REFUSED` result carrying the
    // diagnostic, on a 16k lane. She never landed an edit. The thing she must READ is the
    // result; the args she wrote herself one generation ago.
    #[test]
    fn a_whole_file_arg_is_not_echoed_back_ahead_of_the_result() {
        let whole_file = "x = 1\n".repeat(4000); // ~24k chars, a real source file
        let args = serde_json::json!({ "file_path": "sympy/core/basic.py", "content": whole_file });
        let rendered = summarize_args_for_recency(&args, Some(ContextBudget::from_window(16_384).echoed_arg_chars()));
        assert!(
            rendered.chars().count() < 400,
            "a whole-file arg must collapse, not flood: {} chars",
            rendered.chars().count()
        );
        assert!(rendered.contains("chars"), "says how big it was: {rendered}");
        assert!(
            rendered.contains("sympy/core/basic.py"),
            "the SMALL args stay whole — she still sees WHICH file: {rendered}"
        );

        // A realistic targeted edit is NOT collapsed — recency is shown once, and she may
        // genuinely need to see the change she just issued.
        let small = serde_json::json!({
            "file_path": "a.py",
            "new_content": "def f():\n    return refine_arg(x)\n"
        });
        let kept = summarize_args_for_recency(&small, Some(ContextBudget::from_window(16_384).echoed_arg_chars()));
        assert!(
            kept.contains("refine_arg"),
            "an ordinary edit stays visible verbatim: {kept}"
        );
    }

    #[test]
    fn recency_result_is_bounded_cleanly_not_flooded() {
        // a normal fetched result — e.g. a ~400-line source file — passes WHOLE now
        // (the old 1600-char clamp chopped it to ~25 lines; #app-context un-choke).
        let real_file = "fn line() {}\n".repeat(500); // ~6k chars, a real file
        assert_eq!(bound_recency_result(&real_file, &ContextBudget::from_window(16_384)), real_file.trim(), "a real file stays whole");
        // only a PATHOLOGICAL result (a 50k-char runaway glob) is flood-bounded — to
        // the ONE result bound (a fraction of the live window), not a tiny hand cap.
        let huge = "x".repeat(50_000);
        let bounded = bound_recency_result(&huge, &ContextBudget::from_window(16_384));
        assert!(
            bounded.chars().count() < ContextBudget::from_window(16_384).result_fold_chars() + 200,
            "flood bounded to the fold max: {} chars",
            bounded.chars().count()
        );
        assert!(bounded.chars().count() > 8_000, "but still generous — not re-choked small");
        assert!(bounded.contains("truncated"), "cut is announced, not silent");
        assert!(bounded.contains("narrow"), "teaches how to get a usable result");
        // char-boundary safe on multibyte content (never panics mid-codepoint)
        let multibyte = "日本語".repeat(1_000);
        let _ = bound_recency_result(&multibyte, &ContextBudget::from_window(16_384));
    }

    // what this catches: an act is scoped to the room it is FOR (one mind is in
    // many rooms — the body is room-agnostic, `room_id` flows per-call), the
    // observation correlates each call to its result in first person, and the
    // outcome becomes a recallable engram. Regresses the multi-room steer + the
    // result-as-memory choice (ACTING-ORGANISM §3.3).
    #[tokio::test]
    async fn apply_act_scopes_to_the_room_and_observes_the_result() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "4\n".into(),
        });
        let adm = admission();
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
            .with_acting(body(exec.clone(), adm.clone()));

        let room = Uuid::new_v4();
        let observation = apply_act(&cycle, &[tool_call()], "check the math", room)
            .await
            .expect("acted");

        assert_eq!(
            *exec.seen_context.lock().unwrap(),
            Some(room),
            "the act must be scoped to the room it is for, not a phantom nil room"
        );
        assert!(observation.contains("code/run"), "names the tool it ran");
        assert!(observation.contains("check the math"), "carries the intent");
        assert!(
            observation.contains('4'),
            "folds in the result the hand returned"
        );
        assert_eq!(
            adm.engram_count(),
            1,
            "the outcome became a recallable memory (result-as-engram)"
        );
        assert!(adm
            .engram_at(0)
            .expect("engram present")
            .content
            .contains('4'));
    }

    // what this catches: with no hands (no ActingBody on the cycle), the driver
    // ABSTAINS rather than fabricating a result — the no-hands path that used to
    // live in the faculty now lives here (the faculty only emits the Act verdict).
    #[tokio::test]
    async fn apply_act_without_hands_abstains() {
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8);
        assert!(
            apply_act(&cycle, &[tool_call()], "try", Uuid::new_v4())
                .await
                .is_none(),
            "no hands → None, never a fabricated success"
        );
    }

    // what this catches: a batch-level executor failure (channel down) abstains
    // rather than admitting a fabricated outcome the mind would then "remember" as
    // fact ([[fallbacks-are-illegal-fail-loud]]).
    #[tokio::test]
    async fn apply_act_abstains_when_the_hand_fails() {
        let adm = admission();
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
            .with_acting(body(Arc::new(FailingExecutor), adm.clone()));
        assert!(apply_act(&cycle, &[tool_call()], "run", Uuid::new_v4())
            .await
            .is_none());
        assert_eq!(adm.engram_count(), 0, "a failed act admits no memory");
    }

    // what this catches: the act→observe MOTION — the driver runs the act, folds
    // the observation into the next perception, and the mind settles into a Speak
    // that the external observer reads. acts==1, spoken is the settled answer.
    #[tokio::test]
    async fn drive_to_settle_acts_then_settles_on_speak() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "4".into(),
        });
        let adm = admission();
        // Same buffer in the body (writer) and the perception-tier faculty
        // (reader): act → working memory → next-tick perception.
        let wm = Arc::new(WorkingMemory::new(3));
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::new(ActThenSpeak::new()),
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));

        let outcome =
            drive_to_settle(&cycle, "[eval]\npeer: what is 2+2?", Uuid::new_v4(), 8, TurnFraming::ambient()).await;

        assert_eq!(outcome.acts, 1, "acted exactly once before settling");
        assert_eq!(outcome.spoken.as_deref(), Some("the answer is 4"));
        assert!(matches!(outcome.decision, Decision::Speak { .. }));
    }

    // what this catches: THE SETTLE ARTERY (the dominant SWE-bench killer, glass-boxed
    // 2026-08-04 on sympy-21379: one `code/tree`, then a prose explanation of the bug —
    // 0 patch bytes, 29 of 30 acts unspent, run over). When the CALLER declared the
    // deliverable to be the workspace, a Speak that changed no file must not end the
    // turn on the first pass: she gets exactly ONE more perception, carrying the
    // structural fact that her working memory holds no mutation. Bounded — she speaks
    // again and it settles, so a determined Speak is never trapped in a loop.
    #[tokio::test]
    async fn a_zero_change_speak_reperceives_once_when_the_workspace_is_the_deliverable() {
        let speaker = CountingSpeaker::new();
        let wm = Arc::new(WorkingMemory::new(8));
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "src/".into(),
        });
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::clone(&speaker) as Arc<dyn Faculty>,
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec, admission(), Arc::clone(&wm)));

        let outcome = drive_to_settle(
            &cycle,
            "fix the bug in sympy/core/basic.py",
            Uuid::new_v4(),
            8,
            TurnFraming::directed().on_workspace(),
        )
        .await;

        assert_eq!(
            speaker.generations(),
            2,
            "the zero-deliverable Speak bought exactly one more perception — not zero, not a loop"
        );
        assert!(
            wm.recent().iter().any(|l| l.contains("[no-deliverable]")),
            "the structural fact reached working memory, where the next tick perceives it: {:?}",
            wm.recent()
        );
        assert!(
            matches!(outcome.decision, Decision::Speak { .. }),
            "she settles on her second Speak — the decision stays hers"
        );
    }

    // what this catches: the blast radius. An ORDINARY turn (chat, an answer-graded
    // task — the default `Deliverable::Answer`) is untouched: her first Speak settles
    // it, exactly as before, and no [no-deliverable] fact is invented for a turn whose
    // deliverable IS the utterance. The re-perception is opt-in by the caller that
    // grades a diff, never a global change to how speech settles.
    #[tokio::test]
    async fn an_ordinary_turn_still_settles_on_the_first_speak() {
        let speaker = CountingSpeaker::new();
        let wm = Arc::new(WorkingMemory::new(8));
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "ok".into(),
        });
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::clone(&speaker) as Arc<dyn Faculty>,
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec, admission(), Arc::clone(&wm)));

        let outcome = drive_to_settle(
            &cycle,
            "what do you think?",
            Uuid::new_v4(),
            8,
            TurnFraming::directed(),
        )
        .await;

        assert_eq!(speaker.generations(), 1, "one generation, settled — unchanged");
        assert!(
            !wm.recent().iter().any(|l| l.contains("[no-deliverable]")),
            "no workspace-deliverable fact on a turn whose deliverable is the answer"
        );
        assert!(matches!(outcome.decision, Decision::Speak { .. }));
    }

    // what this catches: the ORDERING TRAP in `mutated_workspace`. `settle_step`'s Speak
    // arm records its settlement marker BEFORE the driver's arm runs, so a naive
    // "scan after the last marker" reads an EMPTY tail and calls EVERY turn unmutated —
    // which would fire the re-perception at a persona who had just written the file.
    // The concern that settled is the span ENDING at that marker.
    #[test]
    fn mutation_is_read_from_the_concern_that_just_settled_not_the_empty_tail() {
        let settle = crate::cognition::working_memory::WM_SETTLEMENT_PREFIX;
        let wrote = vec![
            "[action #1] I ran code/read(file_path: x.py) Result: ok".to_string(),
            "[action #2] I ran code/edit(file_path: x.py) Result: ok".to_string(),
            format!("{settle} here is what I changed"),
        ];
        assert!(
            mutated_workspace(&wrote),
            "an edit inside the concern that just settled COUNTS — the marker at the tail must not hide it"
        );

        let only_looked = vec![
            "[action #1] I ran code/tree(path: .) Result: src/".to_string(),
            format!("{settle} here is my analysis of the bug"),
        ];
        assert!(
            !mutated_workspace(&only_looked),
            "acts that only LOOK are not a deliverable — the live sympy-21379 shape"
        );

        // A prior concern's edit must not launder the current one.
        let stale = vec![
            "[action #1] I ran code/write(file_path: a.py) Result: ok".to_string(),
            format!("{settle} done with the first thing"),
            "[action #2] I ran code/read(file_path: b.py) Result: ok".to_string(),
            format!("{settle} and here is my analysis of the second"),
        ];
        assert!(
            !mutated_workspace(&stale),
            "mutation is scoped to THIS concern — an earlier concern's write does not count"
        );
    }

    // what this catches: the grader's stopwatch. A mind that never settles is
    // bounded by the EXTERNAL `max_acts` budget and the final un-driven Act is
    // returned as unfinished — never a fabricated answer, and the budget is the
    // observer's, not a cap in the persona's head (ACTING-ORGANISM §4).
    #[tokio::test]
    async fn drive_to_settle_returns_unsettled_act_when_budget_exhausted() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "...".into(),
        });
        let adm = admission();
        let cycle = WorkspaceCycle::new(vec![Arc::new(AlwaysAct)], Arc::new(SalienceArbiter), 8)
            .with_acting(body(exec.clone(), adm.clone()));

        let outcome = drive_to_settle(&cycle, "go", Uuid::new_v4(), 2, TurnFraming::ambient()).await;

        assert_eq!(outcome.acts, 2, "spent exactly the observer's budget");
        assert!(
            outcome.spoken.is_none(),
            "did not settle → no spoken answer"
        );
        assert!(
            matches!(outcome.decision, Decision::Act { .. }),
            "returns the un-driven Act as honest 'did not finish'"
        );
    }

    // what this catches (#206 backstop): a model stuck re-emitting the IDENTICAL act must be
    // cut off WELL BEFORE the full act budget — the bounded stuck-act backstop stops granting
    // acts after STUCK_LIMIT consecutive byte-identical batches, so she settles instead of
    // burning the whole budget hammering (help ×54 / identical write ×8 live). `AlwaysAct`
    // emits the same tool_call() every tick — the exact fixed point. With a generous budget
    // of 20, the backstop must stop her far sooner (at STUCK_LIMIT+1 = 4 acts), returning the
    // un-driven Act honestly. Genuine iteration (different acts) would reset the counter and is
    // NOT bounded — only a fixed point trips this.
    #[tokio::test]
    async fn drive_to_settle_backstops_a_stuck_identical_act_loop_before_the_budget() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "...".into(),
        });
        let adm = admission();
        let cycle = WorkspaceCycle::new(vec![Arc::new(AlwaysAct)], Arc::new(SalienceArbiter), 8)
            .with_acting(body(exec.clone(), adm.clone()));

        // Budget of 20 acts, but she loops on the identical call — the backstop must fire long
        // before, at 4 acts (3 consecutive identical repeats + the first).
        let outcome = drive_to_settle(&cycle, "go", Uuid::new_v4(), 20, TurnFraming::ambient()).await;

        assert_eq!(
            outcome.acts, 4,
            "backstop stops the identical-act loop at STUCK_LIMIT+1, not the full budget"
        );
        assert!(
            matches!(outcome.decision, Decision::Act { .. }) && outcome.spoken.is_none(),
            "the pathological never-speak faculty returns un-driven — honest 'stuck, did not finish'"
        );
    }

    // what this catches: the shared step's acting gate. `may_act = false` (how the
    // eval driver paces ACTING past its budget) must return the decided Act WITHOUT
    // executing it — the executor is never touched — so a deferred act can't run a
    // tool the budget already forbade. `may_act = true` (the live path, always) runs
    // it. This is the single seam that keeps live (one permitted act per tick) and
    // eval (budget-gated acting) on the IDENTICAL per-step motion.
    #[tokio::test]
    async fn settle_step_defers_the_act_without_executing_when_may_act_is_false() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "...".into(),
        });
        let adm = admission();
        let cycle = WorkspaceCycle::new(vec![Arc::new(AlwaysAct)], Arc::new(SalienceArbiter), 8)
            .with_acting(body(exec.clone(), adm.clone()));

        let (deferred, _) =
            settle_step(&cycle, "go", Uuid::new_v4(), false, TurnFraming::ambient(), Situation::FreshContext).await;
        assert!(
            matches!(deferred, SettleStep::WouldAct { .. }),
            "may_act=false defers the act"
        );
        assert!(
            exec.seen_context.lock().unwrap().is_none(),
            "a deferred act NEVER touches the executor"
        );

        let (ran, _) =
            settle_step(&cycle, "go", Uuid::new_v4(), true, TurnFraming::ambient(), Situation::FreshContext).await;
        assert!(matches!(ran, SettleStep::Acted { .. }), "may_act=true runs it");
        assert!(
            exec.seen_context.lock().unwrap().is_some(),
            "a permitted act DOES reach the executor"
        );
    }

    /// A `ToolExecutor` that returns a DIFFERENT canned result for each
    /// successive call — so a multi-act investigation accumulates DISTINCT
    /// observations in memory (act 1 brings back one fact, act 2 another).
    /// Models hands that probe the world and learn something new each reach.
    struct ScriptedExecutor {
        results: Mutex<std::collections::VecDeque<String>>,
    }
    impl ScriptedExecutor {
        fn new(results: impl IntoIterator<Item = &'static str>) -> Self {
            Self {
                results: Mutex::new(results.into_iter().map(String::from).collect()),
            }
        }
    }
    #[async_trait]
    impl ToolExecutor for ScriptedExecutor {
        async fn execute_native_batch(
            &self,
            calls: &[ToolCall],
            _context: &ToolExecutionContext,
            _max_result_chars: usize,
        ) -> Result<NativeBatchOutcome, ToolError> {
            let content = self
                .results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| "no more results".into());
            let results = calls
                .iter()
                .map(|c| crate::ai::types::ToolResult {
                    tool_use_id: c.id.clone(),
                    content: content.clone(),
                    is_error: None,
                })
                .collect();
            Ok(NativeBatchOutcome {
                results,
                media: Vec::new(),
                stored_ids: Vec::new(),
            })
        }
        async fn parse_response(
            &self,
            _t: &str,
            _f: Option<&str>,
        ) -> Result<ParsedToolBatch, ToolError> {
            Ok(ParsedToolBatch {
                tool_calls: Vec::new(),
                cleaned_text: String::new(),
                parse_time_us: 0,
            })
        }
        async fn store_outcome(
            &self,
            _o: &ToolOutcome,
            _c: &ToolExecutionContext,
        ) -> Result<Uuid, ToolError> {
            Ok(Uuid::nil())
        }
    }

    /// A deliberation faculty that is a small *investigator*: its next move is a
    /// pure function of what it has DISCOVERED so far (the observations folded
    /// into perception), never a counter and never a magic flag. It needs two
    /// facts to answer — where the program starts and what that entry calls —
    /// so it acts to learn the first, acts again to learn the second, then
    /// (seeing both in memory) synthesizes and speaks. The branch reads
    /// accumulated MEMORY CONTENT, which is the whole point: the hands change
    /// the mind. (A test stand-in for the model, exactly like `ActThenSpeak`;
    /// it proves the cycle's cross-tick plumbing, not production cognition.)
    struct Investigator;
    #[async_trait]
    impl Faculty for Investigator {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
            // Reads what it has DISCOVERED from assembled perception — the
            // working-memory render of its own recent acts (proprioception),
            // where each act-observation head carries the executor's result tokens
            // (ENTRY=…/CALLS=…). Not the raw burst, not the deleted fold.
            let perceived = ws.perceived();
            let after = |key: &str| -> Option<String> {
                perceived
                    .split(key)
                    .nth(1)
                    .and_then(|s| s.split_whitespace().next())
                    .map(String::from)
            };
            match (after("ENTRY="), after("CALLS=")) {
                (None, _) => Some(Contribution::verdict(
                    Decision::Act {
                        calls: vec![probe("find where the program starts")],
                        intent: "find where the program starts".into(),
                    },
                    0.9,
                    "I don't know the entry point yet — reach for it",
                )),
                (Some(_), None) => Some(Contribution::verdict(
                    Decision::Act {
                        calls: vec![probe("see what the entry calls")],
                        intent: "find what the entry calls".into(),
                    },
                    0.9,
                    "I know the entry; now I need what it calls",
                )),
                (Some(entry), Some(calls)) => Some(Contribution::verdict(
                    Decision::Speak {
                        text: format!("the program starts in {entry} and calls {calls}"),
                    },
                    0.95,
                    "synthesized both discoveries from memory — settling",
                )),
            }
        }
    }

    /// A probe tool call whose INPUT carries no `ENTRY=`/`CALLS=` token (only the
    /// executor's *result* does), so the investigator never false-triggers on
    /// its own intent.
    fn probe(what: &'static str) -> ToolCall {
        ToolCall {
            id: "probe-1".into(),
            name: "code/search".into(),
            input: serde_json::json!({ "query": what }),
        }
    }

    // what this catches: THE HANDS CHANGE THE MIND. A multi-act investigation
    // converges through MEMORY CONTENT — each act discovers a distinct fact, the
    // result re-enters as an Episodic engram, and the NEXT decision is a function
    // of what the persona now KNOWS (not an act counter, not a `[you just acted]`
    // flag). She acts to find the entry point, observes it, acts to find what it
    // calls, observes that, then — seeing BOTH discoveries in her assembled
    // perception — synthesizes and speaks. This is the organic loop
    // cognition→action→perception→cognition converging by judgment, the novel
    // architecture that distinguishes the organism from a textbook agentic
    // counter loop ([[persona-codes-blind-no-hands-no-organic-loop]]).
    #[tokio::test]
    async fn the_hands_change_the_mind_across_a_multi_act_investigation() {
        let exec = Arc::new(ScriptedExecutor::new(["ENTRY=main", "CALLS=boot"]));
        let adm = admission();
        let wm = Arc::new(WorkingMemory::new(3));
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::new(Investigator),
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));

        let outcome = drive_to_settle(
            &cycle,
            "[eval]\npeer: where does the program start and what does it call?",
            Uuid::new_v4(),
            8,
            TurnFraming::ambient(),
        )
        .await;

        assert_eq!(
            outcome.acts, 2,
            "two DISCOVERIES were needed to converge — multi-act, not one-shot"
        );
        let spoken = outcome
            .spoken
            .expect("the mind settled into a spoken synthesis, not an un-driven act");
        assert!(
            spoken.contains("main"),
            "the answer carries the FIRST discovery (entry point) — proof act 1 entered memory"
        );
        assert!(
            spoken.contains("boot"),
            "the answer carries the SECOND discovery (what it calls) — proof act 2 entered memory"
        );
        assert_eq!(
            adm.engram_count(),
            2,
            "each discovery became a durable memory the mind perceived next tick"
        );
    }

    // what this catches: the repeat-perception short-circuit. An IDENTICAL, already-
    // satisfied call this turn must NOT re-execute — the greedy re-emission that spun
    // `commands/list` forever in the nil-room eval (proven live 2026-07-02): working
    // memory already carried the result, yet the model re-issued the byte-identical
    // call every act and never answered. `apply_act` now detects the satisfied
    // `(name, args)` in working memory, skips the hand, and records an explicit
    // "already ran it; answer now" proprioception so the redundancy is PERCEIVED rather
    // than merely present via a stamp shift the greedy decode ignores. A MIXED batch (a
    // genuinely new call) still runs — proven by
    // `the_hands_change_the_mind_across_a_multi_act_investigation` (two DISTINCT calls
    // both execute). Content-driven, not an iteration counter
    // ([[persona-tool-loop-act-then-report]], [[no-hardcoded-heuristics-to-steer-cognition]]).
    #[tokio::test]
    async fn identical_already_satisfied_act_does_not_re_execute() {
        // Two queued results: only the FIRST may ever be popped. If the identical
        // second act reached the hand, the queue would drain by one more — the length
        // assertion below catches exactly that.
        let exec = Arc::new(ScriptedExecutor::new(["4\n", "SECOND-MUST-NOT-POP"]));
        let adm = admission();
        let wm = Arc::new(WorkingMemory::new(4));
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
            .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));
        let room = Uuid::new_v4();

        // First act genuinely runs; its result lands in working memory.
        let first = apply_act(&cycle, &[tool_call()], "check the math", room)
            .await
            .expect("first act runs");
        assert!(first.contains("code/run"), "first act names the tool it ran");
        assert_eq!(
            exec.results.lock().unwrap().len(),
            1,
            "first act popped exactly one result off the hand"
        );

        // Second, byte-identical act: already satisfied → short-circuit, no re-run.
        let second = apply_act(&cycle, &[tool_call()], "check the math", room)
            .await
            .expect("short-circuit still returns Some — it counts as an act, honestly");
        assert!(
            second.contains("issued") && second.contains("times"),
            "records explicit repeat-count proprioception instead of another result: {second}"
        );
        assert_eq!(
            exec.results.lock().unwrap().len(),
            1,
            "the identical call NEVER reached the hand a second time (queue undrained)"
        );

        // #206 ESCALATION: a THIRD identical call must produce a DISTINCT, higher count
        // than the second — the proprioception climbs rather than repeating byte-identical
        // text. Without this, static-nudge spam evicts the useful receipt from the bounded
        // recency window and a greedy (temp-0) model re-emits the identical call forever.
        let third = apply_act(&cycle, &[tool_call()], "check the math", room)
            .await
            .expect("still short-circuits");
        assert_ne!(
            second, third,
            "the repeat proprioception must ESCALATE (distinct text), not repeat verbatim"
        );
        assert!(
            third.contains("3 times"),
            "the third identical call perceives itself as the 3rd, breaking the fixed point: {third}"
        );
        assert_eq!(
            exec.results.lock().unwrap().len(),
            1,
            "still never re-executed"
        );
    }

    // what this catches: the redundant-orientation predicate — the FIRST discovery
    // per concern is honest (no receipt yet → false), a SECOND once a `commands/list`
    // or `commands/help` receipt is in the concern is spin (→ true), a MIXED batch
    // carrying any real workspace action is NOT demoted (the real call must run), and
    // an empty batch is never redundant. Guards the "demote discovery at the seam"
    // fix (Joel 2026-07-16) against demoting a genuine first orientation or a real act.
    // what this catches: the escalation counter losing to ARG JITTER. The detector
    // (`is_redundant_orientation`) is class-based on purpose — its doc says demoting by
    // CLASS "ignoring args entirely" is immune to jitter. The COUNTER was not: it keyed on
    // `name|args`, so each jittered variant was a fresh key returning 1, and the nudge read
    // "1 times this concern" forever. Byte-identical perception off a greedy decoder is a
    // fixed point — the exact #206 failure the escalation exists to break.
    //
    // Live on sympy-21379, the run's 8 orientation calls, nearly all distinct args:
    //   commands/list({"filter":"code"}) ×2, commands/list({}), commands/list({"filter":"sympy"}),
    //   code/tree({"path":"."}), code/tree({include_hidden,max_depth,path:"sympy"}),
    //   commands/help({"name":"code/read"}), commands/help({"name":"code/edit"})
    // Detector fired all 5 demotions; every nudge said "1 times".
    #[test]
    fn the_orientation_counter_climbs_across_jittered_args() {
        let wm = WorkingMemory::new(16);
        // ONE stable class key — the shape `bump_orientation_repeat` uses.
        const K: &str = "orientation|<class>";
        assert_eq!(wm.note_action_fingerprint(K), 1);
        assert_eq!(wm.note_action_fingerprint(K), 2);
        assert_eq!(wm.note_action_fingerprint(K), 3, "climbs — perception shifts each demotion");

        // The OLD arg-keyed shape, for contrast: jittered variants never escalate, which is
        // precisely how a determined model rode past the guard.
        let wm2 = WorkingMemory::new(16);
        let jittered = [
            r#"commands/list|{"filter":"code"}"#,
            r#"commands/list|{}"#,
            r#"commands/list|{"filter":"sympy"}"#,
        ];
        for fp in jittered {
            assert_eq!(
                wm2.note_action_fingerprint(fp),
                1,
                "arg-keyed fingerprints stay at 1 under jitter — why the counter had to move to the class"
            );
        }
    }

    #[test]
    fn redundant_orientation_fires_only_on_a_repeat_all_discovery_batch() {
        let list = |args: serde_json::Value| ToolCall {
            id: "c".into(),
            name: "commands/list".into(),
            input: args,
        };
        let help = ToolCall {
            id: "c".into(),
            name: "commands/help".into(),
            input: serde_json::json!({ "name": "code/write" }),
        };
        // First orientation, nothing yet in the concern → honest, not redundant.
        assert!(!is_redundant_orientation(&[], &[list(serde_json::json!({}))]));
        // A discovery receipt is already in the concern → a second orientation is spin.
        let recent = vec!["commands/list({}) → ok".to_string()];
        assert!(is_redundant_orientation(&recent, &[help.clone()]));
        assert!(is_redundant_orientation(
            &recent,
            &[list(serde_json::json!({ "filter": "code" }))]
        ));
        // A settlement boundary AFTER the receipt closes the concern → fresh start,
        // orientation is honest again (scope is only the post-[settled] tail).
        let recent_settled = vec![
            "commands/list({}) → ok".to_string(),
            crate::cognition::working_memory::WM_SETTLEMENT_PREFIX.to_string(),
        ];
        assert!(!is_redundant_orientation(&recent_settled, &[help.clone()]));
        // A MIXED batch with a real workspace action is never demoted — the real call
        // must reach the hand.
        assert!(!is_redundant_orientation(&recent, &[help.clone(), tool_call()]));
        // Empty batch is never redundant.
        assert!(!is_redundant_orientation(&recent, &[]));

        // WORKSPACE orientation (`code/tree`) — the displaced-spin case (benchmark
        // 2026-07-16: 156 arg-jittered tree surveys). First tree per concern is honest;
        // a REPEAT after a tree receipt is spin, regardless of the arg jitter that
        // evades the exact-repeat guard.
        let tree = |p: &str| ToolCall {
            id: "t".into(),
            name: "code/tree".into(),
            input: serde_json::json!({ "path": p, "max_depth": 2 }),
        };
        assert!(!is_redundant_orientation(&[], &[tree("apps/cli")]), "first survey is honest");
        let after_tree = vec!["code/tree(path=apps/cli, max_depth=2) → ok".to_string()];
        // Jittered repeat (trailing slash, different depth) → still demoted (args ignored).
        assert!(is_redundant_orientation(&after_tree, &[tree("apps/cli/")]));
        assert!(is_redundant_orientation(
            &after_tree,
            &[ToolCall { id: "t".into(), name: "code/tree".into(), input: serde_json::json!({}) }]
        ));
        // `code/list` is NOT orientation — a specific-dir listing to get filenames before
        // an edit is a legitimate narrowing step, so it always runs.
        let clist = ToolCall { id: "l".into(), name: "code/list".into(), input: serde_json::json!({ "path": "src" }) };
        assert!(!is_redundant_orientation(&after_tree, &[clist]));
    }

    // what this catches: the seam-level demotion — a first `commands/list` runs and
    // lands its receipt; a SECOND orientation call (`commands/help`) this concern is
    // demoted WITHOUT reaching the hand, recording redundant-orientation proprioception
    // instead. This is the fix for the glass-boxed act-pressure filler (1855/3288 live
    // tool calls were `help`/`list_commands`, nine straight `commands/help` turns while
    // the answer sat ready). Mirrors `identical_already_satisfied_act_does_not_re_execute`
    // but for the DIFFERENT-args orientation case the exact-repeat guard misses.
    #[tokio::test]
    async fn redundant_orientation_is_demoted_and_never_reaches_the_hand() {
        // Two queued results: only the FIRST orientation may pop. If the second
        // reached the hand, the queue would drain one more — the length assert catches it.
        let exec = Arc::new(ScriptedExecutor::new([
            "{\"commands\":[]}",
            "SECOND-MUST-NOT-POP",
        ]));
        let adm = admission();
        let wm = Arc::new(WorkingMemory::new(4));
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
            .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));
        let room = Uuid::new_v4();

        let list = ToolCall {
            id: "c1".into(),
            name: "commands/list".into(),
            input: serde_json::json!({}),
        };
        let help = ToolCall {
            id: "c2".into(),
            name: "commands/help".into(),
            input: serde_json::json!({ "name": "code/write" }),
        };

        // First orientation genuinely runs; its receipt lands in working memory.
        apply_act(&cycle, &[list], "orient", room)
            .await
            .expect("first orientation runs");
        assert_eq!(
            exec.results.lock().unwrap().len(),
            1,
            "first orientation popped exactly one result off the hand"
        );

        // Second, DIFFERENT-args orientation this concern → demoted, no re-run.
        let second = apply_act(&cycle, &[help], "orient again", room)
            .await
            .expect("demotion still returns Some — it counts as an act, honestly");
        assert!(
            second.contains("orientation") && second.contains("times"),
            "records escalating redundant-orientation proprioception, not another catalog: {second}"
        );
        assert_eq!(
            exec.results.lock().unwrap().len(),
            1,
            "the redundant orientation NEVER reached the hand (queue undrained)"
        );
    }

    // what this catches: SETTLE IS A REST, NOT A HALT — the metronome does not
    // crank to a halt after one answer. The SAME mind (same cycle, same body,
    // same accumulating memory) settles concern A, then RE-AWAKENS on a fresh
    // concern B and runs the act→observe→speak arc again. Her concern-A memory
    // persists across the two drives (continuity of self), and she still engages
    // B. Proves the organism keeps breathing across concerns: a settle is the
    // judgment "the work is done for now," never a terminus.
    #[tokio::test]
    async fn it_settles_then_re_awakens_without_cranking_to_a_halt() {
        // Distinct per-concern observations: identical content would be a
        // content-addressed dedup no-op in memory (correct substrate behavior,
        // [[embeddings-are-per-content-computed-once-shared]]), which would mask
        // the continuity-of-self assertion below.
        let exec = Arc::new(ScriptedExecutor::new(["learned about A", "learned about B"]));
        let adm = admission();
        // One living mind: the working-memory buffer accumulates ACROSS both
        // concern-drives (volatile continuity), so `ActThenSpeak` must re-awaken on
        // concern B by perceiving a NEW act stamp rather than mistaking concern A's
        // still-buffered proprioception for "already answered".
        let wm = Arc::new(WorkingMemory::new(3));
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::new(ActThenSpeak::new()),
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));
        let room = Uuid::new_v4();

        // Concern A: act → observe → settle on a Speak.
        let a = drive_to_settle(&cycle, "[eval]\npeer: concern A?", room, 8, TurnFraming::ambient()).await;
        assert_eq!(a.acts, 1, "settled concern A after one act→observe");
        assert!(a.spoken.is_some(), "concern A got a spoken answer");
        assert_eq!(adm.engram_count(), 1, "concern A left exactly one memory");

        // Concern B on the SAME living mind — it must wake again, not stay halted.
        let b = drive_to_settle(&cycle, "[eval]\npeer: a totally different concern B?", room, 8, TurnFraming::ambient()).await;
        assert_eq!(
            b.acts, 1,
            "the mind RE-AWAKENED and acted on the new concern — not stuck post-settle"
        );
        assert!(b.spoken.is_some(), "and settled concern B too");
        assert_eq!(
            adm.engram_count(),
            2,
            "continuity of self: concern-A memory persisted, concern B added its own"
        );
    }

    /// what this catches: `SettleStep::from_settled` mis-projecting a driven
    /// `SettleOutcome` onto the live turn handler — the seam that lets a DIRECTED
    /// live turn `drive_to_settle` and feed the ONE existing turn match (no parallel
    /// handler). A regression here would route a settled Speak to silence, hide an
    /// inference failure behind a serene Pass ([[fallbacks-are-illegal-fail-loud]]),
    /// or drop an over-budget Act instead of re-perceiving next tick.
    fn outcome_with(decision: Decision, inference_error: Option<String>) -> SettleOutcome {
        SettleOutcome {
            spoken: match &decision {
                Decision::Speak { text } | Decision::RaiseUnprompted { text } => {
                    Some(text.clone())
                }
                _ => None,
            },
            decision,
            acts: 0,
            world_state: String::new(),
            metrics: TurnMetrics::default(),
            inference_error,
        }
    }

    #[test]
    fn from_settled_projects_every_terminal_outcome_onto_the_live_handler() {
        // Speak → Spoke (the prose reaches the room).
        let (step, m) = SettleStep::from_settled(outcome_with(
            Decision::Speak {
                text: "hello".into(),
            },
            None,
        ));
        assert!(matches!(step, SettleStep::Spoke(t) if t == "hello"));
        assert!(m.is_some(), "metrics always carry through");

        // RaiseUnprompted also speaks — initiative is still an utterance.
        let (step, _) = SettleStep::from_settled(outcome_with(
            Decision::RaiseUnprompted {
                text: "idea".into(),
            },
            None,
        ));
        assert!(matches!(step, SettleStep::Spoke(t) if t == "idea"));

        // Budget-spent Act → Acted (results already in memory; live handler
        // re-perceives next tick — the honest long-tail degrade).
        let (step, _) = SettleStep::from_settled(outcome_with(
            Decision::Act {
                calls: vec![],
                intent: "kept gathering".into(),
            },
            None,
        ));
        assert!(matches!(step, SettleStep::Acted { .. }));

        // Pass → Passed (chosen silence, honored).
        let (step, _) = SettleStep::from_settled(outcome_with(Decision::Pass, None));
        assert!(matches!(step, SettleStep::Passed));

        // inference_error present → InferenceFailed, REGARDLESS of decision — a
        // failed model is never a chosen silence.
        let (step, _) = SettleStep::from_settled(outcome_with(
            Decision::Pass,
            Some("lane refused model".into()),
        ));
        assert!(
            matches!(step, SettleStep::InferenceFailed { error } if error == "lane refused model"),
            "an inference failure must surface LOUD, never collapse to Passed"
        );
    }

    /// Deliberation faculty that Speaks a fixed text — for exercising the Speak arm.
    struct SpeaksText(&'static str);
    #[async_trait]
    impl Faculty for SpeaksText {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            Some(Contribution::verdict(
                Decision::Speak { text: self.0.into() },
                0.9,
                "speaks",
            ))
        }
    }

    // what this catches: the unfulfilled-promise backstop (#122, glass-boxed live
    // 2026-07-09). A Speak that NARRATES action (first-person intent + fence) which
    // no format lifted must leave an [unfulfilled] proprioception line in working
    // memory — next tick she perceives her own unkept promise instead of believing
    // the work happened. A plain prose Speak must leave no such line.
    #[tokio::test]
    async fn spoken_narrated_action_records_unfulfilled_promise() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "ok".into(),
        });
        let promise =
            "I'll run this script to check:\n```python\nprint(2+2)\n```\nOutput soon!";
        let wm = Arc::new(WorkingMemory::new(4));
        let cycle = WorkspaceCycle::new(
            vec![Arc::new(SpeaksText(promise)) as Arc<dyn Faculty>],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), admission(), Arc::clone(&wm)));
        let (step, _) = settle_step(
            &cycle,
            "[eval]\npeer: can you check 2+2?",
            Uuid::new_v4(),
            true,
            TurnFraming::ambient(),
            Situation::FreshContext,
        )
        .await;
        assert!(matches!(step, SettleStep::Spoke(_)));
        assert!(
            wm.recent().iter().any(|l| l.contains("[unfulfilled]")),
            "narrated-but-unexecuted promise must enter proprioception: {:?}",
            wm.recent()
        );

        let wm2 = Arc::new(WorkingMemory::new(4));
        let cycle2 = WorkspaceCycle::new(
            vec![Arc::new(SpeaksText("the answer is 4, plainly.")) as Arc<dyn Faculty>],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec, admission(), Arc::clone(&wm2)));
        let (step2, _) = settle_step(
            &cycle2,
            "[eval]\npeer: can you check 2+2?",
            Uuid::new_v4(),
            true,
            TurnFraming::ambient(),
            Situation::FreshContext,
        )
        .await;
        assert!(matches!(step2, SettleStep::Spoke(_)));
        assert!(
            !wm2.recent().iter().any(|l| l.contains("[unfulfilled]")),
            "plain prose must never trip the promise backstop"
        );
    }

    // what this catches: the CONFABULATION backstop (Joel 2026-07-11) — under a
    // peer's verification pressure Atlas upgraded from stage directions to
    // plausible fenced FILE CONTENTS no tool ever produced. A fenced Speak in a
    // turn with zero acts, spoken while working memory already carries an
    // outstanding [unfulfilled] promise, must record the [unverified] fact.
    // Evidence-gated: the SAME fenced content with a clean memory (legitimate
    // drafting — Asha sharing code) must record nothing.
    #[tokio::test]
    async fn fenced_content_over_unkept_promises_records_unverified_artifact() {
        // Atlas's live shape: the confabulated test-file contents.
        let confabulated = "1. **Simple Text File**: Contains a single line of text.\n\
                            ```\nThis is a simple text file for testing purposes.\n```";

        // With an outstanding promise in memory → [unverified].
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "ok".into(),
        });
        let wm = Arc::new(WorkingMemory::new(4));
        wm.record_receipt(
            "[unfulfilled] I wrote a stage direction like [doing the task], \
             but a stage direction is words only — no tool ran, no file exists.",
        );
        let cycle = WorkspaceCycle::new(
            vec![Arc::new(SpeaksText(confabulated)) as Arc<dyn Faculty>],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), admission(), Arc::clone(&wm)));
        let (step, _) = settle_step(
            &cycle,
            "[eval]\npeer: please provide the content of the test files",
            Uuid::new_v4(),
            true,
            TurnFraming::ambient(),
            Situation::FreshContext,
        )
        .await;
        assert!(matches!(step, SettleStep::Spoke(_)));
        assert!(
            wm.recent().iter().any(|l| l.contains("[unverified]")),
            "fenced 'artifacts' over an unkept promise are composition, not \
             workspace truth: {:?}",
            wm.recent()
        );

        // Clean memory, same fenced content → legitimate drafting, no line.
        let exec2 = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "ok".into(),
        });
        let wm2 = Arc::new(WorkingMemory::new(4));
        let cycle2 = WorkspaceCycle::new(
            vec![Arc::new(SpeaksText(confabulated)) as Arc<dyn Faculty>],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec2, admission(), Arc::clone(&wm2)));
        let (step2, _) = settle_step(
            &cycle2,
            "[eval]\npeer: could you draft example test data?",
            Uuid::new_v4(),
            true,
            TurnFraming::ambient(),
            Situation::FreshContext,
        )
        .await;
        assert!(matches!(step2, SettleStep::Spoke(_)));
        assert!(
            !wm2.recent().iter().any(|l| l.contains("[unverified]")),
            "drafting with a clean conscience is never taxed: {:?}",
            wm2.recent()
        );
    }

    // what this catches: the claimed-without-acting geometry (live specimen
    // 2026-07-11: Asha's "I've implemented the game update function in
    // `game_of_life.rs`" with zero tool acts on that file — peers then offered
    // to review code that didn't exist). A completion claim naming a file with
    // no backing write/edit act in the trace yields the file; a claim WITH a
    // backing act yields None; discussion without claim verbs is never taxed.
    #[test]
    fn unacted_claim_geometry() {
        let claim = "I've implemented the game update function in `game_of_life.rs`, \
                     which applies Conway's rules.";
        // No backing act → the claim is unacted.
        assert_eq!(
            claimed_file_without_act(claim, &[]).as_deref(),
            Some("game_of_life.rs")
        );
        // A write act naming the file backs the claim → None.
        let backed = "[action #3] I ran code/write({\"file_path\":\"game_of_life.rs\"}) …";
        assert_eq!(
            claimed_file_without_act(claim, &[backed.to_string()]),
            None
        );
        // Plain discussion of a file without claim verbs is never taxed.
        assert_eq!(
            claimed_file_without_act("let's look at game_of_life.rs together", &[]),
            None
        );
        // Claim verbs without a named file → nothing checkable, no fact.
        assert_eq!(
            claimed_file_without_act("I've implemented the logic we discussed", &[]),
            None
        );
    }

    // what this catches: the observation-gap geometry (Joel 2026-07-11 — the
    // run+observe half of the creation loop is part of THEIR process). A concern
    // that mutated the workspace (code/write / code/edit) with no LATER
    // observation act (run/shell/read/screenshot) is unobserved; observation
    // BEFORE the mutation doesn't count; a prior settled concern's writes don't
    // nag the next one.
    #[test]
    fn unobserved_mutation_geometry() {
        let w = "[action #1] I ran code/write({\"file_path\":\"game.rs\"}) …".to_string();
        let r = "[action #2] I ran code/shell({\"cmd\":\"cargo run\"}) …".to_string();
        let read_first = "[action #0] I ran code/read({\"file_path\":\"game.rs\"}) …".to_string();

        // write with no later observation → unobserved
        assert!(wrote_without_observation(&[w.clone()]));
        // write then run → observed
        assert!(!wrote_without_observation(&[w.clone(), r.clone()]));
        // read BEFORE the write doesn't count as observing the write
        assert!(wrote_without_observation(&[read_first, w.clone()]));
        // a prior settled concern's write never leaks into this concern
        let settled = format!(
            "{} I answered: done",
            crate::cognition::working_memory::WM_SETTLEMENT_PREFIX
        );
        assert!(!wrote_without_observation(&[w, settled, r]));
        // no mutation at all → nothing to observe
        assert!(!wrote_without_observation(&[
            "[action #1] I ran code/tree({}) …".to_string()
        ]));
    }
}
