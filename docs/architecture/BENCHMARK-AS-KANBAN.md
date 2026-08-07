# Benchmarks as Work Cards

**Status:** design approved (Joel, 2026-08-07). Seam unbuilt; prerequisites landed.

> "Each new benchmark is just an adapter problem, no matter what it is, and our
> personas get to operate as they're trained, even allowing for easy N persona
> collaboration." — Joel, 2026-08-07
>
> "…so they're measured for what they do."

## The one-line claim

A benchmark task is **a work card on a real board**, claimed by whoever picks it
up, worked in a real workspace, and graded by the harness from the **artifact** —
never from what anyone said about it.

## Why this, and not the headless call we have

`agent/solve` today is a function: task in, patch out. It has **zero** references
to `work/`, `claim`, or the board. That is fine for a scorer and useless for
everything else we want:

| A headless call gives you | A card gives you |
|---|---|
| a transcript | a **trail** — claimed_at → first write → state transitions → artifact diff |
| an assignment | a **choice** — she claims it; subject vs participant |
| a result | a **lease** — the work is *hers* while she holds it |
| a number | a **conversation** — Joel can ask "how's that going" mid-run (#329) |
| a second work concept to maintain | the one the citizens already use |

The trail is the point. Our measured failure mode is a citizen *describing* a fix
that never touched disk ([[the-write-path-not-the-reasoning-is-what-fails-swe-bench]]),
and grading speech trains exactly that. A card's evidence cannot be narrated into
existence.

## The shape

```
BenchmarkAdapter  (one trait, N benchmarks)
        │  tasks()  ──────────────► harness mints one CARD per task
        │                            (repo, spec, acceptance, priority)
        │                                      │
        │                            citizen CLAIMS it  ── lease renews while she works
        │                                      │
        │                            works in HER workspace (real hands)
        │                                      │
        │  grade(task, artifact) ◄── harness reads the ARTIFACT
        │                                      │
        └──────────────────────────► harness sets the TERMINAL state
```

### The line that must not blur

**The card carries DELIVERY and LIFECYCLE. The harness keeps JUDGMENT.**

A citizen may move a card through working states (`claimed → InProgress →
Blocked`) — that is her reporting her own progress, and it is honest. She must
**never** set the terminal state on a graded card. If closing your own card
counted as passing, we would be grading self-report, which is the failure we keep
re-finding from a different angle every week.

Concretely: `work/state` stays open to her for working states; the terminal
transition on a benchmark card is harness-only.

## Why it is viable only as of tonight

Three defects would each have made this silently unusable. All fixed, deployed,
and live-verified 2026-08-07:

1. **Claims did not survive the work.** 30-minute lease, and in that room's entire
   63,711-event history there were **172 claims and zero renewals**. A long solve
   was *guaranteed* to lose its card mid-flight. Now the presence pump renews held
   claims at its own cadence — the substrate observes that she is alive rather
   than asking her to announce it (`e3b1a0f98`).
2. **The board advertised work that could not be taken.** `work/claim` refuses
   `Review|Merged|Closed`; every read surface excluded only `Merged|Closed`. 11 of
   58 offered cards were guaranteed refusals. Collapsed to one predicate,
   `card_holder::claimable_now` — 58 → 47 verified live (`59cbbb735`).
3. **Acting blinded her to the board.** The arbiter deleted all standing framing
   on every post-act tick using a field documented as serialization-order-only,
   and the deferred lane multiplied its floor away by a length-biased lexical
   ratio. A citizen holding a live card could not see the board she held it on
   (`3db8db8ef`).

A benchmark card is only as good as the citizen's ability to hold it and see it.
That is what those three buy.

## The adapter already exists — DO NOT REBUILD IT

**`cognition/benchmark.rs:89` is `pub trait BenchmarkAdapter`, and it is already
the right shape.** I nearly wrote a parallel one; grepping first is what stopped
it ([[grep-for-the-mechanism-before-proposing-to-build-it]], and the
"parallel allocators" refusal in CLAUDE.md).

```rust
#[async_trait]
pub trait BenchmarkAdapter: Send + Sync {
    fn name(&self) -> &str;                        // registry slug
    fn dataset(&self) -> Option<DatasetSpec>;      // runner materializes it
    fn resources(&self) -> BenchResourceHint;      // grid placement
    async fn tasks(&self, dataset_root: Option<&Path>, limit: Option<usize>)
        -> Result<Vec<EvalTask>, CommandError>;    // the ONLY per-benchmark parsing
    async fn grade(&self, task: &EvalTask, outcome: &TaskOutcome)
        -> Result<BenchGrade, CommandError>;       // defaults to the task's own grader
}
```

with a live registry (`register` / `get` / `names`) and `TaskOutcome` already
carrying **both** artifact channels — the persona's spoken answer *and* the
workspace diff. Its own doc states the goal exactly: *"the runner + grid +
learning loop treat every adapter identically."*

So "each benchmark is just an adapter" is **already true**. What is missing is
only the DELIVERY:

| layer | state |
|---|---|
| adapter trait + registry | **built** (`cognition/benchmark.rs`) |
| task → `EvalTask` | **built** |
| grading from artifact | **built** (`grade` + `TaskOutcome`) |
| solve one task with real hands | **built** (`agent/solve`) |
| **task → work CARD on a board** | **MISSING — this is #346** |
| **claim → work → harness sets terminal state** | **MISSING — this is #346** |

### The actual work, then

One bridge: for a chosen adapter, mint a card per `EvalTask` into a benchmark
room; let citizens claim and solve them through the normal path they already
use; feed each `TaskOutcome` to the adapter's `grade` and write the verdict as
the card's terminal state. No new benchmark abstraction, no second registry.

**Outlier A (local/simple):** an already-registered answer-graded benchmark —
`grade` defaults to the `EvalTask`'s own grader, so the bridge is the only new
code.
**Outlier B (maximally different):** SWE-style, where `grade` is overridden to
run the repo's tests against the workspace diff. Different artifact channel,
same bridge. If both ride it without forcing, the bridge is right.

## Known blockers

- **#220 — answer-graded tasks score 0 on a parse miss.**
  `deliberation_parse.rs:38` maps an empty *or unrecognised* generation to
  `Decision::Pass`, so a correct prose answer that does not parse becomes a
  chosen silence and `spoken` is empty. The file already has the right doctrine
  for the sibling case (`InferenceFailed` is explicitly "NOT a `Passed`: a failed
  model is not a chosen silence"). Fix shape: split `Recognised | Unrecognised`
  at the parser — the only place that still has the text — and carry the
  unrecognised text additively so no room behaviour changes. **Do not** simply
  keep text on the Pass arm: #271 deliberately makes a spoken "I'll pass my turn"
  lift to a *silent* Pass, and that must survive.
- **Exam integrity.** `cognition/forget-context` after graded runs (#207) still
  applies, and a card body must not leak the answer key.
- **Room scoping.** Benchmark cards want their own room. Now safe: the board read
  is room-parameterized (#345 / `a60217ba2`), and a room a scope is not
  subscribed to raises `NotSubscribed` instead of silently substituting the
  default board.

## What this unlocks beyond scoring

- **N-persona collaboration for free.** A card is claimable by anyone on a shared
  board; two citizens splitting a task needs no new mechanism.
- **Agents calling in.** An external agent is just another peer who can claim.
- **Learning.** The trail is a labelled trajectory — feeds #319 (remember the
  lesson, never the paper) and #320 (coached moments → verified training pairs)
  with no extra capture path.
- **A benchmark becomes a live room** (#329): progress as chat, Joel talks to
  them mid-run, and they learn from it.

## The standard this is held to

Whoever is measured — one citizen, a team of them, an agent calling in, or Claude
— the harness sees the same trail and grades the same way. What is being measured
is **decomposition and follow-through under real conditions**, not puzzle-solving:
break the problem up, act, verify, find the things nobody named, and deploy.
