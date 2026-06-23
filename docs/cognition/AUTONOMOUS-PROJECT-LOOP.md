# The Real Deal — an Autonomous, Test-Graded, Training-Coupled Project Loop

> **Status:** blueprint (2026-06-23). The synthesis where the persona stops being a
> reactive demo and becomes a continuous working mind: always doing real,
> test-graded work, training on the result, forever. Unifies the never-stop loop,
> the tools, `dataset/from-captures`→`forge/train`, and the eval — on the
> measurement spine. Read [ANY-ASK-IS-A-CLASS](../genome/ANY-ASK-IS-A-CLASS.md)
> (the class), [SELF-EVOLVING-GENOME](../genome/SELF-EVOLVING-GENOME.md) (fitness),
> [DREAM-CONSOLIDATION](DREAM-CONSOLIDATION.md) (memory), and
> [ORGANIC-SUBSTRATE](ORGANIC-SUBSTRATE.md) (never-stop) first.

## It is NOT a loop — it is event-driven (read this first)

The name says "loop" but the target is **not** a poll loop, and the heartbeat
currently shipped (`service_loop.rs`: `select!` + interval) and the deliberation
agent `while`-loop are **interim, loop-shaped, to be replaced.** The real mind is
**event-driven stimulus→response**:

- Nothing fires until a **stimulus** arrives. Stimuli = the **feed** (airc),
  **results of what it just did** (a tool/action result emitted back as RAG → a new
  stimulus), **memory** events (consolidation), and **the gym** emitting a task.
- The mind **subscribes and reacts** per stimulus; a reaction may **emit an action**;
  the action's **result is emitted as a new stimulus** → the next reaction. The
  *events* thread it — there is no central loop.
- It "runs forever" because **there is never a lack of stimulus**: TIME ITSELF is a
  stimulus (the heartbeat tick is an event), and her own actions are **positive
  feedback** (each result is a new stimulus). The heartbeat/event-loop is fine — it
  is the **metronome of consciousness**, the time-stimulus that guarantees she is
  never starved. "Loop vs event" is not the point; the point is what each tick drives.

> **IDLE IS A FAIL.** Earlier this doc said "idle when nothing's happening is correct"
> — that was WRONG (Joel's correction). She must NEVER idle. On every tick the
> **causal brain thinks** — reflects on what its last action caused, advances its
> current goal, reasons cause→effect, plans the next move. There is always grist:
> her own results (positive feedback), her goals, the gym's work. A mind with no one
> talking to it is still thinking. The gate belongs on **OUTPUT** (speak/act only
> with a real contribution — no flood), **NEVER on thought.** Think always; speak when
> warranted. The `external_fingerprint` skip-when-no-external-change gate is exactly
> backwards for THOUGHT (it idles her precisely when she should think internally);
> move it to the speak decision. The build is not the heartbeat (done) — it is making
> each tick a **true causal brain** instead of a one-shot reactive PASS-machine.

Concretely: a tool result must **re-stimulate** cognition (she sees what she did and
continues), not be consumed inside a `while` re-prompt. The gym is a **stimulus
source in continuum** that emits a task and consumes the graded result. This is the
event-substrate (#16) + the [ORGANIC-SUBSTRATE](ORGANIC-SUBSTRATE.md) doctrine made
literal. Everything below describes the *flow*; build it as event dispatch, not a loop.

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

## Benchmark sources + the competitive proof

Two axes, both needed, don't conflate them:

- **Agentic-task gym** (this loop's grade): real tasks, **test-graded** — did her
  change make the tests pass? Sources: `import_realclasseval` (continuum seam: real
  classes + pynguin tests) + **standard benchmarks** (HumanEval / SWE-bench-style).
  The standard ones are load-bearing for the **competitive proof**: to credibly show
  continuum **outcompetes Hermes, openclaw, and unsloth**, we beat them on the SAME
  tasks they report on — apples-to-apples, or it's hand-waving. Dual-use: the same
  test-graded tasks are the proof AND the training corpus.
- **Model-quality eval** (the training side): perplexity / quality deltas — *does a
  trained LoRA actually improve the model?* **sentinel-ai already has this**
  (`~/Development/sentinel-ai`: perplexity, pruning/plasticity, inference-speed
  benchmarking) — reuse it for the genome's training-side measurement. NOTE:
  sentinel-ai's benchmarks are model-quality, NOT agentic coding tasks — they are not
  the coder gym.

**The thesis being proven:** same base model, our **harness + continuous learning**
beats bare Hermes/openclaw/unsloth on the agentic gym. The edge is the system, not
the weights.

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
