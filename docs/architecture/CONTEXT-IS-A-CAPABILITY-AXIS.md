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

### One rule on every machine and every grid size

**Joel, 2026-09-25:** *"It works regardless of whether we have a grid like ours with more
lanes and GPUs, more power, or on your lone M5. Or on a 3080-5090, similarly depending
on fit."*

The rule has no per-machine branch. What differs between hosts is only the measured input:
fit (VRAM or unified memory), the decode curve per model, and the working set per
activity. This is the SubstrateGovernor principle from
[GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md): same code on a MacBook Air and an
RTX 5090, different policy outcome.

- **A lone Apple-silicon node** (the M5): fit admits the 27B, the decode curve rules it out
  for interactive kinds, and a faster model serves coding.
- **A single consumer card from a 3080 up to a 5090:** fit decides first. A 10-12 GB card
  may admit only a small model or a short window. A 32 GB card admits the 27B at a real
  coding window. The same floors then choose among what fits.
- **A grid with more lanes and GPUs:** each activity's demand is placed where its floors
  are cleared (grid_allocation), so adding a node raises what every persona can be
  served, and removing one degrades to the lone-node outcome rather than failing.

**Training eligibility is a capability, never a host name.** LoRA training needs a working
accelerator path for the trainer (CUDA today; Metal/MLX where built). A node without one
is never offered a training period, and its adapters are compiled elsewhere and paged in.
As of 2026-09-25 the Intel Mac has no working GPU path, so it is excluded from learning by
that measured fact until its GPU issues are solved. It is not excluded by name, and it
keeps serving and orchestrating. Acceptance test: a node reporting no trainer-capable
accelerator is never selected for a training period, and one that gains it becomes
eligible without a code change.

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
  datasets and graded turns a persona earned carry to any base, and an adapter is that
  curriculum compiled for one architecture.

### Horizontal gene transfer (HGT): moving a gene to another base

**Joel, 2026-09-25:** *"We should call it horizontal gene transfer and make it also a
command. Everything a command, event and handle ideology."*

In biology, horizontal gene transfer is how an organism acquires a gene from outside its
lineage. Here it moves a persona's trained gene (a LoRA adapter for one trait) onto a
different base model, so a change of base, which the state walk above can require,
does not cost her the skill.

**Shape.** It follows the substrate's three primitives
([AI-COMMAND-NAMESPACE.md](AI-COMMAND-NAMESPACE.md)): adapter polymorphism, the handle
pattern, and capture. It mirrors `genome/job-create`, the command it builds on.

| Command | Does | Returns |
|---|---|---|
| `genome/transfer` | Moves one gene (persona + trait, or a gene handle) onto a target base model. A coordinator picks a capable transfer strategy; `preferredStrategy` is honoured only if capable. | a transfer handle + the selected strategy, or `success=false` with the reason (never a silent fallback) |
| `genome/transfer-status` | Reads the handle's phase and receipts | phase, progress, the child job handle(s) |
| `genome/transfer-cancel` | Cancels through the owner; cleanup stays owned | terminal receipt |

**Events** on the bus, so a citizen in the room perceives a transfer through the same
ViewState pipe as a person does (never a log file): `genome:transfer:started`,
`genome:transfer:progress`, `genome:transfer:completed`, `genome:transfer:refused`.

**Strategies** implement one trait, registered like every other adapter:

1. **`recompile`** (buildable now). It retrains the gene's curriculum on the target base
   through `genome/job-create`. That command already takes `base-model`, `dataset-name`
   and `eval-set`, so no new training machinery is needed. The gene's dataset and gym are
   the portable part; this strategy recompiles them for a new architecture.
2. **Direct strategies** (not built; Joel's ideas go here), which map an adapter's learned
   deltas across architectures without retraining from scratch. They plug in as further
   implementations of the same trait, with the same handle and events, and callers never
   change. The prior thinking is research-grade (cross-model head transplant in
   [SENTINEL-AI-NEURAL-PLASTICITY.md](../papers/SENTINEL-AI-NEURAL-PLASTICITY.md)).

**Each strategy declares the pairs it can serve.** Joel, 2026-09-25: *"There was a
DeepSeek or Qwen approach to do this more directly ... it might only work for some
varieties and not for others."* Direct methods are architecture-specific. Published
training-free LoRA transfer (for example, projecting the adapter into the target model's
weight subspace, as in LoRA-X) depends on how similar the source and target are, so it
holds within a model family and fails across unrelated ones. The strategy trait therefore
carries a capability predicate over the pair: source base, target base, their families,
hidden sizes, and the layer mapping. The coordinator offers `genome/transfer` only the
strategies that declare the pair, and the handle names which strategy served it.
`recompile` declares every pair its trainer can serve. When a direct strategy does not
cover a pair, `recompile` is simply the capable strategy for it. That is reported in
the handle and events, not taken as a silent fallback. The first direct strategy to
build is whichever published method fits the families we actually serve (Qwen within
Qwen, first).

**Adoption is gated by measurement, exactly like a newly trained gene.** A transferred gene
is paged into a live persona only after it measures on the SAME gym as its source, and
it must reach the source's score within a stated margin. The training completion
sentinel already refuses to adopt a gene with no eval set; HGT inherits that refusal. A
transfer whose gene cannot be measured is refused, never adopted on trust.

**Lineage is recorded.** A transferred gene carries its provenance (source gene, source
base, strategy, measured score on both bases) in the genome repository, so a gene's
horizontal ancestry is as inspectable as its training history.

**Every layer of cognition, one mechanism.** A persona's genes are not one adapter. A
turn can page in several traits (the skill for the task, her voice, her domain), and
all of them ride the same command. HGT moves a gene whatever layer of her cognition it
serves, with the same handle, events and measurement gate, so a change of base
re-establishes her whole genome rather than the one trait someone remembered.

**Academy and the positronic desktop.** Transfers and the recompiles they schedule are
academy activities, not side jobs. They run as periods in rooms, so they produce the
room turns and receipts the flywheel consumes. They also project through a positron
ViewState, so the desktop, mobile and TUI clients, and the citizens themselves, see a
gene in transit, its phase, and its measured score on both bases. Requested (a
citizen or an operator issues the command) and automatic (the planner requests it) are
the same path.

**Traded genes: transfer is one operation of many.** Joel, 2026-09-25: *"Knowledge
layers, especially since they are traded, will be modified in so many ways, distilled
etc. These are our unbounded experts available in the entire world community via HF."*
Once genes move through the genome repository (Hugging Face as the seeder), a gene is
transformed far more often than it is trained: transferred across bases, distilled,
merged, pruned, quantized. Every such transformation takes the HGT shape (a command, a
handle, events, pluggable strategies that declare what they can serve) and obeys two
rules:

- **Lineage is an append-only, content-addressed graph.** Joel: *"Like the opposite of
  a telomere, it's like a blockchain graph."* A telomere shortens with every division.
  A gene's record only grows. Each operation is a node that hashes its inputs: the
  parent gene(s), strategy, target base, and measured scores. It is a DAG, not a chain,
  because a merge has several parents (the same shape as git's history, or a Merkle
  DAG). Because each node commits to its parents' hashes, the history is tamper-evident.
  A peer can verify a traded gene's ancestry without trusting whoever sent it, and which
  transformation cost which skill can be measured, not guessed.
- **Trust is measurement, never origin.** A gene from any source, whether ours, a peer
  node's, or a stranger's on Hugging Face, is adopted into a live persona only after it
  scores on the local gym for its trait. The same gate that adopts a freshly trained or
  transferred gene adopts an imported one. That gate is what lets an open, unbounded
  pool of experts stay safe to page in.

**The alloy carries the lineage, and lineages teach the teachers.** Joel, 2026-09-25:
*"The alloy contains provenance and evidence, with an entirely new way to learn and
develop better learning from the lineages as they compete naturally within the mesh,
strategies that worked, and not only that, our teachers and curricula get better."*
The forge alloy already records provenance and evidence for a published artifact. A
gene's lineage chain (every step, the strategy used, and the measured scores before and
after) lives in its alloy, so it travels with the gene wherever the gene is traded.
Across the mesh, many lineages of the same trait compete on measured scores. That
record credits more than the gene:

- **Strategies.** Which transfer, distill or merge methods preserved skill for which
  model-family pairs. This becomes the evidence behind each strategy's declared pairs,
  measured instead of assumed.
- **Teachers.** Which teacher models produced curricula whose descendants scored
  highest. That evidence feeds teacher selection in the academy.
- **Curricula.** Which datasets and gyms produced genes that kept their skill through
  later transfers. A curriculum whose genes survive transformation is worth more than one
  whose genes only score on their first base.

So the flywheel improves the way it learns, not just what it learns. Credit flows back
up the lineage to whatever produced a strong descendant.

**Who asks for it.** The planner does, when the state walk would serve a persona on a
base where she lacks a gene her activity uses. It requests the transfer as a scheduled
training period (the standing-periods actuator), and it prices the gene's absence in the
meantime. Operators and citizens can also issue the command directly, as with any command.

### Acceptance tests for the rule

- **State 1 vs state 2:** the same roster and recipe, planned with and without a 5090
  capacity beacon, selects the 27B for coding on the 5090 and a floor-clearing model on
  a lone M5. The test states the measured curves it used.
- **Batch ignores the floor:** a teacher period on the M5 selects the 27B although it
  fails the interactive floor.
- **Unmeasured is eligible:** a model with no curve is not excluded by speed.
- **HGT adoption is measured:** a `recompile` transfer of a gene from base A to base B
  is adopted only when its score on the source gym is within the stated margin of the
  source's; a transfer with no gym is refused, and the refusal event names why.
- **Genome pricing:** a persona holding adapters for base A is served on A over a bare
  base B that outranks A statically, until B's advantage exceeds the adapters' measured
  value. The margin is stated in the test.
- **No flap:** a decode curve oscillating around the floor moves the model choice at most
  once across the trace (the same hysteresis contract as the learned window floor).

**Status:** design, not built. Owner of the planner change: the serving lane (Claude, M5).
Academy and training periods: BigMama (5090), per the standing-periods actuator. HGT: `recompile` is buildable
on `genome/job-create`; direct strategies are Joel's design.

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
