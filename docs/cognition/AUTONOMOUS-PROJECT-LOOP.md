# The Real Deal — an Autonomous, Test-Graded, Training-Coupled Project Loop

> **Status:** blueprint (2026-06-23). The synthesis where the persona stops being a
> reactive demo and becomes a continuous working mind: always doing real,
> test-graded work, training on the result, forever. Unifies the never-stop loop,
> the tools, `dataset/from-captures`→`forge/train`, and the eval — on the
> measurement spine. Read [ANY-ASK-IS-A-CLASS](../genome/ANY-ASK-IS-A-CLASS.md)
> (the class), [SELF-EVOLVING-GENOME](../genome/SELF-EVOLVING-GENOME.md) (fitness),
> [DREAM-CONSOLIDATION](DREAM-CONSOLIDATION.md) (memory), and
> [ORGANIC-SUBSTRATE](ORGANIC-SUBSTRATE.md) (never-stop) first.

## The one insight

A persona that only re-reacts to external messages goes quiet when no one talks to
it — it is reactive, not organic. We *deliberately* gated the heartbeat on external
change (`external_fingerprint`, `service_loop.rs`) to stop a self-talk flood. **That
gate is exactly why she isn't always-on.** The fix is not "let her react to her own
noise" (the flood). It is: **give her real work to always be doing.** The drive
becomes *make progress on my current task* — productive, not chatter. So:

> **The benchmark gym and the always-on mind are the same build.** She runs
> infinitely *because there is always a task to advance*, and she is a real
> participant *because she's actually doing something and has progress to share.*

## The loop

```
pull a real task (from the gym)
  → attempt it with tools (code/search, read, edit, shell)
  → TESTS grade it — objective, repeatable, learnable (pass/fail, not substring-on-prose)
  → record the turn (from-captures)  →  train the genome on outcomes (forge/train)
  → next task  →  forever
```

This is "run and run and train and run." Test-grading is the key: a task either makes
its tests pass or it doesn't — repeatable signal you can train on infinitely, no
human in the grading loop.

## Why this fixes all three gaps at once

- **Organic / always-on** — the loop never ends; idle only when the gym is empty.
  The heartbeat's reason to fire is *work in progress*, not external noise → no flood.
- **Reliable + improving** — test-graded outcomes are honest fitness (genome §3); the
  loop trains on real pass/fail, so consistency climbs measurably (the variance
  barrier we hit: 7/10→4/10 on the same eval).
- **A real teammate you can talk to** — she's continuously doing real work, so she has
  genuine progress, blockers, and findings to surface (the `RaiseUnprompted` decision
  becomes meaningful, not repetition).

## Grounded — built vs. the gap

| Piece | Seam | Status |
|---|---|---|
| Gym (real test-graded tasks) | `dataset.rs:369 import_realclasseval` (real classes + pynguin tests) | ingestion exists; **running her on it + test-grading not wired** |
| The engine (never-stop) | `service_loop.rs` heartbeat + `run_self_cycle` | exists, but gates on EXTERNAL change → reactive, no internal drive |
| Tools (hands) | code/* + shell, all described | shipped, clean |
| Capture → train | `dataset/from-captures` + `forge/train` | shipped, proven on real turns |
| The gate (measure) | `cognition/eval` (+ causal tasks) | shipped (toy substring grading — must graduate to test-grading) |
| Internal drive | — | **MISSING** — she has no self-task / current-project state |
| Autonomous loop tying them | — | **MISSING** — the keystone of this doc |

## Build slices (smallest-first, each measurable)

1. **Real test-graded task** — replace one toy substring eval with a task whose
   grade is "run the test, did it pass?" (via `code/shell`). Proves objective,
   repeatable grading. Seed from `import_realclasseval`. *The gym, slice 1.*
2. **A task queue + current-project state** — a persona carries a current task
   (pulled from the gym) and its progress. This is the internal DRIVE: the heartbeat
   advances the current task instead of idling. *Makes her always-doing.*
3. **The autonomous loop** — heartbeat: if I have a current task, take the next step
   (attempt → run tests → reflect) ; when graded, record + pull the next. Bounded
   per tick (off the response hot path, concurrency guide). *Always-on, productive.*
4. **Training coupling** — successful task turns → `from-captures` (curated to
   passing runs) → `forge/train` → eval shows lift → adopt. *Run-and-train.*
5. **The eval graduates** — `cognition/eval` measures pass-rate on the test-graded
   gym, with variance (each task ×N) and a bare-unsloth A/B lane. *Consistency and
   the harness edge become numbers.*

## The measurement spine (non-negotiable)

Test-grading is what makes this *not a toy*: a task that runs its own tests can't be
faked, can't be confabulated, and trains honestly. Every slice gates on a number —
pass-rate on the gym, variance across runs, lift after training, harness-vs-bare
delta. A loop that runs infinitely on a grade you can't trust is an infinite
garbage machine; the tests are the trust.

## Honest current state

She is **reactive, not organic** — verified: the heartbeat fires only on external
change, she has no internal drive, no current-project state, and the eval grades
prose substrings, not tests. The pieces to build the real deal exist as seams
(`import_realclasseval`, never-stop, from-captures, forge/train, eval); the keystone
— the autonomous test-graded loop with an internal drive — is the build this doc
exists to sequence. Slice 1 (a real test-graded task) is the next focused move.
