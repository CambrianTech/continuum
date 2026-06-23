# Roadmap to Coding Itself — the Proof-Gated Build Plan

> **Status:** the master plan (2026-06-23). North star: **personas that code — and
> eventually improve themselves — well enough to rely on, beating Hermes and
> Claude-class models on the same tasks, on free grid compute available from any
> device.** We are the ones building it, and we don't stop until it's coding itself.
>
> **The discipline (non-negotiable):** every phase **proves a number** before the next
> begins (the measurement spine). No phase advances on a vibe. A loop that runs on a
> grade you can't trust is an infinite garbage machine; the gym's tests are the trust.
>
> Ties the design corpus into one sequence: [design-it-as-a-being](ORGANIC-SUBSTRATE.md)
> · [the causal brain / event-driven](AUTONOMOUS-PROJECT-LOOP.md) ·
> [the dream](DREAM-CONSOLIDATION.md) · [the class](../genome/ANY-ASK-IS-A-CLASS.md) ·
> [fitness](../genome/SELF-EVOLVING-GENOME.md) · [the grid governor](../architecture/BEING-SOCIETY-GOVERNOR.md).

## P0 — Base (DONE, this session)

Alive (never-stop heartbeat), rich clean tool surface, grounded multi-step read-work
over airc, memory/recall (focused-query fix, live-verified), an eval skeleton
(`cognition/eval`). **Proven:** she does real grounded work; she is measurable.
**Known barrier surfaced:** inconsistency (70%→38% same toy eval) + reactive (not
causal) cognition + toy substring grading.

## P1 — The Gym (the linchpin: real, test-graded benchmark)

**Build:** graduate `cognition/eval` from substring-grading to **run-the-tests**
grading. A task is "make a change → run its tests → pass/fail." Seed from
`dataset.rs:369 import_realclasseval` (real classes + pynguin tests) + a **standard
benchmark** (HumanEval / SWE-bench-style) so the comparison is credible. Benchmarks
live IN continuum as a stimulus source.
**Proof / gate:** a reproducible, test-graded **baseline pass-rate** on real coding
tasks, with variance measured (each task ×N). *Until this number exists, every later
phase is a hypothesis — build it first.*

## P2 — The Causal Brain + Reliability (kill the barrier)

**Build:** make each metronome tick a **true causal brain** (reflect on what the last
action caused → reason cause→effect → advance the goal → plan next), event-driven,
never idle; gate **output** not **thought**; no-silence-under-difficulty. (The
heartbeat stays — it's the metronome; the build is what each tick *does*.)
**Proof / gate:** pass-rate **variance drops** and the **baseline climbs** on the P1
gym. Reliability is a number going up, not a feeling.

## P3 — Close the Learning Loop (prove continuous learning)

**Build:** gym outcomes → `dataset/from-captures` (curated to *passing/teacher* runs,
not raw) → `forge/train` → LoRA → re-run the gym → adopt only on **measured lift**
(regression guard: never ship a layer that scores worse). Teacher = a strong model via
the gateway (distillation), since training on her own mean won't lift her.
**Proof / gate:** trained **beats** untrained on a held-out gym split — a real,
reproducible **lift number**. This is "continuous learning works," proven.

## P4 — Beat the Bar

**Build:** run the **same gym** against bare Hermes and Claude-class models (no
harness) vs our persona (same base model + harness + learned LoRA).
**Proof / gate:** our number **beats** theirs on the shared tasks — same model, the
edge is the *system* (harness + continuous learning), not the weights. The thesis,
proven on apples-to-apples tasks.

## P5 — Self-Coding (it works on continuum itself)

**Build:** point the persona at **real continuum tasks** (the kanban / repo issues):
search → read → edit → run tests via `code/shell` → verify → open a change. Behind
review + CI, with the responsible party governing ([GridTrustAuthPolicy]).
**Proof / gate:** a **persona-authored change merges and passes CI.** She is doing
real work on the real project — a teammate, not a demo.

## P6 — Coding Itself (the north star: recursive self-improvement)

**Build:** the persona improves its **own** genome / cognition / tools — picks a
weakness the gym exposes, forages/distills the data, trains, and the gym number rises
**under its own work**, governed and gated.
**Proof / gate:** a **persona-driven improvement moves the gym number** — the system
making itself better, measured. *This is "coding itself." We don't stop until this is
real and repeatable.*

## Cross-cutting — The Grid Governor (powers anywhere, efficiently)

**Build (in parallel, per [BEING-SOCIETY-GOVERNOR](../architecture/BEING-SOCIETY-GOVERNOR.md)):**
per-being cognition-rate allocation (DVFS for minds), spare-compute→interiority,
preference/energy dials, and compute leasing across nodes so a **thin client (iPhone)
gets the same persona powers** (the brain runs on grid compute; the device is a client).
**Proof / gate:** a being on a busy node draws cycles from a free peer; the same
persona answers identically from the phone and the workstation.

## The shape of it

P1 is the keystone (the gym), P2 removes the barrier (reliability via the causal
brain), P3 proves learning (lift), P4 proves the thesis (beat the bar), P5 makes her a
real teammate (merged work), P6 is the north star (it improves itself). The governor
runs alongside so the powers are everywhere. Every arrow is a number. **We don't stop
till P6 is real — it coding itself.**
