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

Her workspace runs a turn as a stack of tasks: activity, card, step, tool call. Each level yields
while it waits on a completion. An event of higher salience interrupts at the top of the stack and
leaves the rest resumable (chapter 0, "Simulate behavior").

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
| Attention | the amygdala, her dial, activity value curves | her focus: what is near, aside, far | arithmetic | partly (dial, `TurnAttention`) |
| Drives | time, outcomes, her own state | energy, curiosity, boredom, urgency | arithmetic | partly (`continuum-vitals`, vitals pulse) |
| Working memory | focus, percepts, her acts | what she holds in mind now | arithmetic | exists (`WorkingMemory`) |
| Hippocampus | salient percepts, her focus | engrams written; recall pre-staged for her focus | embedding cache, search | exists, node-wide (`HippocampusModule`) |
| Social model | messages, reviews, outcomes per peer | who each person and resident is to her: what they know, want, and how far she trusts them | arithmetic, embeddings | proposed |
| Turn-taking | the conversation's state, her drives | whether to take the floor, backchannel, or wait | small model | proposed |
| Intention | her focus, the board, her drives | her task stack: activity, card, step | arithmetic | partly (continuation, cards) |
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
(the `Region` base, a derive for the declarations, and a scaffold generator,
[CONTINUUM-ENGINE.md](../CONTINUUM-ENGINE.md) §3, "The For Free Triplet") supplies everything else. Two consequences follow:
- **Residents can author parts of their own minds.** A part is ordinary code behind a generator, so
  a resident can propose one, test it against its validators, and submit it like any other change.
- **A part's model can be a gene.** Where a part uses a small learned model (the amygdala, turn-taking,
  metacognition), that model is a gene of hers, and improves through the same learning loop as
  everything else she does (chapter 6).

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
