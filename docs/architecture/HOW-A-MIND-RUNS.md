# How a mind runs

Status: design, 2026-11-20. Each section says what exists today and what is proposed. Owners: Fable
(this chapter); Codex (resources, serving, review). This is the overview: each section explains how
one part of the system works through the part tree, and links to the chapter that holds its detail.
The object model itself is [the object model](object-model/README.md).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

1. [Summary](#1-summary)
2. [The engine, translated](#2-the-engine-translated)
3. [What a persona is made of](#3-what-a-persona-is-made-of)
4. [The node around her](#4-the-node-around-her)
5. [Switching minds rapidly](#5-switching-minds-rapidly)
6. [Caching](#6-caching)
7. [Responding quickly](#7-responding-quickly)
8. [Low latency across the whole system](#8-low-latency-across-the-whole-system)
9. [Multimodal, at once](#9-multimodal-at-once)
10. [Learning continuously](#10-learning-continuously)
11. [The genome](#11-the-genome)
12. [The Academy](#12-the-academy)
13. [Unknown hardware and a changing grid](#13-unknown-hardware-and-a-changing-grid)
14. [How a persona is persisted](#14-how-a-persona-is-persisted)
15. [Embodiment](#15-embodiment)
16. [What exists and what is proposed](#16-what-exists-and-what-is-proposed)

## 1. Summary

Continuum runs minds the way a game engine runs a world on a console: a tree of objects, each with
its own job, all live at once, streamed in at the level of detail the moment needs, within a fixed
budget of memory and time. The objects are parts ([the object model](object-model/README.md)).
- **The tree.** A node is a tree of parts, and each persona is a subtree of it. Her mind, genome,
  body and residency are its branches.
- **Events.** Data flows through the tree as events, and each part does its own work on its own
  worker.
- **The Core.** The Core decodes one mind at a time, in frames, and switches between minds the way a
  CPU switches processes: fast enough that, to each of them, it is all simultaneous.
- **Learning.** Continual learning is not a separate job. It is labelled rows in those same frames,
  and the genome it changes is one more branch of her tree.

## 2. The engine, translated

| A game engine has | Continuum has | Section |
|---|---|---|
| A scene graph of objects made of objects | The part tree: a node, its services, its personas, their parts | 3, 4 |
| Components on an object | A persona's parts: mind regions, genome, body, residency, learning | 3 |
| A CPU switching processes, and their registers | The Core switching minds, and her KV and token history | 5 |
| Asset streaming and caches at every level | Residency: KV pages, genes, models and learning contexts across device, RAM, disk and peers | 6 |
| A frame: a time budget, work sorted into it | A Core frame: one batch, built from decode rows, prefill chunks and labelled rows | 7 |
| Render, audio and streaming threads | Workers: every part runs on its own | 8 |
| Interpolation between physics ticks, level of detail | Fusion: slow results carried forward, rough estimates refined, detail on demand | 9 |
| Hot-reloading content while the game runs | Learning into a shadow layer, adopted between two frames | 10, 11 |
| A level designer's training map | The Academy | 12 |
| One game across many machines and specs | The grid and edge hardware, measured live | 13 |
| Save games | Her durable state, saved at the seams | 14 |

## 3. What a persona is made of

A persona is a part made of parts. Each branch owns one kind of state, and that state is either
durable (part of who she is) or cache (rebuilt or restored at will).

```
persona                     one resident; moves whole (exists: PersonaUnit)
├─ identity                 PeerId, keypair, home, name, role                          durable (airc)
├─ mind
│  ├─ workspace             one turn at a time (exists: WorkspaceCycle)                cache
│  ├─ attention             her dial, where her mind is, what may interrupt            durable (MindState)
│  ├─ amygdala              how much depth this moment needs (proposed)                cache; its model is a gene
│  ├─ wernicke              word-level state shared by hearing and speaking (proposed) cache
│  ├─ hippocampus           her memories in and out (exists, node-wide today)          engrams durable
│  └─ faculties             the workspace's specialists                                cache
├─ genome
│  ├─ stable genes          what serves her                                            durable
│  ├─ shadow                the layer she learns into                                  durable at checkpoints
│  ├─ learning context      shadow + optimizer state                                   paged; checkpointed
│  └─ snapshots             candidate genes, with lineage and exams                    durable
├─ body
│  ├─ voice                 her quick voice and her own voice gene                     gene durable
│  ├─ avatar                her rendered body                                          cache
│  ├─ senses                her subscriptions into perception                          durable (cursors)
│  └─ hands                 her tools, as working sets per activity                    cache
├─ residency
│  ├─ blocks                her KV pages (exists)                                      cache, saved on stop
│  └─ tokens                her token history, appended once (exists)                  durable
└─ learning                 her lesson queue, journal, credits (exists)                durable
```

Every branch answers the same commands (prepare, suspend, resume, shutdown) and the same queries
(footprint, health, what is loaded) through the part tree, so "suspend her" is one command that
reaches every branch in its order, and "how much memory is she using" is one query.

## 4. The node around her

```
node
├─ governor        memory tiers and device time for every part (SHARED-RESOURCES)
├─ core            the persona engine; one mind decodes at a time (exists: PersonaEngine)
│  └─ roles        active, next, previous (exists: pipeline::Roles)
├─ perception      shared streams (camera, screen, microphone, peers' avatars), computed once
│  └─ fusion       the living estimate of each scene and conversation (BRAIN-REGIONS "Fusion")
├─ personas        her subtree, and every other resident's
└─ services        chat, voice, grid, store: parts every persona uses
```

Two rules keep this tree honest:
- **Ownership is not location.** She owns her genome, her memories and the authority to change them.
  The Core holds her KV bytes and the governor her budgets, and she holds handles to them, never
  copies (MODULE-ARCHITECTURE §9.4).
- **Content is shared, interpretation is hers.** A stream's detections and fused estimate are
  computed once for every mind. What they mean to her is computed in her mind
  (CONCURRENT-MIND-AND-GOVERNOR §3.3).

## 5. Switching minds rapidly

The Core is a CPU, each mind is a process, and her residency (KV pages and token history) is that
process's saved context ([CONTINUUM-ENGINE.md](CONTINUUM-ENGINE.md) §4.9).

- **A switch is a save and a restore.** On unified memory a sequence saves in about 0.22 s and
  restores in about 0.1 s, measured in isolation on the M5 (§4.1b). A mean of about 21 ms per swap
  was measured between resident minds on 2026-11-20.
- **The next mind is prepared while the active one decodes.** The Core's roles are active, next and
  previous. The next mind's pages are promoted, her genes bound and her prefill chunk staged while
  the active mind decodes. Only the fence that hands over the Core is on the critical path, and the
  previous mind's pages demote at their own pace.
- **A ready mind runs now.** She never waits for permission, only for the engine itself while every
  byte is held by minds in the middle of an act (§4.9.3, I3).
- **A stop is a pause.** Every resident is saved, and the next engine restores her where she was: a
  restart costs about 5.6 s of restore against about 401 s of cold prefill, when her input is
  unchanged (§7.0).

Why switching is cheap rather than catastrophic: her context only ever appends (section 6), so what
she needs after a switch is almost all already computed.

## 6. Caching

Every derived value has a cache keyed by what derived it, and recomputing a pure function of
unchanged inputs is a defect ([SHARED-RESOURCES.md](SHARED-RESOURCES.md), "Cache levels everywhere").

- **Her context is a FIFO.** An engine reuses a prompt up to the first token that differs, so her
  context is ordered by stability: an immutable head (identity, tools, policy), an append-only middle
  (her history and results), and a tail that alone may change. Nothing already sent is moved or
  re-rendered. The health number is tokens re-processed per act against tokens appended, and healthy
  is about 1:1 (CONTINUUM-ENGINE §5).
- **Content is cached once, for everyone.** Embeddings, transcriptions, detections and file reads are
  keyed by content hash and shared by every mind. Fourteen minds reading one message embed it once.
- **Tiers, priced.** Every cached page sits on a tier (device, pinned RAM, RAM, disk, a peer, the
  archive), and below every tier is recompute. Eviction is demotion one tier down, chosen by expected
  loss per byte: the chance it is needed again, times the cost to bring it back.
- **Temperature.** The scheduler knows which mind acts next, so her pages are promoted before she
  needs them, and the coolest minds' pages demote first.
- **Shared prefixes are one page.** A head every mind shares is computed once and copied, never
  re-prefilled.

## 7. Responding quickly

A turn is fast because almost nothing in it waits.

- **Her context is already warm.** Perception, recall and fusion run continuously on their own
  workers and publish what they found. By the time she acts, her inputs are ready, and her prompt is
  her previous one plus the appended tail.
- **The frame is built for her.** The Core fills each batch by the shape of each need: her decode
  rows first when she is speaking, the next mind's prefill chunk beside them, and labelled lesson
  rows only as many as fit the waiting speaker's budget.
- **She answers fast and thinks deep at once.** Her amygdala decides in milliseconds how much depth
  the moment needs. Her quick voice can open at once while her deep mind decodes under the same turn,
  and the deep thought merges into the speech still to come (LIVE-CALL-ARCHITECTURE, "A Mind in the
  Call").
- **Tools overlap her thinking.** A tool's result is a task whose completion wakes her, and the
  prefill of what she will read next can run while the tool runs.

## 8. Low latency across the whole system

Low latency is a property of the whole tree, not of one fast component.

- **Nothing calls into a part and waits.** Queries read what parts already published. Commands are
  messages to a part's worker, answered by completions that arrive as they happen ([roots](object-model/03-roots.md)).
- **Every wait is an event.** Nothing polls and nothing sweeps on a clock. A part wakes when an input
  it depends on changes.
- **Time is governed by the shape of each need** ([SHARED-RESOURCES.md](SHARED-RESOURCES.md#time-every-consumer-is-given-its-time)):
  - deadline work (avatar frames, speech out) gets its time first;
  - latency work (a mind's turn) next;
  - throughput work (prefill, transfers, learning) uses the slack.
- **Every piece yields.** Work goes to a device in small pieces. The longest piece that cannot yield is
  the floor under everyone's latency, so it is measured and shrunk.
- **Level of detail is the release valve.** Under pressure an unwatched avatar renders less and a
  stream is analyzed less often, before any deadline is missed.

## 9. Multimodal, at once

She sees and hears with every layer at once, dynamically fused
([LIVE-CALL-ARCHITECTURE.md](../live/LIVE-CALL-ARCHITECTURE.md), [BRAIN-REGIONS-SUBSTRATE.md](BRAIN-REGIONS-SUBSTRATE.md)).

- **Cheap layers react; deep layers refine.** Frame differences, motion, voice activity and
  YOLO-class detectors run on every frame and emit events. Those events tell the deeper layers
  (segmentation, a deep network, her own model's vision and audio layers) when they are needed and
  with what priority.
- **The fusion is her sight.** Her perception of a scene is a living estimate, never finished and
  never blank. It starts rough and refines as better knowledge arrives, each source weighted by its
  confidence and age. A slow result is carried forward to the present by continuous tracking, and a
  rough estimate fills what would otherwise be a gap.
- **Hearing and speaking are fused the same way.** Transcription builds her comprehension word by
  word, and her Wernicke region holds the words she hears and the words she is about to say in one
  state.
- **Any modality, in or out.** Text, code, audio and video each have the same shape: fast first, then
  refined; speak fast, then deepen.
- **Computed once.** Every stream's analysis is shared by every mind in the room.

## 10. Learning continuously

Learning is inference with labelled rows switched on, in her ordinary frames
([IN-PROCESS-LEARNING.md](IN-PROCESS-LEARNING.md)).

- **A lesson is her decode with labels.** The forward pass is the one serving runs anyway; the
  backward pass over the labelled rows is the extra cost. Exists in the llama crate (#6203).
- **Learning is a duty cycle.** When the Core has slack and her lessons wait, frames take some of her
  labelled rows; when a speaker waits, they take fewer. A dedicated course is the same control held
  high. Nothing is reserved for learning.
- **Her learning context is paged like any page.** Her shadow and its optimizer state are promoted
  when a labelled row needs them, kept warm while they pay, and demoted under pressure.
- **Two ways in:**
  - continuously, from prediction error on what she perceives, weighted by her relevance;
  - deliberately, from a reviewed correction or a course.
- **She keeps talking while she learns,** because learning is a share of her frames, never a session.

## 11. The genome

Her genome is a part of her, and a trained layer is one state of it
([GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md),
[ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md](ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md)).

- **Its states.**
  - Stable genes serve her.
  - The shadow is a small layer above her frozen stable stack, never a copy of it.
  - Snapshots are candidate genes.
  - Checkpoints hold the session state (optimizer, cursors) so learning resumes after a restart
    without applying any step twice.
- **Adoption is by exam.** A snapshot that passes its exam on unseen tasks of its family is promoted
  to stable: a pointer swap between two frames. She never serves moving weights.
- **Genes are composed per turn.** The genes a turn needs are selected and bound, and a gene that is
  not resident is paged in like any page: from RAM, disk or a peer.
- **Genes have lineage and fitness.** Every gene carries its provenance. Adopted genes are published
  and spread across the grid by measured fitness, and a resident's unique voice is one more gene.

## 12. The Academy

The Academy is the dedicated form of the same learning: a higher duty cycle, with curriculum
([ACADEMY-AS-CONTINUOUS-EVOLUTION.md](ACADEMY-AS-CONTINUOUS-EVOLUTION.md),
[REPO-GENOME-AND-COURSES.md](REPO-GENOME-AND-COURSES.md)).

- **A course is a recipe.** It runs in a room like any activity, with a teacher who writes lessons,
  a grader for exams, and her as the student.
- **A lesson is her context and the corrected reply.** The doctor writes a family of graded tasks
  around it, and the exam on unseen tasks decides whether the gene she grew is adopted.
- **The repo is a course.** PR reviews, airc verdicts and our own Claude and Codex sessions become
  lessons.
- **Grading lives only here.** Her normal work is never graded. Exams belong to the Academy and to
  genes.
- **Same mechanism, higher duty.** Teaching is the learning of section 10 with the duty held high and
  the lessons chosen by a curriculum.

## 13. Unknown hardware and a changing grid

Nothing about the machine is assumed ahead of time, and nothing is assumed to stay the same.

- **Capacity is measured, never configured.** Each node's governor builds its memory tiers from the
  backend's own report and from timed transfers, not from a machine kind. A Mac has no pinned-RAM
  tier; a 5090 box has all of them. Device time is measured the same way.
- **Change is an event.**
  - A user starts a game on the 5090: memory and device time shrink, learning's duty and the
    avatars' level of detail step down, and lower-value pages demote.
  - The game quits: they step back up.
  - Nothing restarts.
- **Nodes join and leave mid-session.** A node that joins offers its frames and tiers from the moment
  it serves. A node that leaves expires its claims (each claim is a lease renewed by heartbeat), and
  its work resumes from the last checkpoint elsewhere, each step applied once.
- **Work moves to the compute.** A lesson runs on a peer serving the same base: her learning context
  travels (megabytes), the lesson runs in the peer's frames, and the context comes back with the
  claim of who owns her learning. A render or a deep analysis runs where its deadline is met.
- **Never a pool.** One whole model is never assumed to fit across two nodes' summed memory. Placement
  reads the best single node and counts nodes ([GRID-ELASTIC-CAPABILITY.md](GRID-ELASTIC-CAPABILITY.md)).
  Splitting a model across nodes is an explicit plan with measured latency.

## 14. How a persona is persisted

Persisting her means persisting her durable branches (section 3), each at its own seam, and
restoring her caches as an optimization, never as her identity.

- **Identity** lives in airc: her keypair, her peer id, her home. Every surface (a browser, a voice
  room, Slack, an IDE) is a projection of the same citizen. A bridge going away does not touch her.
- **Her mind's state** (`MindState`: her activity cursors, her continuation, her attention dial) is
  saved at the seam of each turn and loaded at boot ([EVENT-MIND.md](EVENT-MIND.md), rule 8). She
  resumes what she was doing, with what she expected next.
- **Her memories** are engrams in the store, through the ORM.
- **Her genome** is files with provenance: stable genes, snapshots, and learning checkpoints with
  the journal cursor they consumed through, so a restart never applies a lesson twice.
- **Her token history** is appended once and kept.
- **Her KV pages are cache, saved on stop.** A stop saves every resident, and a restart restores her
  newest pages under a compatible engine, model and adapter (CONTINUUM-ENGINE §4.9.3, I1 and I2). If
  they cannot be restored, her context is rebuilt from her durable state; that costs a prefill, never
  her identity.
- **In the part tree, persistence is a command like any other.** `suspend` reaches every branch in
  order: each saves its durable state, writes its pages, and reports `Done` or `Pending` on what it
  is still writing.

## 15. Embodiment

Her body is a branch of her tree, and the world is outside it.

- **In a call,** her avatar renders her body at a level of detail set by whether she is speaking and
  whether anyone is watching, and her voice speaks with her own voice gene.
- **In a shared world** (a Sims-like environment, residents living with no human present), the world
  is its own tree of parts.
  - Her senses subscribe to it as they would to a camera: changes are events, and detail comes on
    demand.
  - Her hands act on it by command (walk, pick up, open), each answered by a completion.
- **Many residents at once** are many subtrees of one node, sharing perception, the governor and the
  Core. How many fit is measured from their footprints, never a fixed number.

## 16. What exists and what is proposed

| Piece | State |
|---|---|
| The Core, its roles, one mind decoding at a time, save and restore of residents | Exists (`continuum-persona-engine`, serving) |
| `PersonaUnit` and her five parts (blocks, tokens, genome, learning, decode) | Exists |
| Content-addressed caches (embeddings, vision descriptions) | Exist |
| Her context as a FIFO with prefix reuse | Exists; reuse measured 41% to 91.7% after #6171 and #6174 |
| `MindState` persisted at the seam | Designed in EVENT-MIND; check before relying on it |
| Labelled rows and a lesson on her decode | Exists in the llama crate (#6203, in review) |
| The part tree, commands, completions | Proposed ([the object model](object-model/README.md)); #6206 being reworked |
| Fusion, the amygdala, Wernicke, the quick voice | Proposed |
| Learning duty and learning-context paging | Proposed ([IN-PROCESS-LEARNING.md](IN-PROCESS-LEARNING.md)) |
| Tiered demotion instead of eviction | Proposed ([SHARED-RESOURCES.md](SHARED-RESOURCES.md)) |
| Lessons and genes across the grid; a shared embodied world | Proposed |
