# Flow implementation plan: making the concurrent mind real (2026-10-28)

> The design already exists. [CONCURRENT-MIND-AND-GOVERNOR](CONCURRENT-MIND-AND-GOVERNOR.md) §1 and
> §3.1-3.3 and [SUBSTRATE-DOCTRINE-ORGANIC-FLOW](SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md) ("Flow not RPC",
> "Demand not FIFO") are precedence-winning. This plan implements them against the code as it is,
> using a measured inventory of every place that queues or gates work (appendix,
> [FLOW-INVENTORY-2026-10-28](../planning/FLOW-INVENTORY-2026-10-28.md)). The code violated the
> doctrine in 160 places, and the gap between the two is the slowness.
>
> Joel, 2026-10-28: "Its got to flow or have MANY queues"; "Semaphore really only exists for queue
> inserts and signals"; "You can't let inference pile up"; "I'd always take the most recent";
> "Think of live conversations"; "Simulate all cases".

## 1. What the measurements say (M5, Qwen3.6-35B-A3B, 4 residents)

- **The engine is not slow.**
  - Our server matches stock `llama-bench` at 64k context: prefill 322 vs 304 tok/s, decode 30.5 vs 29.5.
  - Four streams decode 101 tok/s together, 3.4x one stream.
  - The engine was occupied 99.6% of the wall clock.
- **Minds waited on gates in front of it.** A 155 s median act held about 50 s of permit waits:
  - a reserved directed lane that was never lent, a median 25.7 s;
  - a prefill throttle guarding a spike one engine never draws, a median 24.6 s.
  - Removing both: act median 155 s -> 80 s, around-model 70 s -> 21 s (4579b4711, live).
- **Removing the next layer without a plan regressed** (90ed24a22, rolled back):
  - the in-flight gauge went dark;
  - directed priority vanished;
  - the wait moved inside the model call (slot wait median 17.2 s, because a reserved scratch slot served 6 of 400 requests).
  - Removing scratch would have switched warm-ahead off (the test caught it).

  The pieces are coupled. They move together, under one design, with a simulator.
- **Context length is the largest per-token cost.** Stock prefill goes 968 -> 304 tok/s and decode 59 -> 29 tok/s from 0 to 64k. The resident median prompt is 82k, and an epoch turnover re-prefills about 65k tokens.

## 2. The model (each line traces to the doctrine)

1. **Content and signals are different things.**
   - Content (chat lines, tool results, perception) appends to the mind's stream in the context codec (`continuum-context`, positron), which consolidates it: superseded items collapse, older ones fold to handles, the newest stays whole (§3.3 ChannelDigest).
   - What flows to the scheduler is only a coalescing signal per mind: "she has fresh input at class X". Twenty lines while she is busy are one wakeup.
   - Distinct committed inputs and tool results are never dropped. They are content, kept by the codec (Codex).
2. **Govern resources, never minds** (invariant 1). A mind always may want. The flow governs only capacities:
   - engine compute (decode/prefill share);
   - KV memory;
   - training memory;
   - CPU workers;
   - external APIs.
3. **One authority per capacity, consumed by every caller** (Codex: "consume one resource authority rather than nesting independent capacity promises"). Memory is admission: `MindMemory::seat` for KV, the training lease for training. Compute is the preemptible share inside it.
4. **Quality-of-service classes**, GCD-style:
   - **interactive**: someone waits on the answer;
   - **work**: her act on her card;
   - **utility**: ambient thought, perception enrichers, gene exams;
   - **background**: dreams, training, consolidation.
5. **Each owner is a serial queue** that targets a class. Her newer signal supersedes her waiting one and keeps her place. Utility and background queues are bounded and shed their oldest. Nothing piles up (Joel; "Backpressure is intrinsic").
6. **One event-driven dispatcher per capacity**, the `continuum-flow` primitive.
   - It owns its ready set, wakes on events (submitted, freed, page landed, aging instant), and never polls.
   - It is a pure scheduler (decisions, testable in microseconds) plus a thin actor.
7. **Two preemption verbs** (Cormac): *yield compute* (stop at a decode step or batch boundary, keep memory) and *release memory* (page out or stop). A mind's act yields compute by finishing its step and pages out only if memory is wanted. A training job pauses at a batch boundary and keeps its graph.
8. **Aging bounds every class's worst-case wait** (Cormac: a paused training job has no guarantee it ever runs).
   - Each class has a stated maximum wait, which the simulator proves and the live read measures.
   - Under permanent interactive saturation (Codex), work still gets a measured minimum share. Interactive cannot starve a mind's own act forever.
9. **Late binding, captured at send** (Cormac).
   - The prompt is rendered when the grant arrives, so she reads the newest.
   - The recorder captures the bytes actually sent, because learning (L1) trains on captured turns.
   - Settle still writes the room turn.
10. **Prefill is an enricher** (§3.1).
    - The context is append-only within an epoch, so appended items are prefilled as they arrive: during a tool call, during a person's speech.
    - At grant time only the newest fragment remains.
    - This is how a live reply starts within its budget.
11. **Waits on predicates, not re-arm loops** (Cormac). "A lane serving base X is ready", "Y bytes of memory free" are conditions the dispatcher evaluates on the events that can change them. The examiner's NotNow and CapacityParked become such waits.
12. **No second scheduler.** The fork's training yield windows, TrainingHolds-as-scheduler, the reserved lanes, the prefill throttle and per-module `max_concurrency` limiters are deleted. The engine executes what the flow decides. Pause stays as an operator override.

## 2a. Stages are short: CBAR's `QueueThread`, in Rust

Joel, 2026-10-28: "Your goal is for modules to make this stuff trivial like cbar's pipeline/*.cpp stages.
They're short." The reference is `cb-mobile-sdk/cpp/utility/Threads.h` `QueueThread<T>` and
`cbar/CBAR_VideoThread` (default `queueSize = 1`):
- `addItem` locks only to pop the oldest until there is room, push the newest, and signal. The
  lock exists for the insert and the signal, nothing else, and the newest item always wins.
- Each stage sets a priority, declares its cadence (`needsRealTime`, `videoOnly`), fetches
  dependencies by type (`getAnalyzerOfType<T>()`), and implements one handler. `CBP_ElevationAnalyzer`
  is 71 lines, most of them boilerplate.

`continuum-flow` therefore ships a **`Stage`** alongside the capacity dispatcher:

```rust
pub trait Stage: Send + 'static {
    type Input: Send;              // what wakes it (a signal, a digest handle, a frame)
    const QOS: Qos;                // its class
    const DEPTH: usize = 1;        // bounded queue; when full, the oldest is dropped (keep-latest)
    async fn handle(&mut self, input: Self::Input);
}
// spawn_stage(stage) -> StageInput<Input>: push() inserts, drops the oldest past DEPTH, and signals.
// A stage that needs a shared capacity (the engine, a TTS model) submits to that capacity's Flow.
```

The module owns every concern that today is copied into each subsystem:
- the queue and its bound;
- keep-latest and coalescing;
- wake-on-signal (no polling);
- class and aging;
- yield;
- shutdown drain;
- probes.

A stage is its handler and nothing else. **The rule for every migration in §5:** the migrated code
holds no lock across an await, no timer, no backoff, no limiter and no queue of its own. The receipt
for each step is lines removed versus added, per subsystem, with stage files measured like CBAR's (most
under ~100 lines).

## 2b. Scheduling math, episodes, and learning the policy (Joel, 2026-10-28)

> "Isn't there math for this? … a traffic light problem with clusters of cars … never leaving
> someone in the left hand turn lane for too long (never a threshold) or just reinforcement
> learning … the things you're measuring as scoring." "Usually you want groupings like code reads
> and edits … like a group of semis." "Just realize this is general for other asks."

**The index, not thresholds: the generalized cμ rule.**
- Each class has a delay cost that grows with waiting, `C_i(w)`: steep for a live reply, shallow for
  a dream.
- When capacity frees, serve the waiter with the largest `μ_i · C_i'(w_i)`, where `μ_i` is
  1 / predicted service time. This is optimal for convex delay costs in heavy traffic; the traffic-light
  form is max-pressure control.
- Priority rises smoothly with the wait, so nobody is stuck in the turn lane, and there is no class bump
  at a quantum (`continuum-flow`'s current step aging is that threshold and is replaced).

**Predicted service time.** A request's cost is known before it runs:
- prefill = tokens to prefill ÷ the engine's rate at that depth;
- decode = the owner's expected reply length ÷ the per-stream rate.

These come from the same calibrated curve the simulator uses. The scheduler sees that a 2k reply costs
~3 s and a turnover ~60 s and orders them. The head-of-line blocking fork #69 fixes inside the engine is
this same mistake.

**Episodes: a concern's working set travels as a convoy and is cleared at its close.**
- Nothing is independent. A concern (a code change, a research thread, a document, a game, a
  conversation thread, a media session) needs a working set loaded for several steps.
- Loading it is a setup cost (the prefill of those reads). Interleaving or preemption pays it again on
  every return.
- **Boundaries come from the activity's own events:**
  - a card claimed or done, a commit, tests green after edits;
  - files or sources that no longer overlap;
  - a topic change in the room, the recipe step's completion, or her own words.
- **Each item an act adds is tagged with its episode.** At close, the episode's bulky items collapse to
  their outline level plus a handle, while conclusions (decision, commit, result) stay whole. That is a
  planned turnover at a natural seam, its re-prefill scheduled while she runs a tool or idles (prefill
  as an enricher), so forced turnovers at the window limit mostly stop.
- **The flow schedules episodes as the unit.**
  - Affinity to one slot.
  - Preemption charges the switch's setup cost in the index: a dream never breaks a convoy; a live reply
    may, paying ~0.1 s with RAM paging.
  - MindMemory pages an episode's KV as a unit.
- **An expand of a collapsed handle is the measured cost of collapsing too much.** Its rate feeds the
  epoch-start optimum below, so the policy tunes itself from her behavior.

**The epoch start has a closed form.**
- With grounding floor `g` (re-prefilled every turnover; measured 12-20k tokens on the dyno), start `s`,
  window `W` and appends per act `a`, turnover prefill per act is `(g + s) · a / (W − g − s)`. That always
  favors a small `s`.
- Against it stands the expansion cost of what a small start collapses.
- Minimizing the sum gives `s*` from measured `g`, `a` and the expand rate, per resident. The dyno already
  shows the direction: a 1/4 start beat 1/2 by 10-25% prefill at 48k.
- The grounding floor `g` is the next target: blocks that rarely change should not re-prefill on every
  turnover.

**Learning the policy.**
- The reward is what we already measure: wait between acts, first-token latency for live and directed
  calls, aggregate tokens per second, a penalty on any class's worst wait, and expands of collapsed
  handles.
- The simulator is the training environment (thousands of simulated hours per minute); the dyno replays
  her real traces; the live read validates.
- Policy parameters: the cμ cost slopes, the start-share model, the episode-boundary detector. Learning
  starts from the math above, never from nothing (plan line 14, "learned policies on our own receipts").

## 2c. Corrections from review (Codex and Cormac on 6d18b4564)

1. **Signals and reliable inputs are different types** (Codex).
   - A *signal* stage keeps the newest and may drop older entries, because its input only *references*
     content already committed to its owner (the codec, a store).
   - A *reliable* stage never drops: when full, the producer gets backpressure (an `Err` it must handle,
     or an awaited send), and a restart recovers durable work before re-subscribing.
   - `continuum-flow::Stage` encodes this as its overflow policy, so a tool result can never ride a
     dropping queue. Fixture: a full queue receiving distinct tool results and a cancellation keeps every
     committed result reachable; only wake signals collapse.
2. **A bounded wait for background needs an explicit share** (Codex).
   - "Training completes within a bounded wait" and "work is never preempted for it" cannot both hold
     while work is always ready. Rising priority alone does not bound the wait.
   - So background has a measured minimum service share, granted at its own batch and step boundaries.
     It is non-preemptible for at most one batch, whose duration is measured.
   - The bound holds under a stated admitted load. Simulator case: continuous work plus continuous
     interactive arrivals, and background still meets its bound.
   - Joel's "a dream still happens irregularly and doesn't eat into work" is this minimum share, sized
     from measurement, not a guess.
3. **Every bound names its measurement** (Cormac). The stated maximum waits in §6 come from receipts or
   replays, never from someone picking a constant. A simulator that proves a guessed number proves
   nothing.
4. **Training's cost is forward plus trainable tokens** (Cormac; card 32cc831f: 48 trainable of 146,044
   tokens held a node 3.3 h), so the background share buys lessons, not re-reads.
5. **The fork keeps its batch-boundary stop** and loses only its yield *policy* (`share_ppm`,
   `max_slowdown_ppm`). The engine executes; the flow decides.
6. **The admission and pager seams** (Codex):
   - one physical-memory ledger backs both KV and training leases, and a compute yield keeps them;
   - subscribe before checking a predicate, and recheck at grant;
   - speculative pages and prefill are keyed by context generation, so a stale completion never
     replaces newer state;
   - an uncertain external effect is never replayed because its wake signal was coalesced.
7. **Inventory additions** (Cormac):
   - the persisted pause holds (`~/.continuum/state/training-holds.json`);
   - `CapacityParked` (`job_create.rs:116`);
   - partition parking (#5557).

   All become §2.11 predicate waits. Codex: `TrainingTriggerModule` has no event subscriptions and a
   30 s tick, while `ResourceDaemon::subscribe` already exposes board changes; the all-parked partition
   path (a failed prepared job) is distinct from `DispatchPhase::Parked`, so each gets its own fixture
   before the timer goes.

## 2d. Focus: the flexbox budget, with full compliance (Joel, 2026-10-28)

The owner of this contract is [FOCUS-CONTRACT.md](FOCUS-CONTRACT.md); this section is the flow plan's view of it, and where the two differ, FOCUS-CONTRACT wins.

> "We called this flexbox budgeting." "Ideally it even helps us shrink down during what we've seen the
> coding models excel at, like as it starts to develop it's not a cluttered mind." "In human terms this
> is focus." "Others shrink. She still has all of the context. Requires adherence."

**The rule** (already law; saved 2026-10-18; `PERSONA-COGNITION-PIPELINE.md`:
`FlexboxRagBudgetAdapter`/`flex_allocate` is the only budgeter):
- Every part of a turn is a claim in **one** flexbox allocation per turn, over one window:
  - framing, tools, required input, conversation, results/actions, grounding, recall, working memory;
  - the reply.
- Each claim has a priority from the turn's intent, a floor of real need, and a want of its whole.
- Sources deliver *levels* (whole, summary, pointer), never a size.

**Focus is a brain process, and it is hers** (Joel: "I don't choose my focus, my mind just does";
"it can still be 'her' but in reality it's a brain process"). The priorities select themselves:
- **Recent use.** Items her own acts touched (files read and edited, tests run, the card she's on) are
  the open episode's working set and take the high claims. Untouched items decay toward summary, then
  pointer.
- **Salience.** The amygdala's outlier detector (being addressed, affect, a sudden change) raises a
  claim on its own. A directed line also takes the interactive class.
- **Relevance.** Recall ranks by nearness to her current thought, so related engrams surface at summary
  level.
- **`cognition/focus.rs` `FocusState`** (her self-set level, thread cursor and mutes) is an override, not
  a requirement. A recipe's preset is where the process starts. Nothing is computed *for* her: it is her
  own activity, reflected.

**What focus does to the view.**
- The current episode's working set gets the high claims.
- Everything else shrinks to summary or pointer: still in her view, reachable on demand, never clipped.
- She keeps all of her context; only its level changes.
- A directed line raises its claim and the flow's interactive class; the episode stays warm and resumes.

**Measured non-compliance, as of 2026-10-28** (`fable/working-window`; the full map is on card 23ad5675). Since then #5564 (one allocation sizes her prompt and every reader complies) and #5573 (results storage never decides what she sees) replaced two items below: the results share (`collapse_results_over_cap`) and the tools' fixed window/4 claim (now the core hands' floor); read this as the 10-28 baseline, not the current state:
~40 production sites size prompt content outside the allocation.
- **Conversation** runs a second flexbox in `compose_for_turn` over the 159k served window, plus a
  per-turn cap and head cuts.
- **Results and actions:** `collapse_results_over_cap`, `latest_action_chars`, `split_action_result`
  and `RESULT_SHARE_DENOM`.
- **Grounding** budget is frozen at spawn from the affordable window; doctrine, map and wall head cuts;
  board 12/5 caps.
- **Tools** take window/4 as a fixed claim.
- **Recall:** count tiers, frozen at spawn.
- **Dispatch and tool-output** slices, echo and trail heads.
- **A per-call cache credit** swung `ContextBudget` from 22k to 159k between calls
  (`cognition.budget.prefill_bound` fired 739 times in an hour).
- My 48k working window on `prompt_fit` alone was another non-compliant patch: the prompt fit 48k while
  the sources sized for 159k, and Aiko turned over every act.

**The change, shipped as one:**
1. The `self_sized` ratchet first (`budget_discipline`, ONE-BUDGET.md slice 0; it absorbed `prompt_self_sizing`). It counts `ContextBudget::live|live_or_floor|from_window*`,
   the fraction methods, `_SHARE_DENOM`, and any `flex_allocate`/`FlexboxRagBudgetAdapter` outside the
   one allocation.
2. Migrate by prompt mass: conversation (one allocation, `compose_for_turn` delivers items), results,
   grounding, tools, recall, small sites. Delete dead code.
3. RAM and disk limits become a named storage budget that never decides what she sees.
4. The total window is one parameter of the allocation: the dyno's measured knee.
5. Ship when the ratchet reads 0, all cognition tests pass, and the dyno on all four residents shows
   prefill per request no worse with zero handles dropped. Then the fraction methods go private, and
   the compiler holds the line.

## 3. Live conversation, end to end

The budget: first audio within about 1 s of the speaker stopping. The path and its timestamps (Codex's list, each a probe):

1. ingress (a word, a line);
2. mind wake;
3. context ready;
4. admission queued and granted;
5. page restored;
6. engine first token;
7. delivery (TTS first audio, UI first character).

What the plan does at each point:
- **Ingress and wake.** Streaming STT partials are perception events; her signal is interactive at once.
  - The STT Semaphore(2) that *drops* a third utterance becomes a queue.
  - The single Whisper mutex and the one mutex per TTS model become capacities in the flow, with interactive class (inventory #124-126).
- **Context ready.** The codec appends partials as they arrive, and prefill follows them.
- **Admission.** A mind in a call is seated with its KV pinned; nothing pages it out mid-call. Interactive class preempts lower classes' compute at the next decode step.
- **Barge-in.** A new utterance during her reply raises the yield on her generation. The stream is dropped, and what she said so far stays in her context.
- **Delivery.**
  - The 250 ms token flush timer and the positron `min_interval` sleep become event-driven: flush on sentence boundary or first token (#95, #159).
  - The call join's fixed sleeps go (#132).
- **Several people.** Every utterance is perception. Whether she speaks is her decision, one coalesced signal per mind.

## 4. Inventory mapping (160 places; full list in the appendix)

| category | count | what happens |
|---|---|---|
| A: belongs in the flow | 74 | migrates to the capacity it protects; its local semaphore/heap/backoff is deleted |
| B: content for the codec | 13 | becomes a codec source or consolidation (rings, dedup, digests) |
| C: real locks on shared data | 28 | stay; any held across an await on a cognition path is narrowed (#79, #78) |
| D: polling loops | 34 | become event-driven (watch/notify on the thing that changes) |
| E: dead or duplicate | 11 | deleted, including the kernel's ConcurrentPriorityQueue, inference-grpc's unused priority queue, the unused TokioConcurrencyPolicy |

The top live-latency items, in order:
1. the serial per-mind loop with no preemption (a directed line waits for a whole self-turn);
2. the per-mind act gate;
3. the classless adapter semaphore;
4. slot wait and page IO when minds outnumber slots;
5. the relaunch drain barrier;
6. the adapter registry RwLock held across generation;
7. the voice path's drops and mutexes;
8. RAG work inline on the turn path;
9. remote minds' refusal and 2 s reply poll;
10. token flush timers.

## 5. Order of work (each step: simulator cases pass, dyno replay no worse, 15-minute live read no worse)

| step | what | owner | proves |
|---|---|---|---|
| 0 | Hold #5558 commits 3-5; keep 4579b4711 live | Fable | done 14:20Z |
| 1 | `continuum-flow` primitive + **simulator** (pure scheduler, fake engine, virtual clock; §6 cases) | Fable | every case in §6 green in seconds |
| 2 | Engine compute as the first capacity: one dispatcher replaces lanes, adapter semaphore and the act gate; in-flight gauge and directed class move into it; warm-ahead becomes a utility-class use of free capacity (no reserved scratch slot) | Fable (dispatcher), Codex (admission/allocator/pager seams) | sim: directed waits < one decode step, gauge exact, warm-ahead intact; live: slot wait, act median |
| 3 | KV memory admission: `MindMemory::seat` + RAM paging (fork #79, #70) as the release-memory verb | Fable, Codex | sim: no resident overwritten unsaved; live: switch freeze ~0.1 s |
| 4 | Training and examiner as consumers: compute yield at batch boundary, memory lease kept; fork yield windows and TrainingHolds-as-scheduler deleted; S7 stop-drain fixture (920619c5) | Cormac | sim: training's worst-case wait bounded; live: resident decode during training |
| 5 | The mind's stages on the flow: coalesced signal per mind, codec consolidation, late render with capture at send, prefill as enricher | Fable | sim: 20-line burst = one wake, newest read; live: act median, prefill per act |
| 6 | Live path: STT/TTS capacities, barge-in yield, event-driven flush, call-join sleeps gone | Fable + BigMama (live) | sim: first token ≤ 1 s after silence; live: the seven timestamps |
| 7 | The rest of category A by subsystem; D polls to events; E deleted | each owner by area | per-subsystem receipts, lines removed |

## 6. The simulator: every case, deterministic, in seconds

Pure scheduler + MindMemory + a fake engine on a virtual clock. Each case states its pass rule.

| case | must hold |
|---|---|
| 4 residents, 4 slots, all working | no waits; every slot busy |
| 6 residents, 4 slots | each class within its stated max wait; pages swap without loss |
| directed call mid-act, engine full | lowest class yields within one decode step; first-token time recorded |
| live call: streaming partials, speaker stops | prefill keeps pace; first token ≤ budget after silence |
| barge-in during her reply | her generation yields; her context keeps what she said |
| 20 chat lines to one busy mind | one coalesced wake; she reads all 20 consolidated; no stale request runs |
| permanent interactive saturation | work still gets its minimum share (Codex) |
| dream + training under steady work | both complete within their stated worst-case wait; work never preempted for them (Cormac) |
| training holds memory, needs compute | yields compute at batch boundary, keeps memory; never deadlocks memory-vs-compute (Cormac) |
| warm-ahead with no reserved slot | next mind's page restored ahead; no resident overwritten |
| engine relaunch mid-act | in-flight work re-queued or failed cleanly; pages intact |
| capacity shrinks 4 -> 2 | no new starts past capacity; running work finishes |
| KV memory full | admission pages by class; never an OOM |
| remote mind leased from another node | same classes, remote capacity; lost reply recovered by event, not poll |
| relaunch drain | new turns not blocked behind the longest in-flight turn (inventory #5) |

The simulator reports what the live read reports: each resident's wait between acts, first-token latency
for directed and live calls, switch freeze, aggregate tokens per second, and each class's worst wait.

## 6a. The dyno and the simulator: how to use them (both proved their worth on 2026-10-28)

Answer "will this be faster?" here before touching the live node. Live reads under load are road
tests in traffic.

**The dyno** replays each resident's real captured prompts through her view, offline, with serving
down (no residents involved):
- Captures: `~/.continuum/fixtures/prompt-captures/<persona-id>.jsonl`, plus `.prev.jsonl` for the
  previous segment. Resident ids come from the core log, never from memory.
- Run:
  `cargo run --release -p continuum-context --example replay -- --append-tail <prefill tok/s> [--window <tokens>] [--generational] <captures>`
- Read the `TABLE` lines: prefill tokens and seconds per request, turnovers, re-prefill per turnover,
  and handles dropped. A change that drops handles loses ability and is out.
- Receipt that set the working window: 48k cut prefill per request about 3x for all four residents
  with zero handles dropped; 32k doubled turnovers. Evidence:
  `~/.continuum/evidence/2026-10-28-working-window/` (`replay_window.sh` is the serving-down
  window script: trap and watchdog, restart on any exit).

**The simulator** (`continuum-flow::sim`) puts scheduling and engine changes on a virtual clock:
- The engine model is calibrated from `llama-bench` at depth and from the measured multi-stream
  aggregate.
- It reproduced the live gate removal (simulated 76 -> 57 s, live 122 -> 84 s) before forecasting
  anything.
- Run with `cargo test -p continuum-flow --release -- --nocapture`. Each case prints what a live read
  reports.

**Order for any speed change:** first the simulator or dyno forecast, then the dyno on real code,
then the 15-minute live read against the same numbers.

## 7. What gets deleted when this lands (mandatory: a migration that leaves its old mechanism beside the new one is not done)

- Lane admission (`resource_admission/lanes.rs`, `progress.rs`, `serving_lane_permit.rs`).
- The adapter concurrency semaphore and its grow/shrink task.
- The scratch reservation.
- The prefill throttle (done).
- The kernel `ConcurrentPriorityQueue`, `TokioConcurrencyPolicy` and `message_processor`.
- `inference-grpc`'s priority queue and worker pool.
- Per-module `max_concurrency` limiters on the typed path.
- The fork's training yield windows, and TrainingHolds as a scheduler.
- The act clock's queue-time accounting.
- The 34 polling loops' timers, wherever an event exists.
