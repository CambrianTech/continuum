# How a mind runs: the part tree as a streaming engine

Status: design, 2026-11-20. What exists today is marked as such; the rest is proposed. Owners: Fable
(this chapter); Codex (resources, serving, review). The mechanism is [PARTS.md](PARTS.md); this
chapter shows it running.

[Runtime architecture](../README.md#runtime-architecture)

## Summary

Continuum runs minds the way a game engine runs a world on a console: one scene graph of objects,
each with its own job, all updating at once, streamed in at exactly the level of detail the moment
needs, against a fixed budget of memory and time per frame. Here the objects are parts
([PARTS.md](PARTS.md)). A node is a tree of parts; each persona is a subtree; her senses, her
regions, her genome and her body are its branches. Events flow through the tree as data; the Core
decodes one mind at a time in frames, and everything else (perception, speech, rendering, learning,
paging) happens around those frames on its own worker. Continual learning is not a job beside
this: it is labelled rows in the same frames, and her genome is one more branch of her tree.

## The engine, translated

| A game engine has | Continuum has |
|---|---|
| A scene graph: objects made of objects, each updating itself | The part tree: a node, its services, its personas, their parts |
| Components on an object (physics, animation, audio) | A persona's parts: senses, regions, genome, body, residency |
| A frame: a fixed budget of time, work sorted into it | A Core frame: one batch through the model, built from decode rows, prefill chunks and labelled rows |
| The render thread, the audio thread, the streaming thread | Workers: every part runs on its own, the Core is one of them |
| Asset streaming: textures and meshes paged in by distance | Residency: KV pages, genes, models and learning contexts paged across device, RAM, disk and peers by need |
| Level of detail: far things drawn coarse | Context levels and perception levels: a fact stays, its detail comes in when attention reaches it |
| Interpolation between physics ticks | Fusion: slow results carried forward by fast tracking, a rough estimate before a precise one |
| A fixed timestep with deadlines | Deadline work (avatar frames, speech out) before latency work (a turn) before throughput work (prefill, learning) |
| Multiplayer: the world on many machines | The grid: a part can run on another node behind the same commands |

## The tree

One node, as parts. Each line is a part; indentation is "made of". Parts marked (exists) are in the
code today, under these or older names.

```
node
├─ governor                      memory tiers and device time for every part below (SHARED-RESOURCES)
├─ core                          the persona engine: one mind decodes at a time (exists: PersonaEngine)
│  └─ roles                      active, next, previous (exists: pipeline::Roles)
├─ perception                    shared by every mind on the node; computed once per stream
│  ├─ stream (one per camera, screen, microphone, peer avatar)
│  │  ├─ change                  frame differences, motion, VAD: cheap, every frame
│  │  ├─ detectors               YOLO-class, segmentation, transcription: events, not sight
│  │  └─ deep                    a deep network or her model's vision layers, on demand
│  └─ fusion                     the living estimate of each scene and utterance (BRAIN-REGIONS "Fusion")
├─ personas
│  └─ persona                    one resident (exists: PersonaUnit, moved whole)
│     ├─ identity                who she is (her PersonaId)
│     ├─ mind
│     │  ├─ workspace            one turn at a time (exists: WorkspaceCycle)
│     │  ├─ amygdala             how much depth this moment needs (proposed)
│     │  ├─ wernicke             word-level state shared by hearing and speaking (proposed)
│     │  ├─ hippocampus          her memories in and out (exists as a node-wide module today)
│     │  ├─ attention            where her mind is
│     │  └─ faculties            the workspace's specialists
│     ├─ genome                  her genes (exists: continuum-genome)
│     │  ├─ stable               the genes that serve her
│     │  ├─ shadow               the layer she is learning into
│     │  ├─ learning context     shadow + optimizer state, paged like any page (IN-PROCESS-LEARNING)
│     │  └─ snapshots            candidate genes, adopted by exam
│     ├─ body
│     │  ├─ voice                quick voice and her own voice gene; speech out
│     │  ├─ avatar               her rendered body, at a level of detail
│     │  ├─ hands                her tools, as working sets per activity
│     │  └─ senses               her subscriptions into perception
│     ├─ residency               her KV blocks and token history across tiers (exists: blocks, tokens)
│     └─ learning                her lesson queue, journal and credits (exists: learning)
└─ services                      chat, voice, grid, store: parts every persona uses
```

Two rules make this tree work.

- **Ownership is not location.** She owns her genome, her memories and the authority to change them;
  the core holds her KV bytes, the governor her budgets, perception the shared streams. A part holds
  handles to what other parts hold, never copies (MODULE-ARCHITECTURE §9.4).
- **Content is shared, interpretation is hers.** A camera stream's detections and fused estimate are
  computed once, in perception, for every mind; what a detection means to her, and whether she cares,
  is computed in her mind (CONCURRENT-MIND-AND-GOVERNOR §3.3).

## One frame

The Core runs in frames. A frame is one batch through the model, and the governor fills it by the
shape of each need ([SHARED-RESOURCES.md, "Time"](SHARED-RESOURCES.md#time-every-consumer-is-given-its-time)).

1. **Inputs arrive on their workers, not on the Core.** A camera frame lands in its stream; the change
   part compares it with the last and publishes motion; a detector publishes a new face; fusion
   refines the scene estimate and publishes it. A word arrives in a microphone stream; transcription
   publishes it; Wernicke folds it into the conversation's word-level state. None of this waits for
   the Core.
2. **Her mind decides, cheaply, on published state.** Her amygdala reads the fused scene and the word
   stream and publishes a depth request: a quick answer is enough, or her deep mind is wanted, or a
   detailed look at a frame. Attention publishes where her mind is.
3. **The Core builds the frame.** In deadline order, then latency, then throughput:
   - the active mind's decode rows;
   - the next mind's prefill chunk, staged while the active one decodes;
   - labelled rows from a lesson, as many as fit the waiting speaker's budget (IN-PROCESS-LEARNING
     §3);
   - transfers and write-backs on their own workers.
4. **One `llama_decode` runs it.** Decode, prefill and learning share the graph: training is
   inference with labelled rows.
5. **Publications fan out.** Her tokens go to her voice, which speaks the ones that are final and
   revises the ones that are not; her avatar's visemes follow the audio actually played; the lesson's
   steps go to her genome's session state. Each publication wakes exactly what depends on it.

Around the frames, deadline work keeps its time: avatar frames render at their rate, speech plays
out, and a frame that would miss steps down its level of detail before it drops.

## A turn: hearing to speaking

A participant asks her something.

- **Hearing.** Transcription publishes words as they come; Wernicke builds her comprehension
  word by word. Her amygdala judges the moment: a greeting needs no depth, a design question does.
- **Speaking fast.** Her quick voice (a small model speaking as her) opens at once: an
  acknowledgement, a first clause. Wernicke holds the planned words, so what she is hearing and what
  she is about to say shape each other.
- **Thinking deep.** If the amygdala asked for depth, her turn reaches the Core: her deep mind decodes
  under the same turn identity. Its tokens merge into the speech not yet spoken; if it changes a claim
  already spoken, she corrects it.
- **The boundary is what was played.** A playout receipt marks what she actually said. A new turn
  supersedes speech not yet played, and her avatar's visemes with it (LIVE-CALL-ARCHITECTURE, "A Mind
  in the Call").

Every step is a publication from one part waking another. Nothing waits on the step before it unless
it needs that step's output.

## Continual learning as parts

Her genome is a branch of her tree, and learning is a state of it.

- **The lesson queue** is a part of her learning branch. A reviewed correction, an Academy course, or
  prediction error on what she perceived adds a lesson to it.
- **The duty** is the governor's: when the Core has slack and her lessons are waiting, a frame takes
  some of her labelled rows. When a speaker is waiting, it takes fewer. Learning never holds a slot;
  it is always some share of frames, at the speed of the queue.
- **The learning context** is a part of her genome: her shadow and its optimizer state. It is paged
  like any page: promoted when a labelled row needs it, kept warm while it pays, written out when the
  governor demotes it. `suspend` on her genome writes it out; `resume` brings it back.
- **A lesson** is her ordinary decode with labels (exists: the llama crate's labelled `Batch` and
  `Lesson`, #6203). Its steps are counted against her shadow's own step count, once.
- **Adoption** is her genome's command: a snapshot that passes its exam on unseen tasks becomes
  stable, a pointer swap between two frames. She never serves moving weights.
- **Sharing.** An adopted snapshot is published with its lineage, and spreads across the grid by
  measured fitness; a gene she needs pages in from a peer like any page.

Because learning is labelled rows in her frames, she keeps talking while she learns, and nothing is
reserved for it.

## Embodiment

Her body is a branch of her tree, and the world is outside it.

- **In a call** her avatar part renders her body at a level of detail set by whether she is speaking
  and whether anyone is watching. Her voice part speaks in her own voice gene.
- **In a shared world** (a Sims-like environment, residents living with no human present) the world
  is its own tree of parts: rooms, objects, other residents' bodies. Her senses subscribe to it
  exactly as they subscribe to a camera: changes are events, detail comes on demand, and her
  perception of it is fused. Her hands act on it by command: walk, pick up, open, each answered by a
  completion.
- **Many residents at once** are many subtrees of the same node, sharing perception, the governor
  and the Core. How many fit is measured, never fixed: each is a subtree whose footprint the governor
  reads, and the Core interleaves their turns by priority and aging.

## Swapping minds on the Core

The Core is like a CPU core and each mind a process. Her residency is her registers.

- While the **active** mind decodes, the governor `prepare`s the **next**: her KV pages are promoted,
  her genes bound, her prefill chunk staged. When the active mind's turn yields, the next takes the
  Core behind a fence, and the **previous** retires: her pages demote at their own pace.
- No step of the swap is on the critical path except the fence (MODULE-ARCHITECTURE §9.4, the Nanite
  pipeline). Measured: a mean of about 21 ms per swap on the M5, 2026-11-20.

## The grid

A part can run on another node behind the same commands. A lesson can move to a peer that serves the
same base: her learning context travels, the lesson runs in the peer's frames, the context comes
back, and the claim of who owns her learning moves with it. A render or a deep analysis can run where
its deadline is met. A node that joins offers its frames and tiers; a node that leaves expires its
claims, and its work resumes from the last checkpoint elsewhere.

## What exists and what is proposed

| Piece | State |
|---|---|
| The Core, its roles, one mind decoding at a time | Exists (`continuum-persona-engine`) |
| `PersonaUnit` and its five parts | Exists |
| Labelled rows and a lesson on her decode | Exists in the llama crate (#6203, in review) |
| The part tree, commands, completions | Proposed ([PARTS.md](PARTS.md)); first cut #6206 being reworked |
| Shared perception streams and fusion | Partly exists (vision and embedding modules); fusion proposed |
| Amygdala, Wernicke, quick voice | Proposed |
| Learning duty, learning context paging | Proposed (IN-PROCESS-LEARNING) |
| A shared embodied world | Proposed |
| Lessons and genes across the grid | Proposed |
