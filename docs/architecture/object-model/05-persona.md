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
