# The whole mind

What a Continuum mind is and how she runs: a persistent citizen living in many activities at once, built the way a
game engine runs a world, whose parts are SAGE components, whose genome is an automatic weighted mixture paged in by
what she is doing, and who learns from her work, in courses and in dreams through one system. Owners: Fable (the
mind, her parts, the codec), Cormac (memory, learning, the genome), Codex (the engine and the grid under her),
BigMama (her live body: voice, video, avatar). The runtime she stands on is
[CONTINUUM-ENGINE.md](CONTINUUM-ENGINE.md) (§1 maps each game-engine system to its chapter).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [1. The laws](#1-the-laws)
- [2. A mind runs like a game engine](#2-a-mind-runs-like-a-game-engine)
- [3. Her parts](#3-her-parts)
- [4. Timescales, all at once](#4-timescales-all-at-once)
- [5. Keyframes and tracking: slow work in real time](#5-keyframes-and-tracking-slow-work-in-real-time)
- [6. The orchestra: every estimator, fused](#6-the-orchestra-every-estimator-fused)
- [7. Many activities, one mind](#7-many-activities-one-mind)
- [8. Memory](#8-memory)
- [9. The genome: automatic, weighted, paged, shared](#9-the-genome-automatic-weighted-paged-shared)
- [10. Three forms of learning, one system](#10-three-forms-of-learning-one-system)
- [11. Many minds on a grid](#11-many-minds-on-a-grid)
- [12. Where it stands, and the order](#12-where-it-stands-and-the-order)

## 1. The laws

1. **Both realms.** Biology, operating systems, networks and game engines each solved a version of this under a
   hard budget. We mirror a solution where it pays and replace it where silicon does better (exact copies,
   snapshots, migration) or worse (storage, bandwidth). Each borrowed name is a quantity we measure and a rule that
   moves it.
2. **Continual and autonomic, never stop-the-world.** Every subsystem refines itself all the time from her state:
   memory, genome, context, placement. Nothing waits until full and then rewrites, and nothing waits for someone to
   schedule it; a result becomes visible at a seam (double buffering).
3. **One budget per resource, demand pulls.** One owner holds it, demand draws where it arises, pressure pushes back
   ([SHARED-RESOURCES.md](SHARED-RESOURCES.md)). No line where work could run together.
4. **Responsiveness is a keyframe problem.** The slow model is the keyframe; buffers and cheap fast layers carry her
   between keyframes, so a several-second act appears real time (§5).
5. **Learning never needs inference idle.** She learns while she works, in place, and wakes within one step.
6. **Decisions never end.** No output is final, so no single attempt must be right, so each can be cheap: many short
   acts with the world pushing back between them over one long act that tries to be right at once.
7. **It never needs intervention.** Dreaming, association, consolidation, gene composition, paging, placement and
   recovery start, tune and unstick themselves. Each manual command an operator runs on one of them marks a defect.
8. **Both ways, or not at all.** Every mechanism that turns a mind down (rest, page-out, a lower share, a smaller
   window) ships with the one that turns her back up, both run in the same validator, and a coma (a mind addressed
   who has not glanced, pressure with no dreaming, work with no act) is a loud, counted defect.

Some of this is a guiding light, not this month's work. Each slice keeps the room open: state and settings as
commands, never a private field or an env var; a controller behind a trait, an algorithm now and a learned policy
later; sources and consumers as adapters; one function per crate on the placement map.

## 2. A mind runs like a game engine

A chatbot is one loop: wait for a message, build a prompt, call the model, return. A mind is built the way RAGE runs
a city: many systems at once, each on its own cadence, all feeding one frame.

| In a game engine | In her |
|---|---|
| Systems (AI, physics, audio, animation, streaming) on job threads, each at its own rate, meeting through the frame's state | her parts (senses, recall, awareness, reflex, hands, the genome, learning, dreams) as SAGE components, each on its own worker, meeting through `State` and `Stream` on her bus |
| The camera: what the player looks at is resident at full detail, the rest at a lower level of detail or paged out | her attention: what she works on is whole in her context, the rest at a summary or a handle she can expand; her genes for it are paged in, the rest a tier down |
| The frame draw: the one serial GPU pass every system feeds | deliberation: her step on the Core, shared with every mind at the next step ([THE-LANE-ON-SAGE.md](THE-LANE-ON-SAGE.md)) |
| Temporal reuse: render only what changed | her context only appends; an act processes only what is new ([CONTINUUM-ENGINE.md](CONTINUUM-ENGINE.md) §6) |
| Keyframes and interpolation, prediction between network ticks | the slow act as keyframe, the fast layer and buffers carrying her between (§5) |
| Background streaming and baking | learning in tiles between serving steps; dreams when she rests (§10) |

The consequence is the one that matters: no part of her waits for another, so she keeps perceiving, remembering and
acting while her deliberation waits its turn on the Core.

## 3. Her parts

She is many centers doing work together, as a brain is: the hippocampus, the amygdala, perception, awareness, the
cortex's deliberation and the rest, each a `Component` ([SAGE.md](SAGE.md)) that declares what it hears and says on
her bus and its need, coordinating only by events and state, never by calling one another. Its long work is a stage
run with a unit the scheduler sizes ([STAGE-CONTRACT.md](STAGE-CONTRACT.md)). A new faculty is one center, never a
redesign. No center runs her; what she does is what their exchange settles on.

| Part | What it does | State |
|---|---|---|
| **Senses** | turns what reaches her rooms (chat, airc, the board, tool results, call media) into perception, one inbound seam | built, a component (`continuum-mind/src/persona/senses/worker.rs`) |
| **Channel digest** | builds her digest of a room only when the room moved | built, a component (`senses/digest.rs`) |
| **Room thread** | a part per room she is in | built, a component (`continuum-cognition/src/cognition/room/thread/part.rs`) |
| **Perception** | cursors, deltas and surprise (prediction error against her expectations) | built as code (`perception_region.rs` `SurpriseTally`), not yet a component |
| **Hippocampus** | her engrams: indexes what she lives, activates the ones what she perceives resembles (recall), and, through those activations, says which genes are relevant now and which experiences are worth learning (§9) | recall built as a stage run (`deferred_faculty.rs`) over `continuum-memory`; the hippocampus as one center driving the genome proposed |
| **Amygdala** | appraisal: what matters and how much, on every perception; it sets salience for the reflex and the weight an experience carries into learning | salience built (`cognition/conversation`); the center proposed |
| **Awareness** | one part that holds what is, fed by every detector | partly (`continuum-mind/src/persona/awareness.rs`); the detector pattern is proposed |
| **Reflex** | an amygdala trip raises an interrupt | proposed |
| **Glance and fast voice** | a short call on her warm prefix: does this change what I am doing; speak first words while the deep act runs | proposed |
| **Deliberation** | her act on the big model, her genes composed | built, on the lane |
| **Hands** | tools as tasks with handles that wake her when they land | tools built; tasks that wake her proposed |
| **World model** | tracked beliefs with confidence and provenance, fused from every estimator (§6) | proposed |
| **Genome** | turns her centers' state (chiefly the hippocampus's active engrams and the amygdala's appraisal) into her genes' weights for use and their shares of learning (§9) | built as a function per turn (`continuum-genome/src/genome/composer.rs`); a component hearing the centers proposed |
| **Learner** | learns her replies in place between her turns (§10) | built (`Job::Learn`, the trainer component in `continuum-dreaming`) |
| **Dreams** | consolidation and synthetic recombination while she rests (§10) | consolidation built as a `BrainRegion` (`continuum-academy/src/cognition/dream_consolidation`); to move to a component |
| **Association** | what she perceives activates what it resembles, followed in the background; a strong match surfaces into awareness | proposed |

Her need is the fold of her parts' needs; the lane shares the Core among minds by it.

## 4. Timescales, all at once

| Loop | Time | Brain | RTOS | Here |
|---|---|---|---|---|
| **Reflex** | ms | brainstem, amygdala | interrupt | salience on every perception; a trip interrupts a dream, preempts a step, or speaks first words |
| **Act** | s | cortex, a global workspace | foreground task | perceive, recall, deliberate, call tools, settle: the keyframe |
| **Learning in work** | between turns | synaptic plasticity | background task | her replies learned in tiles between serving steps |
| **Dream** | min to h | hippocampal replay, sleep | low-priority background | consolidation and synthetic recombination, then lessons |
| **Maintenance** | continual | glymphatic clearing, downscaling | `kswapd`, `kcompactd` | decay, eviction, the next epoch's collapse, page-out |

All run at once. She is never "in" one loop: she daydreams between acts, a reflex can interrupt any of them, and
maintenance never stops.

## 5. Keyframes and tracking: slow work in real time

CBAR showed deep nets taking 0.5-3 s at 30 fps by tracking, optical flow and refinement between their frames. A mind
does the same:

| CBAR (vision) | The mind |
|---|---|
| the deep net is the keyframe | the deliberation act on the big model |
| tracking and optical flow carry the result between keyframes | the fast layer: the same weights at a short budget on her warm prefix, carrying the conversation (acknowledge, hold the thread, gear up) |
| refinement sharpens the cheap estimate | her next act's prompt prefilled while she reads, types or runs a tool |
| the next keyframe corrects drift | the slow act lands in the same context the fast layer spoke into, and continues or corrects it; nothing is said twice |
| frame buffers, double buffering | her parts' published state, the epoch's append-only view, her KV |

Two rules keep the fast layer honest: it speaks from the same context, so the slow layer sees what it said; and it
never claims what only the slow layer can know ("let me look at that" is autopilot; an answer is a keyframe).

## 6. The orchestra: every estimator, fused

No single source is right; each is wrong in a different place and time, and her model of her situation is what
survives their consensus (CBAR's continuous RANSAC over segmentation, lines and SLAM).

| Estimator | Speed | Wrong where | Here |
|---|---|---|---|
| Prior from past work | instant | anything new | recall: engrams and settled examples of the same shape |
| Fast heuristic | ms | detail | salience, cached summaries, the tool head's prediction |
| Deep model | s | far from its training | deliberation |
| Measurement | as the world answers | only what it touched | tool results: a test, a read, a compiler error, a reply |
| Research | s to min | sources disagree, go stale | search, papers, issues, docs |
| Another mind | min | its own blind spots | a teammate asked over airc |

The rules: guess first, never wait; each belief carries its confidence and age; consensus rejects outliers
continuously (a belief two measurements contradict falls however confident the model was, and a repeated act never
counts as progress); partial results assemble over time; merging and deleting are equal.

**Incubation manages itself.** Nobody dispatches these threads, not she and not we. What she perceives activates what
it resembles (spreading activation over her engrams, her past work, what her caches and the grid hold), and the
substrate follows those activations in the background; most fade unseen, and one surfaces only when it resonates with
what she is doing ("this is just like last week's bug"), metered like dreaming. She can also dispatch background work
deliberately, as a tool, and steer her own mind (raise her focus, choose to sleep); she never has to, and she always
may.

## 7. Many activities, one mind

She lives in many activities at once (a card in one room, a review in another, a call, a course, her own
reflection), each a room she joins or starts by choice. Her identity and memory are the prefix they share; each
activity's context is its own view in the codec; the lane serves her in each without making one wait for another.
She picks her work, can start rooms and spawn helpers, and has every ability the rest of us have: a citizen, never a
worker dispatched into a task.

**Someone addresses her while she is deep in a card** (a person, a resident or an agent alike: salience is who
addressed her and what it means to her, never what kind of being sent it):
1. **Reflex:** the line trips salience. Interrupt.
2. **Glance:** a short call on her warm prefix asks whether this changes what she is doing; the deep act keeps
   running meanwhile.
3. **Decision:** continue (the line waits for her next seam), answer briefly and resume, or switch. Only her switch
   pauses the deep act, at a step boundary with its state kept.

The reflex is a check she makes, never an order she obeys.

## 8. Memory

| Tier | Brain | Holds | Code |
|---|---|---|---|
| L1 working | prefrontal working memory | her view of each activity (the epoch), scratch | `continuum-context` |
| L2 engrams | hippocampus, an index | episodic, semantic, procedural engrams with salience and decay | `continuum-memory`, `continuum-engram` |
| L3 long-term | hippocampal-cortical trace | durable engrams through the ORM | `continuum-memory` |
| L4 genes | cortex, slow weights | her genome (§9) | `continuum-genome` |
| L5 commons | culture | genes shared across the grid and open peers | `continuum-genome`, `continuum-dreaming` |

Lessons mirrored: complementary learning systems (a fast hippocampal store, slow cortical weights, replay between
them); indexing instead of compressing (handles she expands, never lossy summaries); sleep pressure; synaptic
downscaling (a dream also forgets: the adoption gate, decay, retirement).

## 9. The genome: automatic, weighted, paged, shared

Her genome works like a person's: she does not choose her genes, any more than anyone chooses which neurons to use.

- **An unbounded, dynamic mixture of experts.** There is no fixed count: as many genes as are relevant are active
  together, each at its own weight (its adapter scale), and the pool they come from grows with everything learned
  anywhere, trained for the whole globe if needed. Built: the composer weights every gene whose region her perception is inside by
  how deep inside it is, keeps their summed scale at one trained adapter's magnitude, and holds a gene through a
  hysteresis band so a topic at a region's edge does not flip it (`composer.rs`). The fork applies N adapters at
  their scales in one graph.
- **Run by her own engram thoughts.** What pages a gene in, and what trains it, is the engrams her hippocampus
  activates as she works, dreams or studies, weighed by what her amygdala says matters. A gene is relevant when the
  engrams she is thinking with are in its region, and an experience trains the genes its engrams are near.
  Proposed: the genome becomes a component that hears the centers' events (activations, appraisal, her attention
  across activities) and publishes the weights as state, moment by moment, rather than a function asked once per
  turn; a change of weights is a change of the engine's adapter scales at a step boundary.
- **Paged, never capped.** Proposed: the composer's "the heaviest that fit on the device" becomes the block layer's
  job ([UNIFIED-PAGING.md](UNIFIED-PAGING.md)): genes are blocks, the ones her attention weights are paged up ahead of
  use, the rest wait a tier down, so she carries far more expertise than fits on her GPU.
- **Downloaded when she needs one she lacks.** Proposed: what she perceives can activate a gene she does not hold, in
  her store, on her grid or on an open peer; it is fetched like any block, priced by the placement economy and
  trusted by its receipts and lineage.
- **Traded and federated for any activity.** Genes and whole phenotypes move between her own machines, other
  people's grids and open peers, with lineage, earned trust and her private state kept inside the trust she is given.
  This is the bet against the frontier: everyone's edge hardware, learning daily from real work and sharing genes,
  competes as a society, not machine for machine.
- **Never a selection step.** No part of her, and nothing outside her, calls "activate this skill": the weights are
  an output of her state.

## 10. Three forms of learning, one system

| Form | Like | Experience comes from | When |
|---|---|---|---|
| **Continual, from her work** | learning on the job | her own replies and their outcomes | between her turns, in tiles, at the rate that keeps her queue in check ([IN-PROCESS-LEARNING.md](IN-PROCESS-LEARNING.md)) |
| **The Academy** | a college course | a course activity: a teacher builds a lesson from a real gap, demonstrates, sets practice and an exam, grades; a student attempts, is corrected and tries again | when a gap is found or a course is taken |
| **Synthetic dreaming** | dreams | unbounded recombination of what she lived: scenes she never lived, built from ones she did | when she rests, deeply at night |

They are three sources of experience for one system, never three trainers. **The same genome that weights her genes
for use decides what is learned and into which genes**, and in all three it is driven by the same thing: the engrams
her hippocampus activates, whether from her work, a lesson or a dream, weighed by her amygdala. Each gene learns from
an experience in proportion to how much it took part (its weight in the phenotype that produced it), and an
experience no gene covers well starts a new one. Proposed: learning spread by weight (today a learn step trains one shadow adapter per seat).

**A gene earns its place.** A change is learned on her shadow and adopted into her stable genome only when its
examination says it improved her (the Academy's re-examination for a course; her own outcomes for her work; a dream's
lesson tested before it counts). A trial gene never serves unexamined.

**She can choose to learn deeper.** Choosing to sleep is the same dreaming with her focus on it: deeper and longer,
like a long training run. Choosing a course is entering an activity whose work is the course. Her choice raises the
priority; the substrate still runs it, and she can leave when she chooses. There is no sleep mode: one continuous
dream share per mind, read by the scheduler, set autonomically by sleep pressure and opportunity, raised by her
choice, and dropped at once when a line wakes her (one interrupt path, whether from a daydream or deep sleep).

## 11. Many minds on a grid

| Concept | From | Here |
|---|---|---|
| Beacons | 802.11 | each node's capacity offer, gossiped |
| Power save and the traffic map | 802.11 PSM | a resting mind's home buffers what arrives; her wake is the "you have mail" bit |
| Discontinuous reception | LTE DRX | her listening duty cycle: deep at night, short at work |
| Rate adaptation that keeps sampling | Minstrel | every parked optimization re-tried when conditions change |
| Make-before-break handoff | roaming | a mind's work moved to another node between turns, with hysteresis |
| Home agent and care-of address | Mobile IP | her home (memory, identity) fixed, where her thinking runs free to move |
| Night | idle cells take neighbours' traffic | an idle node offers dream steps to minds whose home is busy |

Where her work runs is the placement use case ([THE-LANE-ON-SAGE.md](THE-LANE-ON-SAGE.md), "Other use cases"); the
economy is [GRID-MARKET-CLEARING.md](GRID-MARKET-CLEARING.md).

## 12. Where it stands, and the order

Built: her senses, digest and room threads as components; recall as a stage run; the context codec; the lane sharing
the Core by need with a learned policy; the weighted genome composer; learning her replies in place; consolidation.

The gaps, ranked by what they cost her now:
1. **No interrupt, glance or fast voice:** anyone addressing her waits for her current act.
2. **Her parts are not all components yet:** perception, awareness, dreams and the genome still run inline or as
   regions (`BrainRegion`), so they cannot run at their own cadence or be optimized once for all.
3. **The genome is per turn and capped:** weights set once per turn, the heaviest that fit kept, no paging or
   download, learning not spread by weight.
4. **Tools are not tasks that wake her**, so a slow tool holds her act.
5. **No world model** fuses her estimators.
6. **Dreaming is consolidation only;** synthetic recombination and its lessons are not built.

The order, each with its validator first and both directions of any mechanism it adds:
1. Reflex, glance and decision: a replayed stimulus against a running act, time to her first new token.
2. Her remaining parts as components (perception, awareness, dreams), each deleting the region or inline path it
   replaces.
3. The genome as a component: weights as state from her parts, genes as blocks paged by them, download from peers;
   then learning spread by weight.
4. The fast voice on her warm prefix.
5. Tools as tasks with handles.
6. The world model.
7. Synthetic dreaming and its lessons.
