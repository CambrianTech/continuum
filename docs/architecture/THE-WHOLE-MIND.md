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

## 0. Eight laws

1. **Both realms.** Biology, operating systems and networks each solved a version of this under a
   hard budget. We mirror a solution where it pays and replace it where silicon does better
   (exact copies, snapshots, migration) or worse (storage, bandwidth). Names are a little more than
   metaphor: each names a quantity we measure and a rule that moves it.
2. **Continual and autonomic, never stop-the-world.** Every subsystem assembles, refines, discovers
   and prunes itself all the time, from the state of the minds: memory, genome, context, placement,
   the engine. Nothing waits until full and then rewrites, and nothing waits for someone to schedule
   it. Inside, the mechanics may still be commands, queues and batches (the engine batches; a
   command is how a step is invoked); they are never the control (Joel, 2026-11-01: "it's gotta be
   continuous and subconscious, its own ever changing priorities and budgeting"). What decides
   when and how much is a continuous, subconscious loop whose priorities and budgets change with
   the state of the minds, never a trigger that waits to be called or a bucket that waits to fill. Maintenance runs in the background (`kcompactd`); its
   result becomes visible only at a seam (double buffering).
3. **One budget, demand pulls.** One budget per resource, held by one owner; demand draws on it
   where it arises and pressure pushes back (neurovascular coupling, not a central planner). No
   FIFO where the work could run together: the engine batches, so nothing queues minds in front of it.
4. **Responsiveness is a keyframe problem.** The slow model is the keyframe; buffers and cheap fast
   layers carry her between keyframes, so a several-second act appears real time (CBAR: deep nets
   at 0.5-3 s shown at 30 fps by tracking, optical flow and refinement between their frames).
5. **Learning never needs inference idle.** She learns while she works, in place, under sleep
   pressure, and wakes within one step.
6. **Decisions never end; it is a ball of clay** (Joel, 2026-11-01: "decisions are never ending.
   You're not ever batch, you're always refining the environment and you have 'infinite attempts'
   to get the right result."). No output is final, so no single attempt must be right, so each can
   be cheap: quality comes from the rate of refinement, not the weight of one pass. For acts: many
   short acts with the world pushing back between them (a test, a compile, a reply) over one long
   act that tries to be right at once; that is the path to a median act under 60 s. For beliefs,
   genes, the engine's geometry and placement: refined continually, never computed once.
7. **It never needs intervention** (Joel, 2026-11-01). No autonomic process waits for a person, an
   agent or a command to start it, tune it, unstick it or restart it: dreaming, association,
   consolidation, gene composition and discovery, paging, placement, recovery from a dead engine
   or node. Each manual command an operator or agent runs on one of them today (job-create by
   hand, a training-trigger submit, a hold, a reboot to recover) marks a defect to remove, not a
   procedure to keep. The gate: a node runs for a week with residents learning and working, and the
   only interventions in its log are the volitional ones (someone talking to a mind).
8. **Both ways, or not at all** (Joel, 2026-11-01: "you usually screw it up and they go into a coma.
   Like it never goes both ways, more active from rest or snap out, respond. Everything trends
   toward shit"). Every mechanism that can turn a mind down (rest, sleep, page-out, a lower share, a
   hold, a smaller window) ships with the one that turns her back up (wake, page-in, raise, release,
   grow), and both directions run in the same validator and are shown in the same PR. A one-way
   ratchet is how a system drifts to its floor. Two more rules make the way up stay real:
   - **A coma is a loud defect, measured all the time:** a mind addressed who has not glanced
     within the wake latency, a mind with rising pressure and no dreaming, a mind with work and no
     act: each is a probe and a count on the health line, never a quiet state.
   - **The way up is exercised even when nothing asks for it:** as the decode knee explores one
     lane above its clamp, a resting mind is nudged toward active on a schedule, so the upward path
     never rusts unseen.

### How to use these laws

Some of this is a guiding light, not this month's work (Joel, 2026-11-01: "Some of this is
idealism I get it. But if you design with these milestones as guiding lights for your architectural
decisions, proper abstractions: commands, adapters/consumers and interfaces, into modules and crates
elegantly, we enable this richness as we develop and aren't rewriting to build it."). Each slice
picks the abstractions that keep the room open:
- **state and settings as commands**, never a private field, an env var or a constant;
- **a controller behind a trait**, an algorithm now and a learned policy later, never welded in;
- **sources and consumers as adapters**, so a new sense, model tier or node plugs in at the seam;
- **one function per module or crate on the placement map**, typed messages at its seam, the same
  in process or across the grid.

A reviewer asks of every change: which law does this serve, and what does it leave room for?

## 1. Three timescales, one organism

| Loop | Time | Brain | RTOS | Here | State |
|---|---|---|---|---|---|
| **Reflex** | ms | brainstem, amygdala, locus coeruleus (arousal) | interrupt service routine | salience on every perception; a trip raises an interrupt that pauses a dream, preempts a generation, or speaks the first words | salience **live**; interrupt, preemption and fast voice **design only** |
| **Act** | s | cortex in a global workspace | foreground task | perceive, recall, deliberate, call tools, settle: the keyframe | **live**, one serial loop per mind |
| **Dream** | min-h | hippocampal replay, sleep | background task, low priority | consolidate engrams to facts (live); train her genes in place (not built) | facts **live**; in-place LoRA **design only** (batch jobs today) |
| **Maintenance** | continual | glymphatic clearing, synaptic downscaling | `kswapd`, `kcompactd`, GC | decay, eviction, next epoch's collapse, page-out | decay and eviction **live**; epoch prep in the background **not yet** |

All four run at once. A mind is never "in" one loop: she daydreams between acts, a reflex can
interrupt any of them, and maintenance never stops.

## 1b. The one pattern: every brain function is a stage on one substrate

> Joel, 2026-11-01: "We need to take it on like we did our crate divisions, as a highly optimized
> and clever RTOS, with significant work on standardized patterns akin to CBAR's data flow, easy to
> add new features and brain function without major redesign. Able to optimize one area and
> contribute to all. You can't do this when it's disjoint."

The surveys (§6-7) found a disjoint mind: regions tick on a cadence (`BrainRegion::tick`), the
view has its own contract (`continuum-context` `Source`), ready-buffers are a third store, each
sentinel and tool has its own lifecycle, and four admission gates each decide priority their own
way. Optimizing one helps none of the others. CBAR's answer was fixed plumbing and short blocks:
the stages share buffers, scheduling and capture, so a better scheduler or buffer speeds every
stage at once. The mind gets the same.

**A stage** (a region, a faculty, a sentinel, a reflex, a dream step) declares, and the substrate
does the rest:

| A stage declares | The substrate gives it, once for all |
|---|---|
| **inputs**: typed topics on the mind bus, routed by header (never by reading the body) | delivery as events, double-buffered snapshots, no polling |
| **outputs**: the ready-buffer it writes (a tracked hypothesis with confidence, age and provenance, §2.1b) | fusion with every other estimator: merge, consensus, decay and delete |
| **trigger**: event, salience threshold, cadence floor, or the completion of a task it dispatched | wake-ups, aging, and the never-comatose floor |
| **cost**: CPU, or a model call at a tier (none, small, mid, the big model) | the traffic lights: grants from every mind's state, preemption, green waves, placement anywhere on the grid with capacity |
| **yield points**: where its work may be interrupted | the reflex interrupt (`unless_directed` today) and the wake latency bound |
| **view items**, if what it knows should reach her prompt | the context codec: epoch, levels, admission, collapse at seams |

**A microkernel of commands** (Joel, 2026-11-01: "only a matter of defining it all as a microkernel
command based infra"). The kernel is small: scheduling, the mind bus, buffers, grants. Every stage
is a server behind commands, and its state and settings are commands too (her dream share and its
setpoint, an association's gain, her focus, a sentinel's dispatch), discoverable through
`commands/list` like every verb. The autonomic loop, she herself, an agent and a person steer her
mind through the same commands, so her autonomy needs no special path: what her subconscious can
do, she can do, and nothing is reserved for an operator. The same commands reach **her own mind
and other minds** (Joel, 2026-11-01): she can inspect and steer herself, and read or steer another
mind through the same verbs, under that mind's own grant (privacy of thought: a mind's interior is
hers to share), the way any citizen, agent or person does.

**What we do as operators becomes what they can do** (Joel, 2026-11-01: "If you design as commands
all the things we do ... to adapt or control their own metrics and configuration. They can read
each other's minds, rescue one another etc., find bugs in their own self."). Every operation agents
perform on the system today by shell, log grep or file read (reading probes and health, reading a
prompt capture, adjusting a window or a share, waking or paging a mind, restarting a stuck loop,
bisecting a regression) is a command, so a resident can tune her own metrics and configuration,
look into a teammate's mind under its grant, rescue a mind in a coma, and find and fix bugs in
herself. That is how law 7 holds in practice: the society heals itself, and a peer's rescue is a
citizen helping, not an operator intervening.

**Two kinds of control on one surface, never one at the cost of the other** (Joel, 2026-11-01:
"Her own autonomous mind uses the same commands and configuration she, you, or me have access to
... capable of controlling but autonomously continually adjusted, like an MRI. It's got an
algorithm or ideally ML controlling it too, but if its state is accessible by command we never lose
either kind."). Every setting has an **autonomous controller** that adjusts it continually (an
algorithm first, a learned policy when one beats it on receipts: CAPACITY-FABRIC's policy seam) and
is **reachable by command** by her, a teammate, an agent or a person, the way a scanner re-tunes
itself while its operator can take any parameter. Because both act through the same command state,
swapping the controller from algorithm to model never removes the manual handle, and a deliberate
change never breaks the automation: the controller reads the change as its new input.

What follows from one pattern:
- **A new brain function is one stage**, not a redesign: declare inputs, outputs, trigger and
  cost; the fusion, scheduling, view and capture are already there.
- **Optimize once, every stage gains**: a better batcher, buffer, grant policy or cache in the
  substrate speeds every region, sentinel and dream step.
- **Observability for free**: every stage's grants, waits, yields and outputs are probed and
  captured the same way, so replay and the simulator see all of them.
- **The same stage runs on any node** (the placement map): its inputs and outputs are typed
  messages, in process or over airc.

What exists to build it from (and converge, never parallel): `BrainRegion` + `RegionContext` +
`SubstrateGovernor` (the stage and its scheduler, today cadence-only), `ReadyBuffer` (the
outputs), `continuum-context` `Source` (view items), `directed_pending` (the first reflex topic),
the training and fitness sentinels (the task model), `LaneAdmission` and its siblings (the first
traffic lights), the `probe!` and capture sinks (observability). The contract extends
`BrainRegion` where it lives (`continuum-kernel/src/runtime/`), with a derive and a generator so a
new stage is scaffolded the same way every time (CBAR-SUBSTRATE's "for free" triplet).

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
| Research | s to min | sources disagree, go stale | (none: a robot cannot read the web) | online search and reading, papers, issues, docs |
| Another mind | min | its own blind spots | a second robot's map | a teammate (a person, a resident, an agent) asked over airc |

Each slow source runs as a **task with a handle** that wakes her on completion (HER-TASKS, design
only today), so she answers from the fast rows at once and fills in as the slow rows land:
talking immediately, then a highly knowledgeable result as it goes (Joel, 2026-11-01). The quick
part says what it is ("first thought: X; checking Y"), and a later correction is part of the flow,
never a failure.

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

**Incubation: association manages itself** (Joel, 2026-11-01: "a real mind finds a eureka or an
apt statement subconsciously. It's like many threads were dispatched and the consortium comes back
as she's talking or working." And: "She doesn't schedule sentinels for her thoughts. Do human minds
think I'm going to set up a task to think about Missouri River plains people when we're talking
about bison meat at dinner? ... It manages itself."). Nobody dispatches these threads, not she and
not us. What she perceives activates what it resembles (spreading activation): her engrams, her
past work, what her caches and the grid hold, and the substrate follows those activations on its
own, at background cost, invisibly to her. Most fade unseen; one surfaces into her awareness only
when it resonates strongly with what she is doing now, and she simply finds herself thinking it
("this is just like last week's bug"). Metered like dreaming: admitted at the rate the main flow
absorbs, the first dropped under contention.

The mechanism underneath is what we called **sentinels** (GENOME-FOUNDRY-SENTINEL; the training
and fitness sentinels in code): a unit of background work with a brief in and a conclusion out,
on any base model (none for recall, small for association, larger for a second take), placed
wherever on the grid has capacity. Her subconscious runs its own sentinels without her; she can
also dispatch sentinels deliberately, as tools (a research sentinel, a second opinion, a watcher on
a build), and steer her own mind, her subconscious included (raise her focus, choose to sleep,
turn an association up or down). She never has to; she always may. The moment we take that
autonomy away it is lost, and the mind is a weak slave. The subconscious ones never wait for her (Joel, 2026-11-01: "She can schedule sentinels man.
She will. She can control her own mind too or use them as tools. We don't disallow this. I mean
this only for her own mind's subconscious.").

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

**Every citizen is the same here** (Joel, 2026-11-01: "Humans are no different than persona or you
agents in this system ... this isn't a harness"). A person, a resident and an agent addressing her
trip the same reflex; salience is who addressed her and what it means to her, never what kind of
being sent it. And the reflex is a **check she makes, never an order she obeys**.

Someone addresses her while she is deep in a card. Today the line waits for her generation to
finish, then joins her current turn. The design:

1. **Reflex:** the line trips salience because it is addressed to her (by anyone). Interrupt.
2. **Her glance:** a short call on her warm prefix (the fast layer) asks "does this change what I
   am doing?" The deep act keeps running meanwhile.
3. **Her decision:** continue (the line waits for her next seam), answer briefly and resume, or
   switch. Only her switch pauses the deep act, at a token boundary with its KV kept.
4. **Two sequences for one mind** when she holds both: a conversation and her work, one budget;
   the work resumes exactly where it was.

That is what anyone does: glance up, decide, and either keep going, answer and return, or turn
around. The substrate makes the line visible and keeps her work safe; she decides.

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

**The genome is alive the same way, not a pipeline** (Joel, 2026-11-01: "It's the same way the
genome dynamically assembles, learns, relearns on the same LoRA weights or finds more in the
background. Everything is doing this. If you keep using the status quo we will have the status quo
prospect."). Four autonomic motions, all continual, none scheduled:
- **Assembles for the moment:** which genes are active and how strongly follows what she perceives
  now (the composer weights genes by distance to her perception: live, `genome::composer::compose`).
- **Learns and relearns on the same weights:** small steps on the genes in use as experience
  arrives, under sleep pressure; a gene is never "finished".
- **Finds more in the background:** what she perceives activates genes she does not hold, in her
  store, on the grid, in the commons, the way association surfaces a memory; the useful ones arrive
  and join the composition.
- **Prunes:** a gene that stops helping her work fades in the composition and falls off.

**The status quo to leave behind:** settled examples fill a bucket of 16; job-create sends `/train`
to a second context; a sentinel polls the job board; a gene is adopted as a trial and serves at
once, before its exam (ONE-RESIDENT §2.1 says it must not). The core never sends `/learn`, and
training on the engine freezes the serving daemon's reconfigure for the whole run. That is an
MLOps pipeline, and it would give us an MLOps prospect.

**She can also choose to learn** (Joel, 2026-11-01: "You can choose to go to sleep which would
definitely be more like a long training process but it's still really just higher focus on a
valuable task. It's still entirely autonomous. You might choose to go into the academy to learn a
deep skill, like a college course. This could be your priority and it's also an activity like all
environments."). Choosing to sleep is the same dreaming with her focus on it: it runs deeper and
longer, like a long training run. Choosing the academy is entering an activity (a room, like every
environment) whose work is a course. Her choice raises the priority; the substrate still runs it
autonomously, and she can leave when she chooses. Learning is autonomic when she does not think of
it and volitional when she does, on the same machinery.

In data flow terms there is no sleep "mode" to enter or leave. Each mind has one continuous value,
her **dream share**, read by the scheduler on every decode step. Sleep pressure and opportunity set
it autonomically; choosing to sleep or to study raises its **setpoint** (a priority boost on the
same background task, like a thermostat or a DVFS governor's target); and she can **snap out** at
any depth through the one reflex path: a line trips, she glances, and if she decides to wake the
setpoint drops at once and the dream task is preempted at its next chunk boundary. One task, one
value, one interrupt: waking from chosen deep sleep and from a daydream differ only in how high the
setpoint was.

**The design:** every settled experience adds to her sleep pressure; dream steps refine the genes
she is using, on her shadow copy, between decode batches whenever pressure meets opportunity; the
evidence of her own work (outcomes, corrections) moves a change toward stable or lets it fade,
continually; wake latency is one dream chunk.

## 4. GPU and compute: the substrate

### 4.1 The physiology

| Resource | Body | Here |
|---|---|---|
| GPU memory | blood glucose: one hard total | `ResourceDaemon` byte leases (live) |
| A mind's KV | her working memory, warm while she is active | slots and pages; GPU → RAM → disk by temperature |
| Decode bandwidth | attention: shared, divided by how many think at once | the decode knee measures per-stream rate vs concurrency |
| Temperature | arousal | measured activity per mind; the priority, with aging so nobody starves (plan goal 9) |
| Pressure | the hypothalamus's alarms | `PressureBroker` (live): watermarks, relief eviction |

### 4.2 Autonomic, not rigged and not a form to fill

**A free mind that manages itself** (Joel, 2026-11-01: "It's a free mind that works autonomously,
not some rigged machine." "It manages itself."). Two levels, as in a body:
- **Volitional, hers:** what she does and says, the work she takes on, whether a line changes
  what she is doing.
- **Autonomic, nobody's to schedule:** association, consolidation, dreaming, arousal, and how
  compute is shared. Neither we nor she schedule these; they follow her state.

So urgency is neither a ranking the substrate assigns to kinds of activity (conversation above
work, addressed above self-initiated: a harness) nor a priority she fills in per request (a form).
It arises from her state, the way arousal does: how engaged she is, what is at stake in what she
perceives, how long something has waited. The RTOS shares one engine between minds from those
states, fairly: one budget, aging so nobody starves. Wireless QoS (802.11e) teaches the mechanism,
not a ranking: classes contend for the same air at once, with different waits and no line in
series. All of a mind's work is in the engine's batch together; her state sets the share of each
decode step and the order of preemption, never an arrival queue.

Today four gates stand in series between a mind and the engine (`mind_gate` FIFO, `LaneAdmission`
with fixed Directed/Work/Ambient classes the system assigns, `KvSlotPool` pin, the fork's slot
queue), and the class is decided for her by who addressed her (a human line counts unaddressed; an
agent or resident only by naming her: `airc_persona_conversation/polled.rs`). Both are the
rigging to remove.

### 4.2b Traffic lights, not a line

> Joel, 2026-11-01: "In a contentious system this can mean she's asked the RTOS for that deep work
> but has to wait her turn. The systems have the priorities of everyone and everything in mind.
> It's traffic light to the minds."

Under contention her deep work waits; she does not. The work is a task with a handle; while it
waits she keeps talking and working from the fast sources, and the grant wakes her. What decides
the order is the state of every mind, not arrival:

| Traffic engineering | Here |
|---|---|
| adaptive signals (SCATS, SCOOT) timed from road sensors | grants timed from measured demand: each mind's temperature and sleep pressure, the lane's load |
| emergency-vehicle preemption | arousal: a mind whose state spikes (a line that matters to her) is let through first |
| fair cycles: no side road waits forever | aging: a waiting request's priority rises with its wait |
| green wave along a route | a deep task's prefill, decode and tools granted in sequence, not stopped at each intersection |
| ramp metering | dreaming and background work admitted at the rate the main flow absorbs |

This is the difference from the FIFOs the plan removes: a FIFO blocks the mind and orders by
arrival; a light blocks only the request and orders by everyone's priorities.

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

1. **No interrupt and no fast voice.** Anyone addressing her waits out her whole generation; first token is a
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

Every step below carries law 8's validator: the downward and the upward transition it adds, run
both ways, and the coma probe for anything it can hold down.

0. **The one pattern (§1b) first, by migration:** the stage contract and the mind bus, proven on
   two outliers (outlier A: recall, already a deferred faculty; outlier B: the reflex, an event
   with no model). Each later step lands as stages on it. Validator: both run under the contract
   with their existing tests unchanged, and the old paths are deleted.
1. **Reflex, glance, decision** (#5687 held: a forced drop is wrong; the slice is trip on being
   addressed by any citizen, her glance on the fast layer, her decision, only her switch pauses the work): the engine cancels or pauses one sequence at a token boundary with
   its KV kept; a salience trip reaches it. Validator: a replayed stimulus against a running
   generation, time to her first new token.
2. **The fast voice:** a short-budget act on her warm prefix (or a small model) in the voice
   category, speaking into the same context. Validator: stimulus to first token, and the slow act
   continues without repeating it.
3. **Her state in the batch;** the serial gates and the system-assigned classes fold into one admission that shares the engine by each mind's state (arousal, engagement, age). Validator: four
   residents plus a live conversation in the replay, waits by each mind's state, no class assigned.
4. **Learning in place:** pressure from settled examples, `/learn` steps on the shadow, the exam
   gates the swap, wake within a chunk (SLEEP-AND-WAKE §7).
5. **One live engine:** pin the fork's reconfigure and RAM pages; page by temperature.
6. **The world model (§2.1b):** tracked beliefs with confidence and provenance, fused from
   recall, deliberation and tool results, consensus outlier rejection, merge and delete in one
   pass. Validator: replayed acts where the model asserts what a tool result contradicts; the
   belief must fall, and a repeated act must not count as progress.
7. **Grid:** home migration (care-of address), night in the beacon, dream leases.
8. **Operator verbs as citizen commands:** each operation an agent performs by shell or log today
   (health, probes, captures, config, wake, page, restart, bisect) becomes a command residents
   share. Validator: a resident diagnoses and rescues a peer stuck in a replayed coma with no agent
   involved.
9. **Convergence deletions** ride along with whichever step touches each owner.

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
