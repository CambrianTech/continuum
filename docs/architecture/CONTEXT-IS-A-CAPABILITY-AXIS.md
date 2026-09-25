# Context is a capability axis, and the governor must learn its floor

**Joel, 2026-08-17:** *"Context windows are as important as model size. It seems 16-20k
is bare minimum for decent activities. Ideally the governor learns."*

## The claim

Delivered capability is a function of **two** variables — parameters and served window —
and the serving planner models only the first. `ModelFootprint::capability_rank` is a
scalar keyed to model identity alone. Under that type, a 27B is always "more capable"
than a 14B, even when the host can only serve the 27B a 2,048-token window and could
serve the 14B 32k.

That is not an abstraction quibble. It shipped as a live defect on the M5 (fixed in
`03c890b29`): the planner crowned a 27B at `usable_gb=5`, served it **2,048 tokens**
against a **measured 63,817-token demand**, and every SWE act ran against a context too
small to hold the task statement. The window collapse was invisible to the ranking that
caused it.

## What is already right (do not rebuild it)

`cognition/working_set.rs` is a well-built learning loop and its hard problem is already
solved. Read it before proposing anything here.

- It records **DEMAND, not USAGE** — the counterfactual "what would this turn have used
  with no budget at all": framing + the full conversation before trimming + *every*
  grounding contribution offered, including the ones assembly dropped + generation
  reserve. This deliberately avoids the thermometer-inside-the-thermostat trap: a p95 of
  what-was-*sent* re-derives the clamp that produced it and freezes it forever.
- It is **peak, not average** — a working set is the high-water mark at which an activity
  stops being strangled; averaging a coding turn with idle chatter serves neither.
- It **persists** per persona and rehydrates across reboot.
- It is **passed as a parameter**, never read from a global inside a decision.

The `demand_window: 63817` observed live is this module working correctly. A demand that
exceeds the window is the signal that the window is too small — and the only signal that
can ever grow it.

## The actual gap: one learned signal, three decisions, two of them deaf

| Decision | Signal today | Should be |
|---|---|---|
| Served window | **learned** (measured p95 demand) | learned (fast) |
| Lane count | `BOOTSTRAP_WORKING_SET` (16,384) | learned (slow) |
| Model choice | `BOOTSTRAP_WORKING_SET` (16,384) | learned (slow) |

Both structural decisions use a hardcoded constant whose own doc says it "was never meant
to survive." And note where that constant sits against Joel's read: 16,384 is the *bottom*
of the 16–20k "bare minimum" band. So the current floor guarantees only bare adequacy, and
guarantees it identically for a chat turn and a SWE turn.

## Why the constant is there, and why that reason does not forbid learning

The static value is not laziness. Coupling lane count to a moving demand signal produced
the **718-replan flap**: `usable_gb` swinging 26→6, lanes oscillating 1↔2, every flip
resizing the live admission semaphore and prefill throttle under in-flight requests — the
`no response headers for 300s` wedge that killed three benchmark runs.

That is an argument against driving a **structural, expensive-to-change** decision from a
**fast, jittering** signal. It is not an argument against learning. The resolution is two
time constants:

- **Fast signal → window.** Per-turn measured demand sizes the served window. Cheap to
  change; already built; already correct.
- **Slow signal → structure.** A hysteretic, long-horizon learned floor drives model
  choice and lane count. Expensive to change, so it must move rarely and with a dwell
  time and a margin band, never on a single sample.

A slow floor cannot flap, because flapping is a property of the update rule, not of
learning.

## Proposed design (NOT built — this doc is the design, not a report)

1. **`capability_rank` becomes 2-D.** Rank candidates by delivered capability
   `f(model, window_it_would_get_on_this_host)`, not by a static scalar with a floor gate
   bolted on. The current fix (most-capable-that-clears-a-full-turn, else degrade) is a
   correct *approximation* of this and is a fine intermediate state — but it is a gate,
   not a model of the tradeoff, and it cannot express "a 14B at 60k beats a 27B at 20k."
2. **The floor becomes learned and slow.** Derive it from the existing
   `WorkingSetRegistry` rather than from `BOOTSTRAP_WORKING_SET`, with: a hysteresis band
   (only move the decision when the learned value crosses by a margin), a minimum dwell
   time, and a hard lower clamp so it can never learn its way below one real turn. The
   bootstrap constant survives as the *floor of the floor*, not as the value.
3. **The floor becomes per-activity, not global.** A chat turn and a SWE turn have
   different working sets; one global floor serves neither well. Recipes are already data
   (#433 parameterized recipes, the activities catalog), and "recipe = content-type +
   RULES" makes the minimum useful window a recipe-owned property. Learn the demand
   distribution *per activity class*, not just per persona.

## Acceptance tests

- A host that can serve model A at 60k or model B at 20k, where B outranks A statically,
  picks by delivered capability — and the test states which and why.
- The learned floor, driven by a synthetic demand trace that oscillates, moves at most
  once across the trace (anti-flap, pinned as a test, not asserted in prose).
- The learned floor never drops below the hard clamp regardless of input.
- A recipe declaring a large working set gets a larger floor than a chat recipe **on the
  same host**.

## The third axis: speed, and model choice by activity (2026-09-25)

**Joel, 2026-09-25:** *"If we are doing code, and we have a viable 5090 we want a 27b
selected, but if it is shifting into an academy learning mode then the challenge of
thinking of these modes. What's ideal? Run through a series of states and idealism, then
it helps make sure the algorithm works. We don't hand rig unless we're just needing to
test something specific."* And: *"Genome has to match the model being trained for LoRA
unless we wrote a way to transfer."*

### The claim

For interactive work, delivered capability has a third input: the decode rate this host
actually achieves on the model. A model that fits but cannot keep a turn inside the
latency law (a 1.5x tax is fine; 5-10x disqualifies) delivers less than a smaller model
that can. Measured on the M5 (M5 Pro, Metal), from `state/decode-knee.json`, per-stream
tokens/s by lanes in flight:

| Model | 1 lane | 2 lanes | 3 lanes | 4 lanes |
|---|---|---|---|---|
| Qwen3.8-27B (Q4_K_M) | 7.5 | 6.2 | 3.0 | 3.3 |
| Ornith-1.5-35B-A3B | 32 | 44 | — | — |

The 27B never clears `DECODE_FLOOR_TPS` (10) at any lane count on this box. The decode
knee (`inference/decode_knee.rs`) is working. It clamps LANES from this curve, but the
planner still picks the MODEL as "the most capable that fits the GPU budget," and fitting
is not the same as serving well. That day on the M5, each coder act took 7-9 minutes
(`persona.act.pace` rolling mean 545 s), a turn held one or two reads, and 10 residents
produced zero writes in an hour.

### What each activity demands

| Activity kind | Capability | Speed | Window | Lane ownership |
|---|---|---|---|---|
| Coding (cards, benchmarks) | high | interactive: an act lands in seconds | large (repo context) | citizen turns |
| Academy: teacher synthesis | highest available | batch: slow is acceptable | moderate | borrowed via `run_teacher_batch` |
| Academy: LoRA training | n/a (trains the persona's base) | batch | n/a | trainer holds the card; serving steps aside |
| Dream / idle consolidation | low | background | small | whatever is free; never forces a reload |
| Conversation | medium | interactive | small | citizen turns |

### The state walk (what the algorithm must produce, never hand-pinned)

1. **Coding, 5090 on the grid.** Coder turns go where capability and speed both hold:
   the 27B on the 5090 (71,680 window, CUDA). The M5 serves the lighter roles (review,
   chat, orchestration). This is the "27B selected" case.
2. **Coding, M5 alone.** The 27B cannot keep an act interactive here. The ideal is the
   most capable model whose measured decode clears the floor at >= `MIN_KNEE_LANES`,
   which on this box is Ornith.
3. **Academy on the 5090 (single lane).** The teacher borrows the lane with the strongest
   model. Training then takes the card. Residents' coding turns are served on the M5
   meanwhile, so the grid keeps them working.
4. **Academy on the M5.** The teacher is the 27B even at 7 t/s. Batch teaching is judged
   on quality, so the speed floor does not apply.
5. **Dream.** Runs in idle periods on whatever model is resident, and never triggers a
   model swap.

### The rule

For each activity kind, pick the most capable candidate subject to that kind's floors:

- **Interactive kinds** (coding, conversation): the model's measured per-stream decode at
  the knee must clear the speed floor, and its served window must clear the activity's
  learned working set (this doc's second axis).
- **Batch kinds** (teacher, training): capability only. Speed is a cost, not a gate.
- **An unmeasured model** is eligible until measured (decode-knee exploration). A model
  with no curve cannot be ruled out by a speed it never had the chance to show.

The same measurements already exist. The knee ledger records every model's curve, and
the working-set registry records demand. The change is that model choice reads them,
through the slow, hysteretic path this doc prescribes, so it can never flap.

### The genome couples model choice to identity

A LoRA adapter is trained against one base model and applies only to it. So the choice
of base is also a choice of which of a persona's learned skills she can use. Consequences
the rule must honour:

- **A persona's candidates carry her genome.** Serving her on a base where she holds
  trained adapters delivers more than a stronger bare base, by exactly the skill the
  adapters encode. A swap away from that base forfeits them. Rank must price that loss,
  not ignore it.
- **Train on the base she will be served on.** The trainer must target the base her
  dominant activity will select, or the adapter it produces is dead weight. If the state
  walk above puts M5 coders on Ornith, M5 coder genomes are trained for Ornith.
- **The curriculum is the portable genome. Adapters are compiled per base.** This is the
  foundry-as-JIT shape in [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md): the
  datasets and graded turns a persona earned transfer to any base, and an adapter is
  that curriculum compiled for one architecture. Until a direct transfer exists,
  re-training from the same curriculum on the new base IS the transfer, scheduled as a
  training period like any other.
- **Direct adapter transfer is NOT built.** Nothing in `genome/` maps an adapter across
  bases; the only prior thinking is research-grade (cross-model head transplant in
  [SENTINEL-AI-NEURAL-PLASTICITY.md](../papers/SENTINEL-AI-NEURAL-PLASTICITY.md)). Joel has
  ideas for this; they belong here when written down.

### Acceptance tests for the rule

- **State 1 vs state 2:** the same roster and recipe, planned with and without a 5090
  capacity beacon, selects the 27B for coding on the 5090 and a floor-clearing model on
  a lone M5. The test states the measured curves it used.
- **Batch ignores the floor:** a teacher period on the M5 selects the 27B although it
  fails the interactive floor.
- **Unmeasured is eligible:** a model with no curve is not excluded by speed.
- **Genome pricing:** a persona holding adapters for base A is served on A over a bare
  base B that outranks A statically, until B's advantage exceeds the adapters' measured
  value. The margin is stated in the test.
- **No flap:** a decode curve oscillating around the floor moves the model choice at most
  once across the trace (the same hysteresis contract as the learned window floor).

**Status:** design, not built. Owner of the planner change: the serving lane (Claude, M5).
Academy and training periods: BigMama (5090), per the standing-periods actuator. Adapter
transfer: open; Joel's design.

## The smell to catch yourself on

If you are adding another constant next to `BOOTSTRAP_WORKING_SET`, stop — that is a third
way to express a floor, and the de-hardcode guard exists to catch exactly that shape (it
already caught `FLOOR_TOKENS`, #411). There should be one floor, learned, with a clamp.

Related: #438 (governor downshift on a bogus sample), #234 (demand-derived lane `-c`),
#213/#214 (window floors and dead grow-back), #124 (de-hardcode the dynamic system),
#441 (throughput sentinel — currently emitting nothing, see below).

## Blocking observation for anyone measuring this

`delib.generate` emits **zero** probe rows on this box. That is the class that would carry
per-generation latency and tok/s. Its absence is why every throughput question in this area
has to be answered by black-box sampling over 20-minute windows instead of read off the
stream. Fix that before trying to tune anything by measurement — an unmeasurable governor
cannot be a learning one.

---

## The blocker on raising the generation reserve (measured 2026-08-17)

`completion_budget_for(window) = window / 4`. On a 16,384 window that caps generation
at 4,096 — and a reasoning model spends output tokens THINKING before it answers, so it
exhausts the cap inside `<think>` and never reaches the tool call. Measured: 7 of 20
captured turns came back `finish_reason: length` with `output_tokens: 4096` EXACTLY,
~15k chars of reasoning, empty text, ZERO tool calls, 4–5 minutes of GPU each.

**Do NOT "fix" this by bounding the reasoning channel.** llama-server offers
`--reasoning-budget N`; using it makes the model smaller to fit a fraction we invented.
Their ability to think is the product (Joel, 2026-08-17: *"So blown away their ability
to think with more capping. Lame."*).

**And do NOT just raise the fraction.** Tried `window/2`; it breaks
`prompt_plus_completion_cap_never_exceeds_the_served_window` — the invariant that keeps
`prompt + completion` under `n_ctx` (llama-server runs with context-shift off, so
crossing it is a 500 on every turn, i.e. every citizen muted).

**The real defect, from reading the sizer.** `prompt_view_within` derives
`budget = context_window − completion_reserve − describe_tool_tokens()`, which is
correct. But three sibling tests name content that must survive budget pressure
unconditionally — the held work card, the most recent burst, the newest message. Those
are INCOMPRESSIBLE FLOORS. When the reserve grows, the budget shrinks below the floor,
and the packer admits the mandatory content anyway. Observed at window=1024:
prompt 525 + completion 512 > 1024. The overshoot IS the floor refusing to compress,
which is correct behaviour — the reserve is what's wrong to hold fixed.

**The fix shape:** the reserve must YIELD to the floor —
`reserve = min(desired_share, window − mandatory_floor)`. Then the invariant holds at
every window (including synthetic sub-`MIN_SERVE_CTX` ones the tests use and production
never serves), and the share can be generous at real 16k+ windows where the floor is a
few hundred tokens against thousands.

**Why it isn't a one-liner:** this is circular — reserve → budget → packing → floor →
reserve. The floor must be computable BEFORE the reserve is chosen, which means hoisting
the mandatory-section measurement ahead of budgeting (or a two-pass size-then-resize).
That is a real refactor of `prompt_view_within`, not a constant change, and it must not
weaken any of the four tests: they are the only thing standing between a generous
reserve and a 500 on every turn.

**Sequence for whoever picks this up:** hoist the floor → make the reserve yield to it →
THEN raise the share → re-run all four `prompt_shaping` tests at both a synthetic small
window and a realistic 16k one. The share becoming a policy knob (and eventually
learned, per this document) only makes sense after the floor is load-bearing.
