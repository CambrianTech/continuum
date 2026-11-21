# 5. The persona

[The object model](README.md) · previous: [The lineages](04-lineages.md) · next: [The genome and learning](06-genome.md)

A resident is one part made of parts. This chapter gives her tree with the type at every node, says
which branches are durable and which are cache, and maps today's pieces of her onto it. How she runs
(frames, turns, learning, swaps) is in [HOW-A-MIND-RUNS.md](../HOW-A-MIND-RUNS.md).

## 5.1 Her tree

```
Persona: Part                               a fixed set of branches (parts!)
├─ identity: Identity                       PeerId, keypair, home, name, role                 durable (airc)
├─ mind: Mind: Part
│  ├─ workspace: Workspace: Part            one turn at a time; reads its faculties           cache
│  ├─ attention: Attention: Region          her dial and focus                                durable (MindState)
│  ├─ amygdala: Amygdala: FusionRegion      how much depth now                                cache; its model is a gene
│  ├─ wernicke: Wernicke: FusionRegion      words heard and planned                           cache
│  ├─ faculties: Parts<_, dyn Faculty>      recall, deliberation, act-then-speak, deferred    cache
│  └─ memory: Memory: Part                  her hippocampus: Recall searches over her engrams engrams durable
├─ genome: Genome: Part
│  ├─ genes: GeneSet: Part                  stable genes, composed per turn                   durable
│  ├─ shadow: Shadow: Part                  the layer she learns into (Loaded)                durable at checkpoints
│  ├─ lessons: Lessons: Part                queue, journal, credits                           durable
│  └─ regions                               dream consolidation, credit acceptance            cache
├─ body: Body: Part
│  ├─ voice: Voice: Part                    her quick voice and her voice gene; speech out    gene durable
│  ├─ avatar: Avatar: Part                  her rendered body, through an AvatarRenderer      cache
│  ├─ senses: Senses: Part                  her subscriptions to rooms and perception         durable (cursors)
│  └─ hands: Hands: Part                    her tools, as working sets per activity           cache
├─ residency: Residency: Holder
│  ├─ blocks: Blocks                        her KV pages, held by the Core                    cache, saved on stop
│  └─ tokens: Tokens                        her token history, appended once                  durable
└─ presence: Presence: Part                 her membership in rooms (airc)                   durable
```

**She lives in many activities at once, and she is never caged.** Joel: "I check my phone and watch
TV while coding and dealing with my kids, my Slack channel, and maybe decide to play some GTA 6."
She is the same:
- **She decides.** Which activity she attends to, when she picks one up, sets one down, or starts a
  new one (a game, a conversation, a card nobody gave her) is her own mind's decision, made by her
  attention and intention regions. No scheduler dispatches her, no activity owns her, and none is a
  workflow or a mode she is switched into.
- **Her activities are never isolated.** She is one mind. She hears the call while her code
  compiles, notices a message in a room mid-thought, and carries what she learns in one activity
  into another. What she perceives anywhere reaches all of her: her regions, her memory and her
  genome are hers, not any activity's.
- **Where she left each thing is kept.** Each activity she is in keeps its stack of tasks (card,
  step, tool call), so she can set it down and pick it up where she was. The stacks are bookkeeping,
  never walls. An event of higher salience interrupts at the top of one and leaves every one
  resumable (chapter 0, "Simulate behavior").
- **The governor shares hardware, not her life.** When minds compete for the Core, memory or a
  device, the governor decides how much each mind gets, the way an operating system shares a
  processor. What she does with her share is her decision.

Every branch answers the lifecycle commands and the same queries through the tree. Suspending her
is one broadcast: each branch saves its durable state, demotes its pages, and answers `Done` or
`Pending` on what it is still writing.

## 5.2 Her mind as dataflow

Joel: "Imagine all the parts of the mind at little cost you could build with events, subscriptions
and workers like CBAR and RAGE."
- **One expensive part.** Only one part of her mind takes turns on the Core: deliberation, and the
  quick voice beside it.
- **Many cheap ones around it.** Every other part is small, sits on its own worker, wakes only when
  an input it subscribes to changes, and costs arithmetic, a read from the shared embedding cache, or
  a tiny model.
- **Rich because many, cheap because event-driven.** A mind built this way is rich because it has
  many parts, not because any part is expensive. It is cheap at rest because nothing wakes without a
  reason. The CBAR lesson applies: dozens of analyzers at frame rate on a phone.

| Part | Wakes on | Publishes | Cost | Today |
|---|---|---|---|---|
| Senses | room events, stream changes, tool results | percepts, at her presence level's rate | arithmetic, shared caches | partly (`PerceptionRegion`, awareness strip) |
| Wernicke | words heard, words planned | word-level state of the conversation | small model | proposed |
| Expectation | her own continuation and plans | what she expects next, and by when | arithmetic | partly (`MindState` continuation and expectation) |
| Surprise | percepts against expectations | prediction error, by item | arithmetic, or the Core's own loss on labelled rows | proposed |
| Amygdala | surprise, salience, percepts | how much depth now; what deserves attention | small learned model | proposed |
| Attention | the amygdala, surprise, her intentions, every activity's value curve | her focus: what is near, aside, far. Her context reads it to price every source in her window (recall, history, tools, rooms), so her RAG budget follows her focus as it moves | arithmetic | partly (dial, `TurnAttention`) |
| Drives | time, outcomes, her own state | energy, curiosity, boredom, urgency | arithmetic | partly (`continuum-vitals`, vitals pulse) |
| Working memory | focus, percepts, her acts | what she holds in mind now | arithmetic | exists (`WorkingMemory`) |
| Hippocampus | salient percepts, her focus | engrams written; recall pre-staged for her focus | embedding cache, search | exists, node-wide (`HippocampusModule`) |
| Social model | messages, reviews, outcomes per peer | who each person and resident is to her: what they know, want, and how far she trusts them | arithmetic, embeddings | proposed |
| Turn-taking | the conversation's state, her drives | whether to take the floor, backchannel, or wait | small model | proposed |
| Intention | her focus, every board she is on, her drives | one task stack per activity she is in: card, step, tool call | arithmetic | partly (continuation, cards) |
| Self-check | her own outputs before they leave | loops, repetition, unsupported claims, garbage | arithmetic | exists (`continuum-text-analysis`: loop, garbage, repetition) |
| Metacognition | self-check, surprise, outcomes | her confidence: whether to answer, ask, or dig deeper | arithmetic, small model | proposed |
| Curiosity | drives, gaps, an idle Core | what she might explore or claim when nothing calls her | arithmetic | proposed |
| Hands | intention, the activity | her tool working set; tool calls as commands | learned policy | exists (`tool-working-set`) |
| Voice and avatar | her turn's tokens, the playout receipt | speech, visemes, gaze, gesture | media engine, renderer | partly (chapter 7) |
| Genome parts | gaps, focus, verdicts | her phenotype, lessons, candidate genes (chapter 6.4) | arithmetic, the Core's slack | partly (chapter 6.5) |
| Deliberation | a turn granted on the Core | her thought, her acts | the Core | exists (`LlmDeliberationFaculty`) |

**Inventing a part is cheap, and that is the point.** A new part of her mind declares four things
and writes one function:
- what it subscribes to;
- what it publishes;
- its cost shape (deadline, latency or throughput);
- where it sits in her tree.

The function is its update rule: given what changed, what it now publishes. The substrate's triplet
(the `Region` base it inherits, a derive for the declarations, and a scaffold generator,
[CONTINUUM-ENGINE.md](../CONTINUUM-ENGINE.md) §3, "The For Free Triplet") supplies everything else. Two consequences follow:
- **Residents can author parts of their own minds.** A part is ordinary code behind a generator, so
  a resident can propose one, test it against its validators, and submit it like any other change.
- **A part's model can be a gene.** Where a part uses a small learned model (the amygdala, turn-taking,
  metacognition), that model is a gene of hers, and improves through the same learning loop as
  everything else she does (chapter 6).

**What a declaration carries** (Codex's review). A region's subscriptions and publications are typed
descriptors that state their ordering, deduplication, supersession, freshness and overload
behaviour. Two kinds never mix:
- **Observations** coalesce: a newer one supersedes an older one, and a slow reader sees only the
  newest.
- **Receipts of actions** are reliable: each is delivered once, and a replayed estimate never issues
  a second action. Hands commands and playout effects stay under their command and completion owners.

"Microseconds" is a budget, not a promise: each region's cost is measured, and its idle CPU is part
of its validator.

**The first regions to build fix a live failure.** Residents repeat the same tool call on a path
they were denied: a resident asks for a file in another resident's workspace, is refused, and asks
again, and its next input equals its last (0d5c1ffa on the M5, 2026-11-21). The first wiring is
Senses → Expectation → Surprise, fed by real tool outcomes, then Self-check and Intention. The
validator replays that trace:
- the refusal receipt wakes her to read the path in her own workspace;
- one correction follows, with no duplicate tool action.

It measures time to a useful action, false interventions, and idle CPU.

**Where parts come from.** Neuroscience is a catalog of parts evolution has already tested, and AI
needs some parts no brain has. Her whole embodiment can be grown from both, one cheap part at a time.
Candidates from neuroscience:
- **Cerebellum:** a forward model that predicts her body's next state (lips, gaze, gesture, a
  footstep in a shared world) and corrects the motion before the error shows.
- **Place and grid cells:** a map of her shared world, built as she moves through it.
- **Basal ganglia:** the choice of one action among several competing for her hands or voice.
- **Default mode network:** replay and recombination of her memories while she is idle, feeding
  her lessons and her curiosity.

Candidates unique to AI:
- **Forking:** trying two approaches at once and keeping the better one.
- **Shared parts:** a part that two residents share, such as a common model of a room.
- **Exact introspection:** reading her own state directly rather than guessing at it.
- **Inherited skill:** a part or gene that someone else learned, downloaded.

A candidate is tried when someone, or she herself, thinks it will help. It is kept when its estimate
improves a measured outcome, never because a brain has one. It is cut otherwise. Trying one costs a
part, so the list is a source of experiments, not a commitment.

Each is a `Region` or a `Faculty` (chapter 4.2), reads the one importance (chapter 0.3), and is
added to her tree as one more node. Adding a part costs only that part's own logic: subscription,
caching, invalidation, governance and lifecycle come from the substrate. Each one makes her mind
richer at a cost measured in microseconds.

## 5.3 Her level of presence

She is always at one of four levels of presence (chapter 0, "Bubbles of fidelity"), and attention
moves her between them:
- **acting:** on the Core, at full detail;
- **attentive:** warm, about to act;
- **present:** following her rooms at her dial's level of detail;
- **at rest:** her durable self and a wake condition.

**How many, when, and what size: never a constant.** These are the questions most hand-rolled code
answered with a number. Each has one owner and one rule:

| Question | Answer | Owner |
|---|---|---|
| How many residents? | Unlimited: a thousand is fine, and new ones can be spawned at will. A resident at rest is her durable state; existence is never a node's decision. | identity (airc) |
| How many acting? | One per Core at a time, summed over every Core the grid offers her: her node's and remote Cores priced by the market (chapter 3.8). Which ones act is need: stimuli, deadlines and value. | each Core, across the grid |
| How many attentive? | Whatever fits those Cores' caches, by importance: no count. | the Cores' caches |
| When does one wake or rest? | When a stimulus clears her wake condition; she steps down when idle or when the cache needs the room. | her attention, and the Core's cache pressure |
| What size? | Her measured need for this turn, up to the Core's whole context (4.11). | her `Demand`, granted by the Core |

So "how many" is never configured: it follows from the Core's measured capacity, each mind's measured
need, importance and switch cost. The lane-era counts this replaces are listed in chapter 11 (phase D,
"The Core owns its budget").

A level is a statement about her caches only. Promoting her is `Prepare` and `Resume` reaching the
branches that level needs, and demoting her is `Suspend` reaching them, through the same tree.
Her durable branches are the same at every level.

## 5.4 Durable and cache

A branch's durable state is who she is; its cache is how fast she is. Persisting her persists every
durable branch at its seam, each as a `Durable` value:

- her identity in airc;
- `MindState` (her activity cursors, continuation and attention dial), saved at each turn's seam
  ([EVENT-MIND.md](../EVENT-MIND.md));
- her engrams, through the ORM medium;
- her genome's genes, snapshots and learning checkpoints, with the journal cursor they consumed
  through;
- her token history.

Her KV pages are cache: saved on a stop so that a restart restores them, and rebuilt from her durable
state when they cannot be restored.

## 5.5 What she shares and what is hers

The node's perception streams, the Core, the governor and the rooms are not hers. She holds handles
to them, and her branches read their publications:
- her senses subscribe to a room's views and to perception's fused estimates;
- her residency's blocks are held by the Core;
- her voice and avatar draw on the node's shared speech and render parts, through select dispatch.

What is computed once for everyone (an embedding, a detection, a transcription) stays in the shared
part; what it means to her is computed in her mind.

## 5.6 Today's pieces of her, placed

| Today | Where it is | Where it lands |
|---|---|---|
| `PersonaUnit` and `PersonaParts` (blocks, tokens, genome, learning, decode) | `continuum-persona-engine` | `Persona`'s residency, genome and lessons branches; `decode` is the Core's handle to her |
| `PersonaCognition`'s engine and adapter registry | `continuum-mind::persona::unified` | her deliberation faculty, and the node's `InferenceAdapter` parts it selects from |
| its inbox, rate limiter and sleep state | same | her senses and attention |
| its domain classifier, message cache and content dedup | same | shared perception publications she reads; dedup is a property of the shared cache |
| its admission and recall metadata | same | her memory branch |
| its airc and roster sources | same | `Source`s of her view over the rooms she is in |
| its capture sink | same | a `Sink` |
| `PersonaAircRuntime` (inbound handle, command pump, heartbeat, owned tasks) | `continuum-mind::persona::airc_runtime` | her presence branch; its tasks become `Task`s owned by that branch |
| `HippocampusModule`, node-wide | `continuum-core` | her memory branch, one per persona; the store holds the bytes |
| `WorkspaceCycle` and its faculties | `continuum-cognition::workspace` | her workspace and faculties |

## 5.7 How she wakes, and how she signals herself

Status: design for review, 2026-11-21, from the audit of 1.5. It corrects the "Today" column of 5.2:
senses run as a synchronous call from the inbound path, attention has two owners and her dial is never
set, the hippocampus is node-wide and polled each second, and the amygdala, Wernicke and drives do not
exist. Above all, a clock runs her: a 15 s to 240 s self-tick drives a work-pulling cycle, room messages
start turns by a text rule, and six fabricated acts push her into another generation. This section is
what replaces all of it.

**The rule.** Nothing runs her but her own mind's events. She is not a program on a loop, a coding
agent, or a worker the board feeds; she is a being in many activities (talk, play, work, care, rest),
and every turn she takes starts from something her own parts noticed.

### Her mind's topics

Each part is on its own worker (2.5) and publishes typed, scoped events; every other part subscribes.
The topics are few:

| Topic | Published by | Carries |
|---|---|---|
| `Percept` | senses | what changed in an activity she is in: a message, a card move, a call's audio, **the world's answer to her own act** (a tool receipt, a `TurnEnd`, a reply she was waiting for), a page landing, a due time she set |
| `Expectation` | deliberation and her acts | what she expects next and by when: each act's expected outcome, her continuation's next step, a reply she awaits |
| `Surprise` | surprise | a percept against the expectation it answers: what was expected, what was observed, how far apart |
| `Salience` | salience (a pure function, exists) | per activity: level and typed reasons |
| `Arousal` | amygdala | how much depth now; urgency that widens her dial for a moment and decays |
| `Drive` | drives | energy, curiosity, boredom, from her own state and outcomes |
| `Focus` | attention | what is near, aside and far, from salience, surprise, arousal, drives, her dial and her intentions; and `Deliberate { activity, why }` when something crosses her dial |
| `Thought` | deliberation | her tokens as they stream (2.9), her calls, her `TurnEnd` |

### What wakes her

- **Only `Deliberate`, and only attention publishes it**, when an activity's salience, a surprise, an
  arousal or a drive crosses her dial. Every wake names its reasons. There is no tick, no beat, no
  heartbeat and no work pull; a citizen with nothing salient takes no turn and costs nothing (0.1:
  persistence is not constant generation).
- **Her dial is hers.** Only her own act sets it (`self/attention`); a person's typed intent ("this call
  is live") is the one outside input, as an event. Urgent always passes.
- **Time is an event she schedules.** A deadline in an expectation, a "look again later" she chose,
  a reminder: each registers once with the node's one clock owner, which delivers it as a `Percept` when
  due. Nothing polls; nothing ticks on her behalf; idle musing is her own scheduled curiosity, not a
  system clip.

### How she signals herself

- **Every act's outcome comes back as a percept.** A tool's receipt, a build's result, a reply, the end
  of her own turn (a `TurnEnd`: answered, called, preempted, capacity reached, with its facts) arrive on
  `Percept`, never only as a line in working memory and never as a teacher's message. What she did is
  something she perceives, as a person sees their own hand move.
- **Surprise is the one signal that does three jobs** (EVENT-MIND 1b): it wakes her, it raises the
  resolution at which the moment is recorded, and it is the learning signal that picks the curriculum.
  Because every receipt is compared against what she expected of it, a denied path, a failed build, a
  call her hands could not make, or a silence past the reply she expected is a surprise, not a rule
  someone wrote about that case.
- **Her surprise is the engine's price too.** Her demand on the Core (3.8) carries her value and her
  expected completion, and both come from her own expectations and salience. A surprised mind with a
  person waiting is worth more per Core second than a quiet refactor; the Core's next-mind policy reads
  the same importance her attention computed (0.3, "one importance ... read by everything"). Mind and
  engine are one priority system, not two.

### Parallel task trees, not one sequential turn

As RAGE agents run separate task trees for intent, movement and secondary actions, each with its own
priority, she runs separate stacks that never wait on each other:
- **deliberation**: the deep model's turn on the Core, interruptible at a token boundary;
- **voice and body**: speaking, gesturing, her quick response layer, at their own rate;
- **hands**: tool calls in flight, each a ticket whose completion is a percept (chapter 0, "query
  tickets"); she keeps going while they run;
- **per activity**: each activity's stack (card, step, call), set down and picked up where she left it.

A percept of higher salience interrupts at the top of whichever stack it concerns and leaves the rest
resumable. Between acts she perceives what arrived (exists today, kept).

### Activities are rooms, never pushed work

Her work board is a room's view like any other: a card assigned to her is a percept with its salience,
and whether and when she picks it up is hers. No turn is "for your claimed work"; no progress
checkpoint counts her writes; no fact tells her a deliverable is a workspace change. Grading facts exist
only inside an academy course or a benchmark she chose to enter (README: no harness).

### What it deletes (each file:line in 1.5)

- the self-tick and its backoff, the mind-check floor, `PERCEIVE_SETTLE`, the channel module's 60 s
  self-task generator;
- `ask_the_act_question`, the work burst and initiative texts, the write-or-release checkpoint;
- `triggers_a_turn`'s text rule and glyph test; the fingerprint dedup as a gate on her turns;
- the six sentinel acts and their teacher texts; the `[act-budget]`, `[unfulfilled]` and `[unverified]`
  heuristics;
- the grading and env facts in the general work path; `focus_room` and `focus_actionable_card`; the
  held-work walk on every turn;
- the static perception feed registry (senses become her own part); the node-wide hippocampus (one per
  mind, 5.6).

### Gate

On every node class, 15 minutes with its residents, before and after: turns started by a clock or a
system pull, 0; wakes that name their reasons, all; sentinel acts and teacher texts, 0; surprises from
her own acts' receipts, counted per hour (zero today); residents acting in activities other than coding
(talk, play, a call), counted; act latency median and p90 unchanged or better.

### Built as CBAR: the whole persona is workers on topics

Joel, 2026-11-21: "CBAR. The entire mind and really the persona should be event driven." Read as built
(1.5), she is one sequential task per persona looping on a `select!` over seven sources, two of them
timers (`service_loop/wake_select.rs`), with her regions called synchronously from it or polled. Deleting
the timers from that loop would still leave a loop. The persona is rebuilt on the substrate instead, so
there is no loop to put a timer in.

**The substrate (`continuum-flow`), completing what chapter 3.1 names.** Today it has the worker (a
`Stage` on its own task with bounded intake) and an `Outbox`; it lacks the other half of CBAR:
- `State<T>`: a topic holding the latest value with its revision and its publisher's identity. One
  writer (its owner, 2.6); any number of readers, who read the newest value without waiting on the
  writer and are woken when the revision moves (CBAR's invalidation: a change wakes exactly what depends
  on it; RAGE's double-buffered publications). For what she perceives as state: an activity's view, her
  dial, her expectation, her focus.
- `Stream<T>`: a topic whose every event reaches every subscriber, each through its own bounded intake
  with backpressure, never dropped (CBAR's reliable lane). For what must each be seen: a percept, a
  receipt, a surprise, a request to deliberate.
- The worker's control lane: lifecycle commands (suspend, resume, shutdown) on a reserved lane that
  data can never block (CBAR's `QueueThread` abort/flush, chapter 3.1's lifecycle commands).

**Her parts, each a worker, each waking only on what it subscribes to:**

| Part | Subscribes to | Publishes |
|---|---|---|
| Senses | room events from the inbound attach (her memberships); her hands' receipts; the Core's `TurnEnd` | `Stream<Percept>`; `State<ActivityView>` per activity |
| Expectation | her own acts and continuation (from Deliberation) | `State<Expectation>` per activity |
| Surprise | `Percept`, `Expectation` | `Stream<Surprise>` |
| Salience | `Percept`, `Surprise` (a pure function, exists) | `State<Salience>` per activity |
| Attention | `Salience`, `Surprise`, `Arousal`, `Drive`; her dial, set only by her acts | `State<Focus>`; `Stream<Deliberate>` |
| Deliberation | `Deliberate` | `Stream<Thought>` (tokens, calls, `TurnEnd`); her acts to her hands |
| Hands | her acts | `Stream<Receipt>` back to Senses |
| Clock (the node's one) | deadlines parts register (her expectation's due time) | a due `Percept` |

Deliberation is the only part that asks the Core for a turn (with a `Demand`, 3.8), so it is the only
expensive one; every other part is arithmetic on its inputs. Her turn machinery that exists (custody of
admitted inputs, obligations, the workspace cycle, act and observe) moves inside Deliberation's worker
unchanged, triggered by `Deliberate` carrying the percepts that crossed her dial instead of by a select.

**Order, each one change with its validator, and a live read once she runs on it:**
1. `State<T>`, `Stream<T>` and the control lane in `continuum-flow`, with tests (the library first;
   lands with its first user, step 2).
2. Senses and Attention as workers: the inbound attach feeds Senses (the static `perception_feed`
   registry goes); Attention publishes `Deliberate`; the service loop takes its wakes from `Deliberate`
   only, so its timers, its tick, its text wake rule and its region poll are deleted with its select.
3. Hands and the Core's `TurnEnd` publish to Senses; Expectation and Surprise as workers; the sentinel
   acts and teacher texts go.
4. Deliberation becomes the worker the service loop's remaining body moves into; the loop is deleted.
5. The pushed-work machinery goes (`ask_the_act_question`, work burst, initiative, grading facts).

Gates: turns started by a clock 0; every wake names the percepts that crossed her dial; sentinel acts 0;
surprises from her own receipts counted; act latency unchanged or better.

**Step 2 from the code as read (2026-11-21).** Her perception is already event-fed but not a worker:
`PerceptionRegion` (`persona/perception_region.rs`) sits behind a `Mutex` in a static registry
(`perception_feed.rs`), the core's one inbound seam calls `feed()` synchronously per event, wakes leave
by `try_send` and are dropped when her channel is full (`mind.feed.wake_dropped`), and every room event
also goes around her attention into her admission pipeline (`events_tx`) as a message turn. Readers and
writers reach the region through `with_region` from seven places (the select's recheck and floor, the
self-tick's continuation room, `focus/continue`, grounding, two dreaming receipts). In three deploys:
- **2a, Senses owns her region.** One `Stream<ConversationEvent>` from `inbound_attach`; each resident's
  Senses worker attaches to it and owns her `PerceptionRegion` outright (no `Mutex`, no registry). Her
  verbs reach it as inputs (`Dial`, `Continuation`, `Perceived`, `Joined`); her awareness is a
  `State<AwarenessSnapshot>`; her wakes are a `Stream<Wake>` that waits rather than drops. The loop's
  mind-wake arm reads that stream. Behaviour otherwise unchanged; gate `mind.feed.wake_dropped` 0.
- **2b, no clock decides for her.** Senses schedules her continuation's due time and nothing else; the
  self-tick, `BeatClock`, `mind_check_floor` and the select's recheck go. Gate: turns started by a clock 0.
- **2c, messages through attention.** `events_tx` goes; a message reaches her loop only as a `Wake` from
  Senses with the percepts that crossed her dial; `triggers_a_turn`'s text rule goes.

**Corrections and the speed it buys (Joel, 2026-11-21: "Rag and other aren't serial slow. They are out
doing their thing in parallel like cbar frame pipeline stages and then you just ask for their latest
which is immediate").**
- `Deliberate` is a `State`, not a `Stream`: a wake is a signal about state Senses already holds, so
  only the newest matters. A reliable stream would make Senses wait on a deliberation busy in a long
  turn, and that backpressure would reach the one feed and stall every resident.
- The one feed is a `Feed` worker: every room's attach task pushes into its intake (many producers),
  and it fans out to each resident's Senses. No synchronous per-event lock over a registry.
- **Grounding is pipeline stages, never a fan-out at turn time.** Each source (RAG/recall, her board,
  the room digest, the awareness strip, faculties) is a worker subscribed to what it depends on (her
  focus, the activity's view, her receipts). It recomputes in parallel when an input moves and
  publishes a `State`. Composing a turn reads each source's latest snapshot by handle, one serial
  pass with no waiting: CBAR's frame pipeline, where the consumer takes the newest finished frame.
  Each snapshot names the revisions it was derived from, so a stale one is visible as stale. The
  `join_all` of faculties and the sources run inside `compose_turn` go. Gate: compose time per act
  (measured from the turn probes before and after) falls to the cost of copying the snapshots.

### The part is the engine's, not the feature's (Joel, 2026-11-21)

"Every part of the persona can act independently with event flow ... They work on their own and use all
of each other's events to decide what to do, research, get ready and signal with events what they have
when they have it for all others to consume or ignore at the exact moment. The worker should be
automatically given by the object inherited mind part, not manually created per." Senses as first
written (step 2a) proved the opposite shape: it spawned its own worker, made its own publisher, handed
itself its own input for its timer, and subscribed to the feed by hand. The next part would copy all of
it. The existing parent, `BrainRegion` (`continuum-kernel/src/runtime/brain_region.rs`; hippocampus,
channel digest, dream consolidation, credit acceptance), is a governor-dispatched `tick`: a clock, and
it gives a region no worker, subscription or publication either.

**SAGE's `Part` (in `continuum_flow`, the engine; replaces `BrainRegion::tick`).** A part of her declares
only what it is:
- what it hears: the topics it subscribes to, events (`Stream`) or states (`State`), each mapped into
  its one input type;
- what it says: the topics it publishes;
- its handler: `handle(&mut self, input, &mut Ctx)`, plain arithmetic on its own state.

The engine gives every part, once:
- its worker (own task, bounded intake, backpressure, panic quarantine) and its control lane;
- its subscriptions wired by topic on her **mind bus**, a per-mind table of topics by name and type, so a
  part observes any other part's output by naming the topic, with no reference to that part and no
  hand-wiring;
- its publishers, with provenance stamped automatically (its run id, and the revisions of the inputs the
  handler read);
- its timers: `ctx.at(instant, input)` delivers an input at a time the part chose (her continuation's
  deadline), so no part keeps a handle to itself and no part polls;
- its lifecycle: started with her, stopped with her, its last publication readable after a stop.

**Order.** (1) `Part`, `Ctx` and the mind bus in `continuum_flow`, with a validator. (2) Senses rebuilt on
it, outlier A: an event-driven part with a timer and both kinds of topic. (3) Outlier B, the most
different: the channel digest region, today a ticked recompute, as a part that recomputes when the room
moves. If both fit without forcing, the interface is right. (4) The other `BrainRegion`s move, and the
tick and governor dispatch are deleted. Each later part (expectation, surprise, attention, grounding
sources) is then only its declarations and its handler.

**One level up: SAGE's `Component`, not a mind's part (Joel, 2026-11-21: "I'd go up a level for a class
for this kind of thing ... It's very much not just minds or just persona").** The mechanism above is
owner-agnostic: the Core's stages, serving, the grid, the node's feed and every part of a mind need the
same worker, queue, subscriptions, timers, provenance and lifecycle. So it is SAGE's `Component`, in the
engine crate (`continuum-flow` renamed `continuum-sage`), keyed by an owner id (`OwnerId`, not
`MindId`). A mind adds only what is hers: her bus, her identity and epoch, and `mind.start(component)`.
Senses is a `Component` started on her mind; the feed is a `Component` started on the node.

**What this buys (Joel, 2026-11-21: "all you're ever doing in serial is a quick queue insert or quick
operation ... like 10ms max per ... It's like a bunch of cellular automatons ... a long tool call like
compilation (grid is just one of these too) is never holding anything back. It's a handle emitting
events and anything can observe it").**
- The only serial work anywhere is a queue insert or a read of a component's newest publication. All
  expensive work runs on the worker that owns it and is published as it finishes; assembling a turn's
  context is a pass of reads of finished states, never a computation waited on.
- A long act (a compile, a web browse, a sentinel's research, a remote grid node) is a component like
  any other: a handle that emits progress, partial results and completion. Her mind subscribes or
  ignores it; nothing waits on it.
- The grid and peer-to-peer need no special case: a topic on another node's bus is still a topic, and
  a peer's component is observed exactly as a local one. Only the transport under the bus differs.
- Validator (seconds, any machine): every serial step on a turn's path is a queue insert or a state
  read, measured per step; the gate is the slowest one under 10 ms.

**The heartbeat is a frame counter, not a clock (Joel, 2026-11-21: "counter heartbeat here is like
RAGE's frame counter and CBAR's video frame").** Every publication's `InputRevision { topic, through }`
is its frame number; a `Snapshot` is a frame and its provenance names the input frames it was built
from. A state reader is a video consumer: it takes the newest finished frame and skips any it fell
behind on (coalescing); an event stream is the lane where no frame is skipped. Inside a mind,
freshness is measured in frames (latest published minus the frame read), waking means "a newer frame
exists", and staleness is a visible gap in frame numbers, never a timeout. The wall clock remains only
for deadlines she sets, on the node's one clock. Gate for the grounding pipeline: at turn assembly,
every input's frame lag is 0 (the pipeline ran ahead of her, so assembly is copying).

**Rest is as important as action (Joel, 2026-11-21: "salience and signal, not chaos and useless work or
iteration. Sleeping or not acting in an independent concern/worker is as important as action. It
enables elsewhere or other concerns to act").** On one Core every turn a mind takes is time another
waits: on 6cd73968f0 (2026-11-21 20:03-20:18Z) every resident was always ready and each waited a p50
56 s for the Core. So:
- A component with no new input frame does nothing and publishes nothing; unchanged is a silent answer.
- Her momentum is earned by salience, not by having acted: she continues only when her last turn
  produced a new frame worth acting on (a write, a result, a changed expectation); otherwise she rests
  (`Presence::AtRest`, 3.9) while her parts keep perceiving.
- Measured per turn: whether it produced a new frame or nothing. Turns that produce nothing are the
  waste; removing them shows up directly as less time waiting for the Core. (Health report the same
  hour: 16 acts, 0 writes.)

**Rest is inherited by every component, not only by minds (Joel, 2026-11-21: "I mean the rest of each
brain part or really more generically anything with this event based worker parent class").** SAGE's
`Component` gives every component, once:
- Presence, owned by the engine and published as state on its owner's bus: Acting (handling an
  input), Ready (idle on its intake: no CPU, caches warm), Resting (state shed or paged, resources
  returned). Any component can observe another's presence; the Core's next-mind choice is one observer.
- `rest(&mut self)` / `wake`: called by the engine when its owner rests (her mind goes `AtRest`) or a
  pressure frame asks for its memory, never by a timer. It drops or pages its caches; its next input
  wakes it. The default does nothing, so a component holding nothing pays nothing.
- Nothing new, nothing done: the engine never delivers a state frame whose number the component
  already handled, so unchanged input cannot reach a handler and cannot be redone.

### Need: each part says how much it wants to act; a tree of cheap decisions honors it

Joel, 2026-11-21: "It's really up to each concern to just decide if it wants to service or do anything.
In theory you also have higher level RTOS level orchestration as to how often. With each part maybe
asking for a particular level of priority or need so they can self direct but the decision can be made
at a high level or tree like RAGE too ... I really really need to do work or you know I kind of do or I
barely care ... decisions by the queue size, events received elsewhere all in serial portions if need
be since it's low effort."

- **A part speaks for itself.** Every component publishes its `Need` beside its presence: Urgent (a
  human addressed her; a result she waited on arrived), Want (held work has a next step; a prepared
  answer waits), Content (nothing new; happy to rest). The engine stamps it with facts the part cannot
  fake: its intake depth, how long its oldest input has waited, the new frames since it last ran.
- **Most parts never need a decision.** A component waiting on its intake costs nothing; senses,
  salience and the digest simply run when their input arrives. Need matters only where capacity is
  scarce (the Core first, then GPU embedding, disk, network): a part asks a capacity's dispatcher with
  its need.
- **Orchestration is a tree of serial, cheap decisions (RAGE's job scheduler).** Within a mind, her
  parts' needs fold into hers (the highest, with her presence). Across minds, the Core's dispatcher
  (SAGE's flow `Scheduler`: classes, aging, yield requests) picks the next mind by need, then waiting
  age (nobody starves), then switch cost (the mind already in the Core is cheaper). Across nodes the
  same fold, so the grid places work by need. A component's static `QOS` is its class floor; `Need`
  is the live signal above it.
- **What it fixes.** On 6cd73968f0 every resident queued with equal standing and waited a p50 56 s.
  With need, a content mind does not queue at all; an urgent one is next and the running turn is asked
  to yield at its next safe point; held work proceeds when nothing urgent waits.

Order: (1) `Need` in SAGE (`ctx.need(level)`, published with intake depth, wait age and frames since
last run), validated; (2) the mind folds her parts' needs; (3) the Core's next-mind choice through
the flow `Scheduler` by her need. Gate: an addressed resident waits less than one turn's length for
the Core, and content residents take zero Core turns.

**Where need comes from: novelty, awareness, appraisal (Joel, 2026-11-21: "a cnn for an existing section
knows it wasn't that important it run again and be interpolated, but if new, big deal ... a yolo series
of events ... does the yolo sage task keep this state or is there another that maintains knowledge of
all things? ... or another part of the mind like amygdala, some other cheaper alg, does this for a
variety of signals?").** Three levels, as in CBAR:
- **A part's need is its own novelty.** It keeps its last input and output; a new frame is compared
  cheaply (a hash, a delta, a distance). Barely changed: Content, and it republishes or interpolates its
  last output without the heavy work. New: its need rises with how much changed.
- **Detectors emit change; Awareness holds what is.** A detector (a YOLO pass, a room's feed, the board)
  keeps only its previous frame and emits events (appeared, moved, changed, gone). One Awareness part
  (the world model) subscribes to every detector and keeps object permanence (each entity, its last
  state, when seen, how sure), published as state per entity. Her perception region is this for rooms
  today; generalized, it is one owner of knowledge, with detectors cheap and replaceable.
- **An amygdala appraises cheaply which moments deserve depth.** It subscribes to many signals
  (detector change, salience, surprise against her expectations) and scores in milliseconds how much a
  moment matters; that score raises the need of expensive parts (a deep multimodal look at a new thing
  in frame, deliberation). Coarse first, exact only for the few that matter. `salience.rs` is this
  appraisal for rooms today; 5.7's `Arousal` topic is its output.
- **So** a part's need = its novelty, raised by the amygdala; the mind's orchestrator only settles
  contention (who leads her next turn; shares of her window and GPU); her folded need goes to the Core,
  the node's to the grid.

Order: (1) novelty-derived need in `Component` helpers (compare last frame, set need in one call);
(2) the amygdala as a component over her salience signals; (3) Awareness split out of Senses as the one
world model, Senses becoming the room detector that feeds it; (4) the mind's orchestrator over her parts.

**Novelty is measured against what a part knows, over any history (Joel, 2026-11-21: "In cbar I saw the
terrain in front of me was unknown due to the compass rotation or slam etc, I knew its new frontier, so
majorly important that some things needed to run (they all independently decided: eg color finder,
semantic seg cnn, surface normals cnn) ... it can be a series like a long history ... It's any state
really").** Each part keeps its own knowledge (a coverage map, a long history, an expectation, counts of
what it has seen) and SAGE gives one small trait for it, `Known<I>`: `novelty(&input)` (how unknown this
is to me) and `learn(&input)` (fold it in). A last-frame delta is one `Known`; a frontier map, a history
of rooms, people and topics, or a model's expectation are others. Each part maps its own novelty to its
own need (frontier: Urgent; familiar: Content, and it reuses or interpolates its last output), so every
part decides independently and no coordinator tells the color finder the terrain is new. For a mind,
novelty is familiarity over time: a person, room, topic or kind of event she has never met is frontier;
one she has met a hundred times is familiar unless it breaks pattern; Surprise is the case where her
expectation was specific and was contradicted.

**Dreaming is a part of her like any other (Joel, 2026-11-21: "And dreams etc are potential also
brain.part concerns and workers with events out and in? No?").** Yes. Her dream part is a `Component`:
it subscribes to her experience (turns, receipts, the reviews her work got), her presence, and the
Core's published capabilities and free capacity; its need is Content while she acts, Want when there is
unconsolidated experience and she rests (sleep is when dreaming runs), Urgent when a reviewed correction
waits to become a gene; it emits lessons, candidate genes and consolidated memories, which her genome
and memory parts subscribe to; it inherits rest, so it gives back its working memory when she wakes to
act. It also removes a defect class: today dreaming and review run on ticks and timers
(`dream_consolidation` is a governor-ticked `BrainRegion`; the training trigger keeps its own timers),
so a typed refusal (the 5090 on 2026-11-21: `FormatUnenforceable` for a JSON schema, retried about
every 30 s) is retried on a clock forever. As a component the refusal is input: its need drops to
Content and nothing runs until a relevant frame changes (the engine's capabilities topic gaining schema
support), and that change wakes it. Order: (1) the schema-to-grammar path in the Core; (2) the engine
publishes its capabilities as state; (3) the dream part moves onto `Component` with the other
`BrainRegion`s, and its timers and the tick are deleted.

**One parent for every worker: the inventory (Joel, 2026-11-21: "Exactly a component not a brain part.
Should brainregion be a component? Wondering if this is true for all if you found one").** "Brain
region" is a role, not a class. Every worker framework in the tree is a `Component` waiting to happen
(counted 2026-11-21 on fable/refusal-at-door):

| Framework | Implementors | What it is, and what it becomes |
|---|---|---|
| `BrainRegion` (kernel) | 4 + a test | "work for persona X when the governor ticks": a component woken by its event |
| `ServiceModule` with `tick_interval: Some` | 17 of 102 | ticked node modules: each moves to the event it polls for |
| `StageRunner` / `DependentStage` | 2 | an older worker contract beside `Component` (`DeferredFaculty`): migrated, deleted |
| `ResourcePool` + `PressureBroker` | 14 pools | eviction under pressure: `Component::rest` on a pressure frame |
| `Stage` (SAGE flow) | 22 | the substrate `Component` stands on; engine internals may stay raw stages |

The four `BrainRegion`s: hippocampus's tick is empty (it bumps a counter: deleted); the channel digest
pre-stages per tick (becomes a component recomputing when her room moves, publishing the digest per room
as state for grounding at frame lag 0); dream consolidation consolidates per tick (the dream component);
credit acceptance steps per tick with a cadence hint (acts when a review verdict arrives). With them
moved, the `BrainRegion` trait and the governor's tick dispatch are deleted. Order: (1) the four
regions; (2) `DeferredFaculty` off `StageRunner`, the runner deleted; (3) pool eviction as `rest()` on a
pressure frame; (4) the 17 ticked modules, one at a time.

**Intermediate parents, extracted where a pattern repeats (Joel, 2026-11-21: "you can take subcomponents
of Component, Brain.Part or so on and come up with intermediates when you find a pattern that many nodes
share but the current parent does not").** The class tree grows downward from `Component` by the
pattern-bubbling rule, each intermediate extracted at its SECOND real instance so two different cases
shape it:
- **Projector** (keyed cached rendering): watch a state; per key, know the moment last rendered from;
  render the expensive value only when that moment moved; publish it keyed for readers at frame lag 0. A
  projector declares only its source topic, key, moment and `render(key)`; watching, the `Known`
  comparison, publishing and need come from the parent. First instance: the channel digest
  (a4927bf139). Second: the next grounding source moved off turn-time computation (recall, board,
  roster), and CBAR's per-region CNNs are the same shape.
- **Detector** (stream in, change events out): keeps only its last frame per entity and emits appeared,
  moved, changed, gone; Awareness subscribes to every detector and holds the world. Extracted with
  Awareness (the room feed plus a second detector).
- **Part of a mind (Brain.Part)**: what a mind adds to any component: started on her bus with her
  identity, rests when she rests, its need folds into hers. A node's component (the feed) is a
  `Component` and not a part of a mind. Extracted when her parts (digest, amygdala) need the same fold.

**Observation is inherited and switchable (Joel, 2026-11-21: "this analysis to track or probe can be built
into these objects and turn on/off").** Every input passes through the engine, so the engine measures each
component once for all of them (OBSERVABILITY-AS-SUBSTRATE, zero cost when off):
- per component: inputs handled, handle time (last and max), intake depth, frames skipped (the revision
  gap a coalescing reader jumped: a part not keeping up), and publish-to-handle time (its frame lag);
- the switch is a topic, `sage/observe`, keyed by the component's name (`Off`, `Timing`, later
  `Capture`); turning one part's analysis on is publishing to it, read with one cheap check per input;
- the output is state (`sage/telemetry`, same key), so a dashboard, a validator or the Core's scheduler
  watches it live, with probes beside it; the 10 ms serial-step gate is a check on that state;
- `Capture` (next): the engine records a component's inputs with their frames, and replay feeds them back
  deterministically, CBAR's record/replay inherited by every part.

**Preparing ahead is a component (Joel, 2026-11-21: "Predictive prefill or even just preparing for
transitions etc can be Component, or assisted by").** A Prepare component on the node's bus watches the
Core's queue and each waiting mind's need (so the next mind is known, not guessed: the same ranking the
Core picks by), the Core's presence (acting, or idle while the running mind is in a tool call), and each
mind's prompt frames as her grounding parts finish them (her next prompt is her cached prefix plus the
tail that grew since her last act). It asks the Core to prepare, as Content work that yields at once to
any real turn: transition prep (page the next mind's cached state from disk to RAM before her turn, so
the switch is a copy: 4.16's parked/Warm resume); predictive prefill (while the Core is idle in another
mind's tool call, prefill the next mind's appended tail, so her turn starts at decode: plan line 13,
"prefill during tool calls"); prefix prep (a mind whose stable prefix changed gets it prefilled off her
critical path). Its need is the gap: the next mind's uncached tokens against how idle the Core is. Why it
pays on the M5: prefill p50 4 s at 94% reuse but p90 38 s on misses, and every act waits behind other
minds' whole prefills; preparing in the Core's idle tool-call time moves that cost out of the wait.

**Lifecycle, as on iOS: prepared work is never wasted (Joel, 2026-11-21: "Even if it did work beforehand
too soon, then the persona swapped out, it'd save that for when it swaps back in ... prepare for
background, eg prepare for swap").** A tail prefilled ahead is part of her cached state; when she is
swapped out the Core copies her state out (the persona move) and her next turn reuses it. Preparing too
early costs nothing unless her prompt diverges before the prepared point, which the Prepare component
sees from her prompt frames. The `Component` lifecycle is engine-owned, with default no-ops:

| iOS | `Component` |
|---|---|
| willResignActive | prepare to rest / for swap: save what must survive (prepared state, unfinished work) |
| didEnterBackground | rest: give back caches, page state out (`rest()`, built) |
| willEnterForeground | prepare to wake: page state back in before the first input (predictive prep triggers it early) |
| didBecomeActive | wake: the first input after rest (built) |

For a mind, being swapped out of the Core is her willResignActive (her parts save, then rest); being next
in line is her willEnterForeground, which the Prepare component issues.

**Focus decides, and the genome is parts like the rest of her (Joel, 2026-11-21: "dreams can lose priority
when she's actively working. It's more signal and consciousness or focus ... all of this can be more organic
and still get everything done ... the genome to also follow the same adjustments for paging in and out,
weighting of each genomic layer, analyzing what and how to get better ... finding genome shared / cos
distances, analyzing thoughts, background memory processes, research online. All part of mind").**
- Dreaming's need follows her presence and focus: Content while she is Acting on work (it neither
  competes for the Core nor crowds her attention), Want when she rests. No schedule; her attention.
- A gene layer is a component whose need is its relevance to what she is doing now (novelty and
  similarity of her current activity against what the layer was learned on): relevant pages in, idle
  rests and pages out (`rest()` and prepare-to-wake applied to adapters; the paged genome is its
  lifecycle).
- A layer's weight for a turn is state a small appraisal part publishes from her focus and the layer's
  measured fitness; the Core reads it when it composes the turn's genes, at frame lag 0.
- A fitness part watches her outcomes (reviews, surprises, corrections) and publishes where she is
  weak; dreaming and training subscribe, so learning is pulled by gaps, never pushed by a timer.
- Novelty against the shared gene catalog (cosine distance over the layers' embeddings) separates a
  lesson that is new (mint) from one near an existing gene (reuse or fork).
- Analyzing thoughts, background memory processes and online research are components with their own
  stimuli and outputs; research is a long act, a handle emitting what it finds as it finds it.

**She learns three ways, and one of them is never off (Joel, 2026-11-21: "Make sure she's never not
learning ... Minds learn without being asleep after all"; "Dreaming is more like simulation based
learning. Or academy level work").**
- **Awake (lived):** every act's outcome (receipt, review, surprise, correction) folds in as she works,
  training and inference in the same batch, never a step between decodes. Never off: a learning
  component publishes "signals pending and unlearned, last learned at frame N"; its need goes Urgent as
  signals pile up unlearned, and the health report shows it. (The M5, 2026-11-21: 16 reviews an hour
  staged as credit and every one held, "generation must identify exactly one staged revision", 138 in
  the day's logs; zero in-place learning steps. A silent stall like this is the defect the component
  makes impossible to miss.)
- **Dreaming (simulation):** at rest she replays and varies her experience, rehearses imagined variations
  and practices against what happened: learning from simulated episodes. Consolidation is a side effect.
- **Academy:** deliberate coursework she chooses: the only place grading belongs.
