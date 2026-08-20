# Benchmarks That Learn — the plan to a number we can defend

**Objective (Joel, 2026-08-18):** *"The learning and score are both the objective. We
should outcompete the base model."*

That is one falsifiable claim, not two goals:

> **A citizen on base X, with her genome, beats bare base X on the same benchmark.**

Not "beat a frontier model." Beat *the model we are running on*. If that delta is not
positive, the cognition + genome stack is not earning its keep — and that is a real
answer, not a failure to measure. Everything below exists to make that sentence
measurable and then to move it.

---

## The four threads, and why they are ordered this way

| | thread | what it delivers | gates |
|---|---|---|---|
| **A** | bench work reaches the genome | the LEARNING half | A2 gates A3 |
| **B** | base-model control | attributable scores + a training target | B1 gates B2 |
| **D** | the score ceiling | turns that can actually finish | **D gates C** |
| **C** | the measurement | the claim itself | needs A, B, D |

The non-obvious ordering constraint: **D gates C.** If turns still burn their whole
output budget inside `<think>` and emit no tool call, both arms of the comparison score
~0 and the delta is noise. You cannot measure a difference between two zeros. Do not run
C until D's gate is green.

---

## Thread A — bench work reaches the genome

**Why it was impossible.** `training_producer::produce` — the L2 step that turns a turn
into a `(context, completion)` training example and submits it to the forge — had exactly
ONE call site tree-wide: the message-**reply** path (`service_loop.rs:1474`). Its own
comment states the exclusion: *"It lives ONLY on this live `Spoke` path, which eval forks
(`drive_to_settle`) never run, so the training set can never be contaminated by a
measurement simulation."* Correct for eval (#59). Consequences:

- `agent/solve` IS `drive_to_settle` → a dispatched bench card produces **zero** training
  examples, regardless of `learn: LearnFromThisWork`. The gate is PATH-based; the policy
  is never consulted; **the policy loses to the path.**
- The **work turn** — the separate drive where she works her *claimed card* (#154,
  "working is not speaking") — also never fed the producer. **Chat produced training
  data; real work did not.**

### A1 — work turn roots hands + feeds the producer ✅ SHIPPED (`18ecef781`)

- hands root at the held card's staged workspace, restored on every exit path (#312)
- the producer fires on the work turn: context = card burst, completion = her report
- `persona/staged_workspace.rs` — ONE answer to "which workspace does this card point
  at", replacing the inline walk in `persona/roster` and `dispatch_staged_swe_solve`

**Unproven.** Compile + unit green only.

### A2 — LIVE-PROVE A1 ← *next action*

Reboot → citizens resident → one staged SWE card in a citizen's hands → observe, in order:

1. `persona.work.hands_rooted` probe naming the instance workspace
2. acts executing **inside** that repo (a `git diff` in the sandbox is non-empty)
3. a training submit reaching `genome/training-trigger/submit`
4. her hands restored afterward (`code/read` on her own workspace, not the exam repo)

**Gate:** all four. (3) is the one that has never happened for bench work.
**Falsifier:** if (3) does not fire, the producer's quality bar is rejecting work reports
— read `score_interaction_quality` against an actual report before assuming a wiring bug.

### A3 — retire the `agent/solve` inbound bypass

`benchmark/dispatch` stops calling `dispatch_staged_swe_solve` directly; the kickoff/claim
drives her turn. **Keep** `agent/solve` as the OUTBOUND primitive it was built to be
(external harnesses drive our agent); only the inbound self-call goes.

Its own comment admits the bypass was deliberate: it fires solve directly *"rather than
depend on her re-deriving a `work/claim` from a chat kickoff (the fragile hop that stalls
every run)."* That hop is what #455 (residency) and #452 (boot owns the tree) fixed — so
the reason for the bypass is now largely gone. **Do not do this before A2**; retiring it
onto a path that cannot reach the repo would be strictly worse than today.

Folds in **#453**: once she works in her own loop, the model is whatever she is served on,
and `base_model_id` as a required param stops existing.

### A4 — the round ends (#371)

staging → ready → working → grading → done, each transition emitted by the component that
knows. Without it a round is not repeatable, and an unrepeatable number is an anecdote.

---

## Thread B — base-model control

### B1 — find the chooser ← *blocking, bounded read*

`persona/host.rs:367` → `self.spawner.plan()` → `plan_rows.first()` → `desired.model_id`.
That is what picks the served model. **Not** `allocator::resolve_model_for_persona`, which
is why the catalog says `qwen3.5-4b-code-forged` while serving runs Devstral.

Read what backs `spawner.plan()` (#430 made the roster recipe data — likely there).
**Until this is known, pegging the allocator pegs a thing that is not deciding** — a green
test on a dead path, the same shape as three defects already found this session.

### B2 — cut serving + training onto `BaseModelPolicy` (`07c67e434`, `836677021`)

The type is landed and deliberately unwired. It collapses **4 → 1**:

| was | now |
|---|---|
| `override_model` (runtime assignment) | `Pegged{reason: Measurement{run_id}}` |
| `model_preferences` (tiered ladder) | `Adaptive{ladder, floor, rungs}` |
| `model_id` (labelled *"Legacy"*) | `Pegged{reason: Operator}` |
| `default_local_model` | **deleted** — that arm WAS the #438 downgrade |

Measured on the real catalog: **behaviour-neutral today** (every local entry has one rung
at `min_vram_gb = 0`; the `default_local_model` arm is only reachable by non-local entries
that never enter the resolver). The new refusals first bite when the Qwen ladder lands —
which is the point.

**Constraint (Joel):** the policy is an INPUT to the governor, never a replacement. The
existing code already draws this line — *"the allocator's budget gate — the override only
changes WHICH model, never whether it fits the host."* `ResourceGovernor` keeps VRAM,
pressure, ports, grid placement. Untouched.

### B3 — author the Qwen ladder as data

```
Qwen3.8-27B      ← top rung
  ↓ smaller Qwen forms
floor: smallest form we stand behind
```

The 64GB M5 lands on the top rung **automatically** — no peg, no special case. A 16GB box
steps down the same ladder. That is the dynamic system; the floor only stops it sliding
past the bottom into a 0.5B (#438).

**Nuance:** a same-family ladder does **not** make adapters portable. Each rung needs its
own forge run. What the family buys is consistent tokenizer/chat-template across rungs, so
one corpus format feeds every forge target.

### B4 — forge fan-out

One corpus → `{adapter@27b, adapter@7b, adapter@1b}`, **targets read from her ladder**, not
from a typed flag. `RungPolicy::RequireGenome` (the default) then makes an unforged ladder
VISIBLE instead of a silent capability cliff: declare four rungs, forge one, and she
resolves to the one that exists — and the refusal names the missing forge targets.

This is also what makes the peg load-bearing rather than bureaucratic. Continuous learning
never stops, so `PegReason::Training{job_id}` is the peg held **most of the time**: if her
base floats between the corpus accruing and the forge finishing, the adapter lands for a
base she has already left. Corpus survives; the training compute does not.

---

## Thread D — the score ceiling (**gates C**)

Two known caps, independent of everything above.

### D1 — turns that never emit a tool call

`completion_budget_for(window) = window/4`, so a 16k window caps generation at 4,096. A
reasoning model exhausts that inside `<think>` and never reaches the tool call. Measured:
7/20 captures at `finish_reason: length`, `output_tokens: 4096` exactly, empty text.

**Two fixes that are wrong and must not be retried:** capping `<think>` (shrinks the
model; Joel rejected it), and raising the fraction alone (breaks
`prompt_plus_completion_cap_never_exceeds_the_served_window` → a 500 on every turn).

The real move is `reserve = min(desired_share, window − mandatory_floor)` — the reserve
yields to the floor. But `/4` is load-bearing in **six** places, so it is four ordered
steps (collapse the duplication first, at the unchanged fraction; move sub-floor test
windows to a real one as a stated PREMISE change; make the reserve yield; then raise the
share). Detail in the prior plan; do not one-line it.

### D2 — grading

May be largely resolved — #383 reports django grading with 7/8 env classes gold-gate green.
**Do not quote the old "114/300 ungradeable" as current.** Re-measure: a gold patch must
pass in every env class, or a 0 means "the env lied", not "the model failed".

**Gate for D:** `finish_reason: length` empties fall measurably from a timestamped
baseline, AND a gold patch passes in every env class in play.

---

## Thread C — the measurement

Only run after D's gate. Then:

| arm | policy | label |
|---|---|---|
| control | `Pegged{base, Measurement{run}}` + `RungPolicy::AllowBare` | `genome_backed: false` |
| treatment | same peg + her forged adapter | `genome_backed: true` |

1. **C1** — control arm on the pegged base. Score.
2. **C2** — the loop runs; corpus accrues from real work (Thread A).
3. **C3** — forge onto **that same base** (Thread B4).
4. **C4** — treatment arm, same peg. Score.
5. **C5** — delta.

`ResolvedBase.genome_backed` labels every score so a lift comparison can **refuse** to
compare across that line. The peg stops the base drifting between arms. Without both, the
delta measures a base swap and calls it learning.

**What would falsify the whole thesis:** delta ≤ 0 with D's gate green and both arms on the
same base. That is a real result and it should be published as one.

---

## Standing rules for this work

- **`export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"`** before any cargo.
- **Commit, then deploy.** A fix you cannot prove reached the running binary is a fix you
  have not made — verify the SHA against git HEAD.
- **Measure deltas from a timestamped baseline**, never cumulative probe totals (they
  survive reboots; this produced two vacuous "proofs" already).
- **An absence is an unfinished measurement.** Four times this session a "zero callers /
  never fires" reading was wrong on the first look. Grep the verb STRING and the module
  path before concluding a wire is dead.
- **Never `--no-verify`.** Canary is the branch; main merge needs Joel.
