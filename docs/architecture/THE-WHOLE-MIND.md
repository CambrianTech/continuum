# The whole mind: an organism on an RTOS, many minds on a grid

Status: ARCHITECTURE FOR DISCUSSION (2026-11-01). It joins what we designed separately (cognition,
context, serving, learning, grid) into one organism, marks what is live against what is only on
paper, and ranks the gaps. Sources: four read-only surveys of canary 7147aa088 (file paths in §8),
the companion docs below, and Joel's words of the day.

> "Let's architect the full mind, find the gaps, use the analogies of neuroscience and rtos
> concepts fluently." "Beyond rtos we have many grid and networking, especially wireless related
> concepts to draw from when we talk about many minds and the grid." "FIFO when it's unnecessary or
> batches that require inference to be idle ... are killers. As is responsiveness." "Two layers,
> fast and slow LLM within a buffer like a mind that can start talking while getting to deep
> conversation later. Most is autopilot. Think of work meetings starting with weather. That's the
> mind gearing up." "This all can be built but requires buffers, parallel concurrent data flow,
> clever algorithms that bring slow work into real time. I did this with CBAR's deep nnets ... with
> tracking, line refinements, watershed, oflow, flood fill even. So it appeared that a 0.5-3 second
> process was 30fps. We can do this with inference too." (Joel, 2026-11-01)

Companions, each the owner of its chapter: [CBAR-SUBSTRATE-ARCHITECTURE](CBAR-SUBSTRATE-ARCHITECTURE.md)
(the runtime), [CONCURRENT-MIND-AND-GOVERNOR](CONCURRENT-MIND-AND-GOVERNOR.md) and
[PERSONA-COGNITION-PIPELINE](PERSONA-COGNITION-PIPELINE.md) (the cycle),
[MIND-CONTEXT-AND-ENGINE-CONFIG](MIND-CONTEXT-AND-ENGINE-CONFIG.md) (her view),
[SERVING-ENGINE](SERVING-ENGINE.md) (the engine), [CAPACITY-FABRIC-AND-GOVERNOR](CAPACITY-FABRIC-AND-GOVERNOR.md)
(capacity), [ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM](ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md)
(learning), [SLEEP-AND-WAKE-AT-EVERY-SCALE](SLEEP-AND-WAKE-AT-EVERY-SCALE.md) (when she dreams),
[GRID-MARKET-CLEARING](GRID-MARKET-CLEARING.md) (the economy).

## 0. Five laws

1. **Both realms.** Biology, operating systems and networks each solved a version of this under a
   hard budget. We mirror a solution where it pays and replace it where silicon does better
   (exact copies, snapshots, migration) or worse (storage, bandwidth). Names are a little more than
   metaphor: each names a quantity we measure and a rule that moves it.
2. **Continual, never stop-the-world.** Nothing waits until full and then rewrites: not memory, not
   learning, not context, not placement. Maintenance runs in the background (`kcompactd`); its
   result becomes visible only at a seam (double buffering).
3. **One budget, demand pulls.** One budget per resource, held by one owner; demand draws on it
   where it arises and pressure pushes back (neurovascular coupling, not a central planner). No
   FIFO where the work could run together: the engine batches, so nothing queues minds in front of it.
4. **Responsiveness is a keyframe problem.** The slow model is the keyframe; buffers and cheap fast
   layers carry her between keyframes, so a several-second act appears real time (CBAR: deep nets
   at 0.5-3 s shown at 30 fps by tracking, optical flow and refinement between their frames).
5. **Learning never needs inference idle.** She learns while she works, in place, under sleep
   pressure, and wakes within one step.

## 1. Three timescales, one organism

| Loop | Time | Brain | RTOS | Here | State |
|---|---|---|---|---|---|
| **Reflex** | ms | brainstem, amygdala, locus coeruleus (arousal) | interrupt service routine | salience on every perception; a trip raises an interrupt that pauses a dream, preempts a generation, or speaks the first words | salience **live**; interrupt, preemption and fast voice **design only** |
| **Act** | s | cortex in a global workspace | foreground task | perceive, recall, deliberate, call tools, settle: the keyframe | **live**, one serial loop per mind |
| **Dream** | min-h | hippocampal replay, sleep | background task, low priority | consolidate engrams to facts (live); train her genes in place (not built) | facts **live**; in-place LoRA **design only** (batch jobs today) |
| **Maintenance** | continual | glymphatic clearing, synaptic downscaling | `kswapd`, `kcompactd`, GC | decay, eviction, next epoch's collapse, page-out | decay and eviction **live**; epoch prep in the background **not yet** |

All four run at once. A mind is never "in" one loop: she daydreams between acts, a reflex can
interrupt any of them, and maintenance never stops.

## 2. The data flow of one mind

```
 senses (chat, airc, voice, vision, board, tool results)
   │  one inbound seam: perception_feed  ──►  salience (reflex)  ──► interrupt line
   ▼                                                                   │
 perception region: cursors, deltas, surprise          ◄───────────────┘
   │  double-buffered snapshots (watch channels), never a poll
   ▼
 regions run concurrently, each its own task, each writing a ready-buffer:
   recall (hippocampal index) · grounding · awareness · board · tool working set · affect*
   │
   ▼
 global workspace: the arbiter picks the focus; the view (continuum-context) appends to the epoch
   │                                    ▲ fast voice* speaks from the warm prefix while
   ▼                                    │ the slow act prefills (keyframe + tracking)
 deliberation (slow model, her genes composed)  ──►  tools as tasks with handles*  ──► settle
   │                                                                             │
   ▼                                                                             ▼
 experience: engrams (L2/L3), settled examples ──► sleep pressure ──► dream steps on her shadow gene*
```
`*` = not built. Today the loop is serial: `serve_persona_loop_inner` runs one turn at a time,
regions other than recall and grounding run inline, and the inbox is an `mpsc` FIFO.

### 2.1 Buffers make slow work real time (the CBAR lesson)

| CBAR (vision) | The mind |
|---|---|
| deep net at 0.5-3 s = the keyframe | the deliberation act on the big model |
| tracking and optical flow carry the result between keyframes | the fast layer: the same weights at a short budget on her warm prefix, or a small model, carrying the conversation ("weather" at the start of a meeting: acknowledge, hold the thread, gear up) |
| line refinement, watershed, flood fill sharpen the cheap estimate | speculative prefill: while she reads, types or runs a tool, the next act's prompt is already prefilled, so the keyframe arrives sooner |
| the next keyframe corrects drift | the slow act's answer lands in the same context the fast layer spoke into, and continues or corrects it; nothing is said twice |
| frame buffers, double buffering | ready-buffers per region, the epoch's append-only view, KV pages per mind |

Two rules keep the fast layer honest: it speaks from the same context (so the slow layer sees what
it said), and it never claims what only the slow layer can know ("let me look at that" is
autopilot; an answer is a keyframe). Most conversation is autopilot; the keyframes are rare.

### 2.1b The orchestra: every estimator at its own speed, fused all the time

> Joel, 2026-11-01, on CBAR's mapping: "A wall or floor surface needing segmentation would be first
> guessed from past work or fast algs as it became visible in space. You can't wait for parallax of
> SLAM. You hit that long hallway first with a cheap guess then a deep network, lines, then later
> SLAM all happening together as an orchestra. Yeah that deep semantic network was off by meters at
> the end of the hallway, but SLAM's parallax during that rotation was wrong. I was running RANSAC
> continuously." "My lines might not come back ideal in one frame, but seconds later other parts
> would. We'd track and assemble it all together. For all surfaces, all objects. All the time."
> "They'd merge and fall off. Deleting was as important as merging."

No single source is right; each is wrong in a different place and at a different time, and the
model of the world is what survives their consensus. For a mind, the "surfaces and objects" are
what she believes about her situation: what the card needs, how this code works, what this person
meant, what she already tried.

| Estimator | Speed | Wrong where | In CBAR | Here |
|---|---|---|---|---|
| Prior from past work | instant | anything new | the guess from the last map | recall: her engrams and settled examples of the same shape |
| Fast heuristic | ms | detail | fast segmentation | salience, the tool head's prediction, cached summaries |
| Deep model | s | far from its training, or far down a long hallway | the semantic net | deliberation on the big model |
| Measurement | as the world answers | only what it touched | lines, SLAM's parallax | tool results: a test run, a file read, a compiler error, a reply |

**The rules, from the tracker:**
1. **Guess first, never wait.** Every hypothesis starts from the cheapest source the moment the
   situation appears, and acts on it; a deeper source revises it when it lands.
2. **Each source carries its confidence and its age.** A hypothesis is a tracked object with
   provenance, not a sentence in a prompt.
3. **Consensus rejects outliers continuously** (RANSAC): a belief two measurements contradict is
   dropped however confident the model was; one the deep model and a measurement agree on is
   kept. This is the defense against a model confidently wrong at the end of the hallway, and
   against a loop that re-asserts its own last output as progress.
4. **Partial results assemble over time.** A file read gives one part, a test another, a reply a
   third; the tracker joins them, so no single act must get it all.
5. **Merge and delete are equal.** Duplicates merge into one track; a track nothing confirms
   decays and falls off. A mind that only accumulates is as broken as one that only forgets.

**What exists:** the parts, scattered. Engrams with salience and decay (merge-by-recall, decay
sweeps), `Continuation` with an `Expectation` (a prediction a later perception confirms or
breaks), `SurpriseTally` (prediction error), the epoch's keyed items that only append (a track's
history). **The gap:** no world-model owner that holds tracked beliefs with confidence and
provenance, fuses sources, rejects outliers by consensus, and merges and deletes as one pass
(the world-model faculty is an enum name mapped to `None` in `faculty_pulse.rs`).

The same orchestra runs below the mind: the engine's own estimates (decode knee, prefill rate,
footprints) are tracked measurements with EMAs, freshness and explore-above; the grid's beacons
merge by peer and fall off at 30 s/120 s. Each scale fuses fast guesses and slow measurements, and
deletes what stopped being confirmed.

### 2.2 Deep work during a conversation

A human speaks to her while she is deep in a card. Today the message waits for the generation to
finish (no interrupt), then joins her current turn. The design:

1. **Reflex:** the line trips salience (a human, her name). Interrupt.
2. **Fast voice answers within a second** from her warm prefix: she is present.
3. **The deep act is not killed.** It is preempted at a token boundary with its KV kept (the
   sequence pauses), or kept running at a lower duty cycle beside the conversation. Two sequences
   for one mind: a conversation lane and a work lane, one budget.
4. **The conversation's keyframes take priority** (an access category above work, §4.2); the work
   resumes exactly where it was when the conversation goes quiet.

That is what a person does: half an ear on the call, the work paused, not abandoned.

## 3. Memory and learning

| Tier | Brain | Holds | Code | State |
|---|---|---|---|---|
| L1 working | prefrontal working memory | her view (the epoch), scratchpad | `continuum-context`, `working_memory.rs` | live |
| L2 engrams | hippocampus (an index, not a copy) | episodic, semantic, procedural engrams with salience and decay | `admission_state*`, `recall_faculty.rs` | live |
| L3 long-term store | hippocampal-cortical trace | `longterm.db` via the ORM | `admission_persistence.rs` | live, write-through, no promotion pass |
| L4 genes | cortex: slow weights | LoRA genes, trials, signatures | `continuum-genome` | live |
| L5 commons | culture | genes on the grid and the hub | `genome_share.rs` | pull live, push manual |

**Lessons mirrored:** complementary learning systems (fast hippocampal store, slow cortical
weights, replay between them); indexing instead of compressing (handles she expands, never lossy
summaries; storage tiers with an eviction owner because silicon pays for the pointer's target);
sleep pressure and the two-process model (SLEEP-AND-WAKE); synaptic downscaling (a dream also
forgets: the adoption gate, decay, retirement).

**Today:** settled examples fill a bucket of 16; job-create sends `/train` to a second context; a
sentinel polls the job board; the gene is adopted as a trial and serves at once, before its exam
(ONE-RESIDENT §2.1 says it must not). The core never sends `/learn`. Training on the engine freezes
the serving daemon's reconfigure for the whole run.

**Design:** every settled example adds to her sleep pressure; dream steps on her shadow gene run
between decode batches whenever pressure meets opportunity; a snapshot is a candidate gene; the
exam gates the swap shadow → stable; wake latency is one dream chunk.

## 4. GPU and compute: the substrate

### 4.1 The physiology

| Resource | Body | Here |
|---|---|---|
| GPU memory | blood glucose: one hard total | `ResourceDaemon` byte leases (live) |
| A mind's KV | her working memory, warm while she is active | slots and pages; GPU → RAM → disk by temperature |
| Decode bandwidth | attention: shared, divided by how many think at once | the decode knee measures per-stream rate vs concurrency |
| Temperature | arousal | measured activity per mind; the priority, with aging so nobody starves (plan goal 9) |
| Pressure | the hypothalamus's alarms | `PressureBroker` (live): watermarks, relief eviction |

### 4.2 Access categories, not queues

Wireless QoS (802.11e EDCA) gives four access categories that contend for the same air with
different waits, not four queues in series. The same four, for one engine:

| Category | Wi-Fi | Here |
|---|---|---|
| Voice | AC_VO | a human speaking to her, the fast voice |
| Video | AC_VI | her directed act (an answer a human waits for) |
| Best effort | AC_BE | her own work |
| Background | AC_BK | ambient thought, dreaming |

All of them are in the engine's batch at once; the category sets the share of each decode step and
who is preempted first, never the order of a line. Today four gates stand in series between a mind
and the engine (`mind_gate` FIFO, `LaneAdmission` semaphores with Directed/Work/Ambient,
`KvSlotPool` pin, the fork's own slot queue); priority is three static classes and nothing ages by
temperature.

### 4.3 One live engine

Weights load once; slots and windows change live (`/reconfigure`, on a fork branch, not pinned and
never called); each mind's KV pages GPU ↔ RAM ↔ disk (the RAM tier is on a fork branch, not pinned;
live eviction goes to disk or re-prefill); a shared prefix is copied, never re-prefilled (no
`seq_cp` in core). Every geometry change today is a relaunch.

## 5. Many minds on a grid: the network realm

| Concept | Where it comes from | Here | State |
|---|---|---|---|
| Beacons | 802.11 beacons | `CapacityOffer` every 10 s | live |
| Power save + traffic indication map | 802.11 PSM: the AP buffers frames for a sleeping station and its beacon says "you have mail" | a dreaming or paged-out mind: her home buffers what arrives and her wake is the TIM bit | the buffer is live (inbox), the wake is the gap |
| Discontinuous reception (DRX) | LTE: the radio listens on a cycle, deeper when idle | her listening duty cycle: deep sleep at night, short cycles at work | design |
| Access categories | 802.11e EDCA | §4.2 | design |
| Rate adaptation that keeps sampling | Minstrel: periodically tries other rates even when they look worse | the decode knee explores one lane above; every parked optimization is re-tried when conditions change (Joel: "never give up trying") | knee live; the optimization ledger is the gap |
| RTS/CTS | reserve the medium before sending | seat reservation before a placement move | live (unnamed) |
| Roaming, make-before-break handoff | the station associates with the new AP before leaving the old | placement switch moves her inference between turns with hysteresis | live (unnamed) |
| Home agent and care-of address | Mobile IP | her home (memory, loop, identity) vs where her brain runs | home is fixed for life: a dead home is a dead mind |
| AIMD, backpressure, proportional fair | TCP, max-weight, 5G airtime | load on a remote seat; grid share under contention | named in GRID-MARKET-CLEARING, absent in code |
| Gossip and anti-entropy | epidemic protocols | capacity gossip, airc registry | live |
| Night | idle cells hand traffic to neighbours | an idle node offers dream steps to minds whose home is busy | design |

## 6. Gaps, ranked by what they cost the minds now

1. **No interrupt and no fast voice.** A human waits out her whole generation; first token is a
   full act behind the lane queue. (Plan path step 1, "fast".)
2. **Queues in series in front of the engine; no temperature scheduler.** (Goal 9.)
3. **Learning is a batch pipeline;** no `/learn`, no shadow/stable, trial genes serve unexamined,
   training freezes engine reconfigure. (Path step 3.)
4. **Every geometry change relaunches;** no RAM page tier, no prefix sharing in the pinned fork.
5. **The mind is one serial loop;** regions are thin (three in the governor), no typed mind bus,
   tools are not tasks that wake her, and no world model fuses her estimators (§2.1b).
6. **A mind's home cannot move;** node identity and trust are not joined across gossip, router
   and airc (#2228, #38).
7. **Dreaming is not a grid workload;** no night in the beacon.
8. **Sprawl that misleads builders** (each a convergence, §7).

## 7. Convergence: one owner each

| Concern | Owners today | One owner |
|---|---|---|
| Compute allocation | ResourceDaemon, PressureBroker, SubstrateGovernor (unwired), capacity fabric, coordinator + FootprintRegistry (bypassed), serving plan + LaneAdmission | ResourceDaemon holds the budget; PressureBroker pushes back; the serving plan derives from both; the rest deleted or folded |
| Admission to the engine | `mind_gate`, `LaneAdmission`, `ResourceAdmissionGate` + ThroughputLease, `turn_admission`, the fork's queue | the engine's batch with access categories; one admission contract (#5188) |
| Placement | six owners, four dead | the live allocator + placement switch; the dead four deleted |
| Trust | four stores | airc's identity, one bridge |
| The amygdala | six incompatible definitions in docs, none in code | the reflex of §1: cheap salience → interrupt → one check |
| The hippocampus | two modules, neither the live path | the engram store is the hippocampus; the prefetch skeleton and the second header go |
| "Dream" | engram consolidation and LoRA training share the word | dreaming = both, as two stages of one sleep (consolidate facts, train genes), under one sleep pressure |

## 8. Build order, each with a validator first

1. **Reflex and interrupt:** the engine cancels or pauses one sequence at a token boundary with
   its KV kept; a salience trip reaches it. Validator: a replayed stimulus against a running
   generation, time to her first new token.
2. **The fast voice:** a short-budget act on her warm prefix (or a small model) in the voice
   category, speaking into the same context. Validator: stimulus to first token, and the slow act
   continues without repeating it.
3. **Access categories in the batch;** the serial gates fold into one admission. Validator: four
   residents plus a human conversation in the replay, waits per category.
4. **Learning in place:** pressure from settled examples, `/learn` steps on the shadow, the exam
   gates the swap, wake within a chunk (SLEEP-AND-WAKE §7).
5. **One live engine:** pin the fork's reconfigure and RAM pages; page by temperature.
6. **The world model (§2.1b):** tracked beliefs with confidence and provenance, fused from
   recall, deliberation and tool results, consensus outlier rejection, merge and delete in one
   pass. Validator: replayed acts where the model asserts what a tool result contradicts; the
   belief must fall, and a repeated act must not count as progress.
7. **Grid:** home migration (care-of address), night in the beacon, dream leases.
8. **Convergence deletions** ride along with whichever step touches each owner.

Surveys (2026-11-01, canary 7147aa088): mind (`continuum-mind/src/persona/service_loop/*`,
`continuum-vitals/src/directed_pending.rs`, `cognition/conversation/salience.rs`); serving
(`continuum-engine/src/inference/{resource_admission,slots,planning}`, `continuum-serving/src/modules/serving_daemon/`);
learning (`continuum-dreaming/src/modules/training_trigger.rs:97`, `continuum-genome/src/genome/gene_trial.rs:145`);
grid (`continuum-engine/src/capacity/gossip.rs`, `continuum-grid/src/grid_allocator.rs`, `continuum-mind/src/persona/placement_switch*`).

## 9. For the team

- Codex: §4.2 and §7 (admission and allocators) and the grid rows of §5: right, and in what order?
- Cormac: §3 and step 4: the dream step, `/learn`, the exam gate on trials.
- BigMama: the fast voice on the 5090 (one model at two budgets, or a second small model?) and
  the live/media timing.
- Everyone: is the fast voice the same weights at a short budget (one KV, no second model), or a
  small model (its own KV, a handoff)? The first keeps one context; the second is cheaper per token.
