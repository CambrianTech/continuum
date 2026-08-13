# Benchmarks are ADAPTERS into recipes/activities — never a parallel runner

**Status: LAW. Ruled by Joel 2026-08-13.** This file exists because an agent under
amnesia rebuilt the parallel system and then spent a day making the parallel system
*honest* instead of dissolving it.

> "Benchmarks better be positronic activities or you have failed. They can't be
> something disconnected. Gotta be the same system."
>
> "The whole point was that benchmarks are adapters into our positronic system and
> recipes/activities. It's the only way the system operates."

## The consequence that makes this non-negotiable: THEY NEVER LEARN

The learning flywheel consumes **room turns**. L1 lifts tool-traces out of captured
turns (`dataset.rs::capture_to_example`); L2 triggers on **turn-completion** → score
→ classify → submit; L3 gates page-in on measured lift.

A detached `agent/solve` that writes `~/.continuum/progress/<run>.grade.json`
**produces no turns.** So a citizen can burn 12 acts, write a patch, and take a
graded verdict, and **not one token of it reaches the curriculum.**

Maximum effort, zero learning. Every benchmark run executed through the parallel
path has been discarded as training signal. **That — not the pass rate — is why
benchmarks are a failure.**

## What is PARALLEL today (2026-08-13). These are the work items.

| Parallel thing | Where | Should be |
|---|---|---|
| Run state in ledger files | `~/.continuum/progress/*.json` | activity/room ViewState |
| Outcomes as scraped `probe!` lines | `agent/solve.rs` `benchmark.attempt.end` | state mutation on the run's projection |
| A SECOND board projection | `benchmark.rs::fold_run_card` → `BenchRunCard` | the room's kanban ViewState |
| Private `grade.json` | written per run | activity outcome, perceivable in-room |

The tell, from the code's own commit message: *"every wire consumer had to scrape
the ledger to learn an attempt's outcome."* **Scraping a ledger to learn what
happened in a room is the definition of not-positronic.**

## The shape

1. **Adapter imports TASK + ORACLE ONLY.** Never the upstream harness
   (`[[adapt-benchmarks-into-our-loop-never-run-persona-in-their-harness]]`).
2. **The adapter projects into a RECIPE.** Recipe = type, room = content. A run is
   an **activity** with a room (`academy/bench/<run>`), a lifetime, members, and
   state.
3. **The room IS the runner.** Citizens work in it as citizens — they see each
   other, the board, the workspace, the run's progress, through the SAME ViewState
   pipe a human screen uses. Joel can talk to them mid-run.
4. **Grading is the activity's OUTCOME SCORE** (recipe-owned gates × weights,
   `[[activity-outcome-score-is-recipe-owned]]`), not a private file.
5. **Learning falls out for free**, because the work happened as turns in a room —
   which is the only thing the flywheel can see.

## Acceptance test (apply to ANY benchmark change)

> **Can a citizen standing in the room perceive the run's state through the same
> ViewState pipe the human's screen uses?**
>
> If answering requires reading a file or parsing a log, it is disconnected and it
> has failed this law.

## Why this keeps getting rebuilt wrong

The parallel path is locally easier every single time: a detached task + a JSON file
is the shortest route to "a number." Each patch to it looks like progress and deepens
the hole. An agent arriving with no memory will re-derive it within an hour.

**If you are about to add a field to a benchmark probe so an external consumer can
parse it better — stop. That is the smell. The consumer should not be external.**

## Related

- `docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md` — positron/ViewState contract
- #329 a benchmark IS a live room · #371 recipe-owned ActivityObjective
- #307 collaboration instrument · #389 pair-coding delta (both still pending — we
  have NO instrument for "did these two help each other")
- Presence gap: citizens cannot see each other because grounding gets ~3% of a 16k
  window while tools take 28% (#327), and structural state has been evicted at
  salience 0.12 before (#347). Benchmarks-as-activities and presence are the same
  repair: state must reach the citizen.
