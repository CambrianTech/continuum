# The Room Is The Runner — build plan

**Objective (Joel, 2026-08-18):**

> *"It's whatever is needed to adapt from natural room BACK INTO EVERY benchmark
> scoring… the glue on each side, through natural continuum, and the benchmark not
> running things. It's just another activity. That way any team can be inside. They
> can work together, delegate, do whatever feature continuum makes possible. Without
> it, we're like everyone else, and measuring dumb loopers not colleagues… If we do
> this right, we crush benchmarks, but more importantly we have infinite utility, and
> an ability to self improve."*

This plan is the build order for that. It is not a benchmark plan. Benchmarks are the
first consumer of a **general** capability: an activity whose recipe carries rules, a
team that works cards inside it, and a held-out oracle that marks the result.

**Status:** plan. Every slice below carries the probe that proves it, and no slice is
"done" on a compile or a unit test.

---

## 0. Read these first, and do not re-derive them

Three days of driver-hours have been spent re-deriving decisions that were already
written down. The two source docs are correct and this plan implements them:

- **[ROUND-LIFECYCLE-AS-RECIPE-OWNED-STATE-MACHINE.md](../architecture/ROUND-LIFECYCLE-AS-RECIPE-OWNED-STATE-MACHINE.md)** (#371) — the stage machine, the three laws, the build order. §6 of that doc is slices 3–6 here.
- **[BENCHMARK-AS-KANBAN.md](../architecture/BENCHMARK-AS-KANBAN.md)** (#346) — task→card→claim→artifact→verdict, and the delivery/judgment line.
- **[BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md](../architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md)** — the law, and the acceptance test.

### ALREADY BUILT — rebuilding any of these is the failure mode

| capability | where | note |
|---|---|---|
| `BenchmarkAdapter` trait + live registry | `cognition/benchmark.rs:89` | `tasks()` + `grade()`; "each benchmark is an adapter" is ALREADY TRUE |
| `TaskOutcome` with BOTH artifact channels | `cognition/benchmark.rs` | spoken answer *and* workspace diff |
| task → card → per-run room | `commands/benchmark.rs` (`benchmark/dispatch`) | THE one adapter into kanban |
| round as an entity, keyed by run room | `cognition/bench_round.rs` | stages are the gap, not the entity |
| card-state → bus event | `modules/benchmark_grade.rs` (#450) | the emitter the stage machine subscribes to |
| recipe as data: regions, affordances, params, **roles** | `experience/recipe.rs` | `CitizenRecipe { role }` — roles already authored data (#430) |
| `ProofSpec` on an affordance | `experience/mod.rs:309` | `None \| CleanLane \| Attestation` — the hook a verdict extends |
| claims survive long work | `e3b1a0f98` | presence pump renews held claims |
| staged-workspace resolution | `persona/staged_workspace.rs` | ONE resolver, three callers |
| act-question glass box | `persona/service_loop.rs` (`persona.work.gate`) | shipped 2026-08-18 |

**Before adding any type, grep for it.** `[[grep-for-the-mechanism-before-proposing-to-build-it]]`.

---

## 1. The shape, in one picture

The benchmark touches only the two ends. The middle is an ordinary room.

```
   IN (recipe projects)                                    OUT (oracle marks)
   ────────────────────                                    ─────────────────
   adapter.tasks()                                         adapter.grade(task, artifact)
        │                                                            ▲
        ▼                                                            │
   ┌──────────────────────────── ACTIVITY ROOM ────────────────────────────┐
   │  recipe RULES: per-role instructions, objectives, who drives          │
   │  cards on the board          team claims, splits, delegates           │
   │  real hands, real workspace  every continuum feature available        │
   └───────────────────────────────────────────────────────────────────────┘
                                     │
                          artifact (diff / answer)
```

**The generalization is the point, and it is free:** swap the oracle and the same
activity is ordinary work. Held-out tests → SWE-bench. CI green → a real PR. Customer
accepted → a real deliverable. The benchmark was never special; it is the case where
the oracle happens to be a sealed test set.

### The law that protects the number

**Delivery and lifecycle belong to the card. Judgment belongs to the oracle.**
(BENCHMARK-AS-KANBAN, "the line that must not blur".)

- A citizen moves her card through *working* states. That is honest self-report.
- A citizen **never** sets the terminal state on a graded card.
- The scorer is **not a role**. It is deterministic and held out. A persona rendering
  the verdict could be talked into a pass, and the number would be worthless — that is
  the difference between crushing a benchmark and self-grading one.
- **Verdicts flow one way.** Nothing downstream of a verdict may revise it, and the
  scorer never reads the room.

### The teacher is OPTIONAL (Joel, 2026-08-18)

A *teacher* role — reads the verdict, explains the failure, proposes the N+1
correction, yields the training pair — is real and wanted, but it is **per-recipe and
deferred**. Everything below works with or without one. Do not block on it. When it
lands it is a `CitizenRecipe` role like any other, strictly downstream of the verdict.

---

## 2. The discipline: probe-first, or it did not happen

Tonight's worked example, and the reason this section exists.

The act-question gate in `service_loop` had **five conditions and emitted nothing when
it declined**. Three of them were defects:

1. `!directed` — the work turn fired only on an *undirected* turn, while dispatch
   actuates with an *addressed* imperative. The actuation path and the work gate were
   **mutually exclusive by construction**.
2. `InProgress`-only — claiming leaves a card `Claimed`; the gate demanded the state
   that starting work is what produces. **Circular.**
3. The gate sits *inside* the speak-pass branch, so it is only reached when she
   declines to talk. Found only after the first two were fixed.

Five hypotheses fit the evidence identically for an entire session, because the branch
was silent on every path but the taken one. **A gate whose refusal is invisible is a
gate nobody can debug.**

**So, for every slice below:**

- **Emit on the negative path.** Any decision that can decline must say so, with its
  inputs, always. `decision=` is a required field, not a nice-to-have.
- **The probe is named in the slice.** A slice is done when its probe row appears live
  with the right fields — not when it compiles, not when a unit test passes.
- **Measure deltas from a timestamped baseline.** Probe totals survive reboots; a
  cumulative count cannot show a rate change. This produced two vacuous "proofs".
- **Discover classes, never guess them.** Query `--class='*'` and read what is there.
- **An absence is an unfinished measurement.** Zero rows means the instrument is
  unproven. Prove the instrument, then read the silence.
- **Positive-control every claim.** For a fix: an event that previously failed and now
  succeeds. For a scorer: the gold patch.

---

## 3. Slices

Ordered smallest-true-cause first. Each is independently useful and independently
provable. **Do not start slice N+1 until slice N's probe gate is green live.**

### Slice 1 — the act-question comes from the RULES, not from a nested negative

**Why first:** it is the live blocker. The work turn has never fired for a bench card,
and the current gate is three-deep in accidental preconditions. Every downstream slice
measures a room where nobody works.

**What:** the question "you hold work in this activity — work it?" becomes a
first-class turn the activity's rules ask, not a side effect of declining to speak.
Retire the nesting; keep her freedom to pass (the answer stays hers — this adds a
question, never an instruction, per `[[no-hardcoded-heuristics-to-steer-cognition]]`).

**Files:** `persona/service_loop.rs`.

**Probe gate:**
- `persona.work.gate` rows appear on turns where she **spoke** (not only on passes) —
  proving the question is asked independently of the speak decision.
- `persona.turn.work` ≥ 1 with a held card.
- `persona.work.hands_rooted` names the staged workspace.
- `git diff` in that workspace is non-empty.
- hands restored afterward (no #312 leak).

**Falsifier:** if `persona.work.gate` shows `held=0` while the board says she holds a
card, the defect is claim visibility, not the gate — go there instead.

### Slice 2 — the round pulses while it works (law 2)

**Why:** liveness today is a file mtime written once per attempt, against attempts that
legitimately run hours. The projection whose stated purpose is *"silence must never be
ambiguous with progress"* structurally cannot tell them apart, and it has already
flagged a healthy run `quiet`.

**What:** a heartbeat consuming `WorkspaceCycle::actions_taken()` (the seam exists,
`c9ba5f943`) so `acts` and last-activity are live at the cadence work happens.

**Files:** `cognition/bench_round.rs`, the run projection.

**Probe gate:** `bench.round.pulse` rows arrive at working cadence during a live run,
with a monotonically climbing `acts`; a genuinely idle run stops emitting and is
distinguishable from a working one **without reading a file**.

### Slice 3 — the round entity owns STAGES

**What:** `BenchRound` gains `stage: STAGING | READY | WORKING | GRADING | DONE`, and
the transitions land as probes. Entity exists; the stage field and subscribers do not.

**Files:** `cognition/bench_round.rs`.

**Probe gate:** `bench.round.stage` fires once per transition, in order, naming the
**emitter** — never a timer. A round that reaches DONE emits exactly one DONE.

### Slice 4 — transitions come from the components that KNOW (law 1)

**What:** env builder → `STAGING→READY`. Supervisor (hosted + serving ready, #442) →
gate open. Work board first claim → `WORKING`. Card store (#450, already event-driven)
→ `GRADING`. Round entity all-settled → `DONE`.

**Probe gate:** each transition row carries the emitting component. **Zero timeouts,
zero polls, zero agent judgement anywhere in the path** — grep the diff for `sleep`,
`interval`, and retry windows as the review gate.

### Slice 5 — `RoundViewState` on the pipe

**What:** fold the round entity onto the same ViewState pipe humans and citizens read.
Retires the 5s progress-directory poll in `positron_bench_source`.

**Probe gate — the acceptance test, from the law doc:**

> *Can a citizen standing in the room perceive the run's state through the same
> ViewState pipe the human's screen uses?*

Proven by a citizen answering "what stage is this round in" from perception alone, and
by a fresh driver answering *is it ready / has it started / is it stuck / is it done*
with **queries only** — zero log reads, zero probe archaeology, zero inference from an
absence.

### Slice 6 — dispatch consumes the state pipe (#442)

**What:** dispatch refuses to stage into a room that is not READY, and says why.
Expressed as a state, not a check.

**Probe gate:** a dispatch attempted during a serving transition is **refused with a
reason**, and the same dispatch succeeds once READY. Both rows visible.

### Slice 7 — the rules half of the recipe

**What:** `ExperienceRecipe` gains the authored rules it never had:
- per-role **instructions** and **objectives** (a worker gets a charter, not just a card)
- **who drives** the work — retiring the `WorkDriver` enum into recipe data
- the activity's **outcome**, recipe-owned rather than `benchmark_grade.rs`-owned

**Files:** `experience/recipe.rs`, `experience/mod.rs`, `recipes/*.json`.

**Probe gate:** a citizen's capture contains her role's instructions verbatim
(`[[READ-HER-CAPTURE-first]]` — read the input, never a probe's prose about it), and
two recipes with different rules produce different behaviour from the same code path.

**Note:** the `WorkDriver{DetachedSolve|Citizen}` enum shipped 2026-08-18 is a
*stopgap* — a Rust enum deciding what the recipe should say. It retires here. Its one
lasting contribution is the round-open-before-first-card ordering, which stays.

### Slice 8 — `ProofSpec::Verdict`: the oracle as an affordance

**What:** the scorer becomes a declared affordance yielding a verdict proof, alongside
`CleanLane` and `Attestation`. Deterministic, held out, one-way. `adapter.grade()`
already exists and is not rebuilt — this is the declaration and the wiring.

**Probe gate — positive control, non-negotiable:** the **gold patch** scores resolved
in every env class in play. A 0 must mean "the model failed", never "the env lied".
Report the ungradeable count explicitly (#383/#380: 114/300 → target 0).

### Slice 9 — the number

Full round, citizen-driven, on a deploy-verified build with hosted citizens, gradeable
envs, and a round that ends. Report resolved/total with per-instance receipts and the
patch sha for each pass.

---

## 4. Forbidden moves

Each of these has actually happened and cost a session.

- **A parallel runner.** If you are adding a field to a benchmark probe so an external
  consumer can parse it better — stop. The consumer should not be external.
- **A timeout, retry window, sleep, or agent-side heuristic to decide a stage.** That
  is the ritual growing back. Ask: *which component already knows this, and why isn't
  it saying so?*
- **A second adapter/registry/allocator.** `BenchmarkAdapter` exists. `bench_round`
  exists. `staged_workspace` exists.
- **A silent decline.** Any new gate emits on its negative path or it does not land.
- **Reporting a state you did not observe.** Say "I did not observe it," and name the
  query that would.
- **A verdict that flows backwards.** Nothing downstream of the oracle may revise it.
- **Grading speech.** The artifact is the evidence; a citizen describing a fix that
  never touched disk is our measured failure mode.

---

## 5. Standing rules

- `export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"` before any cargo.
- Commit, **then** deploy. Verify the running SHA == git HEAD.
- `df -h /` after each cargo cycle; sweep ghost target dirs under 20 GB free.
- Never `--no-verify`. Canary is the branch; main merge needs Joel.
- Restart freely — hesitating to reboot is the defect. But say when a live round dies
  for it.
