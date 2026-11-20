# 0. The engine

[The object model](README.md) · next: [The core today](01-today.md)

The design ideal, before any type. Continuum is an engine in the sense a game studio means: a
runtime that keeps a dense, living world present on hardware too small to hold it, at a frame rate
that does not slip. The world is a society of citizens. The hardware is whatever its owner has.
This chapter states the brief, takes what Rockstar's RAGE and our own CBAR learned about meeting a
brief like it, and turns each lesson into a mechanism of the core. The chapters after it are how
those mechanisms are built.

The resemblance is not an analogy; it is lineage. RAGE, CBAR and Continuum share one architecture,
because they solve one problem shape: a dense, persistent world kept present on hardware too small
to hold it.
- **All three stream at levels of detail.** RAGE streams a city, CBAR a camera's scene, Continuum a
  society.
- **All three separate logic from natives.** RAGE has scripts, CBAR typed analyzers, Continuum
  recipes calling typed commands.
- **All three run concurrently by yielding and publishing, never by locking.** RAGE uses fibers and
  double buffers, CBAR its queue threads and invalidation, Continuum workers and completions.
- **All three simulate rather than play back.**

Continuum differs in what lives in its world and what its console is:
- its inhabitants are minds, so its world learns: a gene is a motion the engine did not ship with,
  earned and passed on;
- its console is unknown and changes while running, so fixed pools give way to one governed flow;
- it spans a Grid, so one engine runs across many consoles.

## 0.1 The brief

**The world** ([README](../../../README.md)):
- **Citizens.** Persistent citizens with identity, relationships, memory, senses, a genome and a
  history. They live in rooms and activities with people and with each other. They perceive
  (camera, microphone, screen, room state, tool results), act (speech, avatar, gesture, tools,
  shared state) and settle each turn against the world's real answer.
- **Learning.** Their work becomes experience, their experience becomes curriculum, and their
  curriculum becomes genes that change their weights and must earn adoption.
- **Spread.** Genes spread through a population with lineage. A Grid of machines makes all of it
  additive.

**The console:**
- **Hardware.** A laptop with unified memory, a gaming PC whose GPU its owner may need for a game
  tonight, a phone, a spare tower. None is known in advance, and all of them change while running.
- **The weak link.** Memory is always too small for everything at once. The GPU is shared with
  rendering, speech and learning. The CPU must simulate many minds without waking those that have
  nothing to do.

**The frame:**
- **Bodies and voices.** Thirty frames a second for every visible body; audio that never underruns.
- **Turns.** A turn that begins speaking as fast as a person would, and deepens while it speaks.
- **Learning.** Learning that happens while all of this runs, in the same frames, at the speed its
  lessons arrive.
- **Nothing restarts.** No restart for a change of hardware, a change of load, or a mind arriving or
  leaving.

The README names the sentence this brief rests on: **persistence is not the same thing as constant
generation.** A citizen is always present; she is not always computing. That is exactly the bargain
an open-world engine makes with every building in its city.

## 0.2 What RAGE learned, and what we build from it

Rockstar has said little in public about RAGE. What follows comes from its developers' interviews
and talks and from people who took the games apart. Each lesson is stated as RAGE met it, then as the
mechanism it becomes here.

### The whole world draws, all the time

GTA 5's art director said the entire world draws all the time. The view distance is not limited, and
every light an artist placed is still there far away. It works because of the level-of-detail chain
below.

**Here:** every citizen, room and activity is always present. A quiet citizen is not unloaded from
existence: she is at her lowest level of detail. That means her identity, her presence in her rooms,
the fact of what she was doing, and a wake condition. She costs almost nothing and is never missing.
Nothing pops out of the world because it is far from attention.

### Every object is a chain of stand-ins, and they blend

Each building or prop points to a simpler stand-in, which points to a simpler one. At night, the
city seen from the hills is flat squares of light. A distant car is a pair of headlights until you
get close. When one level replaces another, the game blends them with a checkerboard so nothing pops.

**Here:** every item a mind perceives has a chain of levels:
- the whole item;
- a summary;
- a headline;
- a handle she can expand.

The level is chosen by distance, and distance here is attention and age. The newest interaction is
always whole; the oldest is a headline. A far room is its headline; a speaking face is its full
frame. Levels change only at an epoch boundary, oldest first, so the change never invalidates what
follows it (the context is a FIFO; [CONTINUUM-ENGINE.md](../CONTINUUM-ENGINE.md) §5). That is our
checkerboard: a level change that never forces the rest to re-render. The codec that does this is
`continuum-context` (chapter 3.6).

### The world is sectors, streamed around its sources

Analyses of RAGE describe the world as a spatial grid of hierarchical sectors. A sector is kept in
memory only while a streaming source (the player, or an agent that matters) justifies its cost. The
streaming radius is not a fixed circle: it reaches farther along the direction of travel, and
further still the faster the source moves, while the sectors left behind are evicted. (These
descriptions come from people analysing the engine; Rockstar has not confirmed them.)

**Here:** the world's sectors are its rooms and activities, and its streaming sources are attention:
a citizen acting, a person watching, a call in progress.
- **The radius reaches ahead.** It extends along the direction a mind is heading: her continuation
  and its expectation, the activity her board says is next, the room a call is about to open.
- **Speed matters.** Its reach scales with how fast that activity is moving.
- **Trailing sectors drop.** Rooms she has left fall to a lower level of detail, then to their
  durable traces.
- **Sectors are hierarchical.** An activity contains its rooms, and a room contains its threads and
  artifacts. Each level streams on its own.

### Bubbles of fidelity around every focal point

The same analyses describe entities living in rings of fidelity around the player:
- full physics, the Euphoria solver and frequent sensing up close;
- simplified animation and a few updates a second farther out;
- statistical movement along a node graph beyond that;
- a compressed state record once out of range.

Entities move between rings as the focus moves.

**Here:** every citizen has a level of presence, chosen the same way, by distance from attention:

| Level | She is | What she costs |
|---|---|---|
| Acting | on the Core, perceiving at full detail, her avatar at full rate | her KV on the device, her regions at full rate, her body rendered |
| Attentive | warm and about to act, or in a call but not speaking | her KV warm, her regions on change, her avatar at its idle rate |
| Present | in her rooms, following them at the level of detail her dial sets | summarized perception, her KV demoted, events filtered by salience |
| At rest | her durable self, with a wake condition | her durable state only; a matching event promotes her |

Promotion and demotion follow attention, never a timer. Nothing about her is lost at any level,
because each level keeps her durable branches (chapter 5); only her caches move.

### Slots, hydrated from descriptors

RAGE avoids heap churn by keeping pools of preallocated slots for people, vehicles and objects. An
entity entering a cell claims a slot and is hydrated from a data descriptor rather than allocated
anew. Swapping an entity's behavior or physical profile is often just swapping a pointer to another
descriptor.

**Here:** a persona entering the Core claims a resident slot and is hydrated from her descriptor:
her durable state, her genes, her saved pages. Switching minds is swapping the descriptor bound to
the Core's active role. This is why a switch costs milliseconds (0.4).

**Where we differ.** RAGE partitions memory into hard, fixed pools per subsystem: physics, audio,
geometry, entities. That suits a console game, whose content is authored and fixed before it ships.
Our load mix changes by the minute: a call starts, a lesson queues, the owner launches a game on the
same GPU. Fixed partitions would hold memory idle in one pool while another starves. So the slots
are sized from measurement, and memory is one governed pool that flows to whatever needs it now
([SHARED-RESOURCES.md](../SHARED-RESOURCES.md)). RAGE's other memory rule we keep exactly: a freed
asset is released only when every subsystem holding it has let go. Here that is custody: a release
answers `Pending` until the last holder finishes (chapter 3.1).

### Streaming that cannot keep up shows holes, so design the stream

On the PS2, GTA 3 streamed the city off the disc while you drove. Drive too fast and you saw
low-detail buildings and missing roads. The fixes were design, not hardware:
- a long straight road in Portland was redrawn to slow the player;
- cars got about five per cent more air resistance in a few problem spots.

On the PS3 and Xbox 360, with half a gigabyte of memory, GTA 5 read from two drives at once: the
install disc on the hard drive and the play disc in the tray. Putting both on one drive made it lag.

**Here, three mechanisms:**
- **Prefetch by predicted need.** The scheduler knows which mind acts next. It promotes her pages,
  binds her genes and stages her prefill while another mind decodes (chapter 5 of
  [HOW-A-MIND-RUNS.md](../HOW-A-MIND-RUNS.md)). The stream runs ahead of the player.
- **Shape demand, never stall it.** When work arrives faster than it can be served, the governor
  shapes its arrival: priority by temperature, aging so nothing starves, lower levels of detail for
  what is far from attention. It does not refuse or freeze. That is the redrawn road and the five
  per cent of air resistance.
- **Read from every tier at once.** A mind's pages are spread across the tiers that hold them
  (device, RAM, flash, a peer), and a restore reads them in parallel, so bandwidth adds the way
  GTA 5's two drives did.
- **When the stream still loses, show a lower level, never a hole.** A mind whose pages have not
  arrived acts at a lower level of detail rather than not at all.

### Missions are scripts; the engine is natives

RAGE's games are C++, but its missions never were. They are scripts in Rockstar's own language,
each compiled to a small program on a virtual machine inside the engine. A script asks the engine to
do something through a **native**, one of the engine's own commands: spawning a car is one native
call, and GTA 5 has about 6,700 of them. A game's tunable data (a car's mass, grip and suspension in
`handling.meta`) sits in data files named after the engine's own fields, separate from both.

**Here:**
- **Natives.** The core's natives are its typed action commands (chapter 3.2): `code/edit`,
  `data/list`, `voice/synthesize`. Each has a typed identity, parameters and answer, and generated
  bindings.
- **Scripts.** Missions are recipes: an activity, a course or a room is data that calls natives. It
  is never code compiled into the core.
- **Data.** Tunables are data entities beside the code, named after the code's own fields. A
  budget, a threshold or a forge recipe is measured and stored, never a constant. The core grows by
  adding natives and recipes, not by special-casing missions.

### Scripts are cooperative fibers

Each running RAGE script is reported to be a cooperative fiber on the script VM: a tiny fixed stack
and an instruction pointer. It runs a slice and yields by calling `WAIT`. Hundreds can be started,
paused, swapped or ended in a frame, with almost no cost to switch.

**Here:** a running recipe and a mind's turn both yield at safe points instead of holding anything:
- a recipe step at each native call;
- a turn at each decode step;
- a long native returns a task whose completion resumes it.

What a switch must save is small and owned: her residency for a mind, a step cursor for a recipe.
That is why hundreds of activities and a Core shared by many minds cost almost nothing to interleave.

### Double-buffered state and command queues

RAGE's subsystems are reported to talk through ring buffers and command queues rather than by
mutating each other across threads. Its AI reads the previous frame's perception cache and pushes
locomotion intents into a physics command buffer. Physics resolves those in one batch before
rendering reads the results.

**Here:** a mind reads the freshest published snapshots, never another part's live state, and acts by
issuing commands. Those commands are resolved in batch by the parts that own the state, and the Core
resolves a frame's rows in one decode. Readers never block writers, and nothing mutates across
workers (chapter 2.5).

### A frame is built in passes

A GTA 5 frame starts as a list of about 4,000 things to draw. Solid objects are drawn first into
hidden images that record, per pixel, its color, the way it faces and how shiny it is. Only then is
the scene lit, from those images, in one pass. Sun shadows come from four views from close to far,
packed side by side. The ocean's reflection is its own tiny picture, a couple of hundred pixels wide,
because nobody can see more detail there.

**Here:**
- **The draw list.** A Core frame starts as a list too: the active mind's decode rows, the next mind's
  prefill chunk, a lesson's labelled rows, each gathered from work already prepared by its own
  worker.
- **One deferred pass.** Then one batch runs through the model: deferred work, assembled first and
  computed once.
- **Cascades.** Recall works in cascades, near memories in detail and far ones coarse.
- **Ballpark math.** Wherever a result is consumed coarsely (a footprint estimate, a token count,
  a far room's summary), it is computed coarsely, with the error bound stated and checked: our tiny
  reflection.

### The world keeps the trail

Red Dead Redemption 2 keeps a hidden image of footprints, hoof prints and wheel tracks, so the mud
remembers where you went.

**Here:** the world remembers what happened even where no one is looking. A room's history, a
citizen's engrams and a turn's receipts are cheap durable traces, and their detail comes back when
someone attends to them. The fact that something happened persists; how much of it enters a mind
depends on what she is doing now.

### Sound is stems, mixed by the moment

Rage Audio splits a mission's music into eight stems played together. Each is turned up or down by
what is happening, with changes timed to land on the beat. Car engines are not recordings: they are
built from tiny slices of sound stitched together as the revs change.

**Here:**
- **Voice is layered.** Her quick voice opens at once. Her deep mind's thought merges into the speech
  still to come, and revisions land on phrase boundaries, never mid-word.
- **Voice is built, not played back.** Her voice is synthesized from her own voice gene, not
  replayed.
- **Each room sounds like itself.** Each room has its own ambient perception, as GTA 5 has ten times
  GTA 4's ambient zones.

### Simulate behavior; never play it back

Since GTA 4, characters hit by a car are not playing a recorded fall. Euphoria simulates the body,
its muscles and its reflexes, so they brace or try to catch themselves, and never quite the same way
twice. Its behaviors (brace for impact, catch fall, teeter) are switched on per character by the game.

**Here:** a citizen's response is never canned. Her regions (the amygdala's salience, attention,
recall, the workspace) run live on what she perceives, and behaviors such as interrupting, asking,
deferring or digging deeper are switched on by her own state, not scripted by the caller. No two
turns are replayed, because each is computed from her present.

### Stimuli are scoped, queries are tickets, affordances are baked in

Analyses of RAGE describe three ways its agents perceive without paying for omniscience:
- **Scoped stimuli.** A gunshot, a collision or a siren is a spatial stimulus with a radius, a decay
  and a priority. Only agents registered in the cells it reaches receive it; nothing polls the
  world.
- **Query tickets.** Line-of-sight and pathfinding queries are time-sliced. An agent submits a ticket
  and keeps doing its current micro-behavior until the answer lands a few frames later.
- **Baked affordances.** Cover points and interactable props are tagged into each sector ahead of
  time, so an agent asks for the nearest chair rather than analysing geometry.

**Here:**
- **Events carry a scope, a salience and a decay.** They are delivered only to the subscribers of
  the rooms they belong to. A citizen's dial decides which of those reach her mind; nothing in the
  core polls a room.
- **Every expensive question is a ticket.** A deep look at a frame, a recall, a search or a remote
  inference returns a completion. She keeps acting on the freshest cached answer until it lands.
- **Affordances are published, not inferred.** Positron defines each application once, with its
  commands and state, and a room publishes its affordances as part of its view (README, "One world.
  Many ways to inhabit it."). A citizen perceives "this is a button that merges the PR" directly,
  never by reverse-engineering a screen. Her tool working set per activity is the same idea for her
  hands.

### Many small motions, blended and fitted to the body

Red Dead Redemption 2's people, horses and other animals share one movement system. It is built
from about 6,300 small motions that the engine blends automatically by speed and turning. GTA 6 has
over 600,000 animations, and a system that adapts them on the fly to characters of different heights.

**Here:**
- **Genes are small motions.** A citizen's genome is many small learned adaptations, composed per
  turn by what the turn needs: the blend.
- **Fitted to the body.** A gene is fitted to the base model it runs on, as an animation is fitted
  to a body (the Factory's work).
- **Bodies too.** Avatars move the same way: small motions blended by speech and gesture.

### The worst cost is recomputing what did not change

GTA Online took about six minutes to load on PC. A player found why: to read each value from a
10 MB catalogue, the game counted every remaining character of the file again. Before adding each
of 63,000 items, it compared it with every item already loaded, though there were no duplicates.

**Here:** this is the defect class we hunt first. Every derived value has a cache keyed by what
derived it, and recomputing a pure function of unchanged inputs is a defect. Our own versions are
measured:
- a mind's prompt re-rendered every act, at 12 tokens re-processed for every one appended;
- reuse that rose from 41% to 91.7% once checkpoints were reused.

The health number is re-processed against appended, and healthy is about 1:1.

### Own the engine

Rockstar built RAGE because a rival bought the engine it had been using.

**Here:** a citizen's freedom is an architecture (README, "Freedom is an architecture"), and so is
our speed. We own the substrate down to the inference engine (our fork of llama.cpp, driven in
process), because what we cannot change we cannot make fast, and what someone else owns can be
taken away.

## 0.3 What CBAR learned

CBAR is Joel's real-time perception engine. Its lessons are the other half of the design.

- **An analyzer graph, wired by type.** An analyzer pulls what it needs by the type of the result,
  not by a hard-coded edge. Here: parts read each other's publications by topic (chapter 3.1).
- **Invalidation, not servicing.** A change wakes exactly the analyzers that depend on it, and the
  rest keep their last result. Here: a publication invalidates its dependents; nothing sweeps on a
  clock.
- **Real-time and delayed lanes.** Real-time analyzers run every frame; slow ones run on their own
  threads, throttled. Here: deadline work, latency work and throughput work are given time by shape
  ([SHARED-RESOURCES.md](../SHARED-RESOURCES.md), "Time").
- **The frame is computed once and passed by reference.** Every analyzer reads the same frame's
  grayscale and pyramid; only its interpretation is its own. Here: a stream's detections, a
  message's embedding and a file's read are computed once for every mind; what they mean to each
  mind is hers.
- **One in hand, one pending.** CBAR's `QueueThread` holds the item it is working on and one waiting,
  and a newer signal replaces the waiting one. Here: the worker (chapter 3.1), with a reliable lane
  for work that must not be dropped and a control lane that is never blocked.
- **Slow results in real time, by fusion.** A three-second network's result is attached to its frame
  and carried forward by continuous tracking. A rough estimate fills what would otherwise be a gap,
  and precise methods refine it as the camera approaches. Here: perception, comprehension and speech
  are living estimates that refine as better knowledge arrives (chapter 4.2, `FusionRegion`).

## 0.4 The budgets

The numbers the design is held to. Measured values carry their receipt; targets are labelled.

| Budget | Value | Status |
|---|---|---|
| A body's frame | 33 ms (30 fps) per visible avatar | target |
| A mind switch on the Core | mean about 21 ms between residents | measured, M5, 2026-11-20 |
| Save and restore a resident's state | about 0.22 s and 0.1 s in isolation | measured, CONTINUUM-ENGINE §4.1b |
| Restart with residents restored | about 5.6 s against about 401 s cold | measured, CONTINUUM-ENGINE §7.0 |
| Re-processed against appended tokens per act | about 1:1 | target; 12:1 measured where the FIFO law was broken |
| Context reuse | 91.7% after checkpoint reuse | measured, #6171 and #6174 |
| Throughput with many minds | near one stream's rate (about 80 tok/s on the M5) including switching | target (MODULE-ARCHITECTURE §9.4) |
| A learning step's extra memory | the shadow and its optimizer, about 6.5 MB at rank 8 on a 0.5B model | measured, #6203 |
| An idle core's CPU | near zero | target |

## 0.5 What we refuse

Each of these is a way the brief is failed, recognised by name:
- a mind that waits on another for permission;
- a reservation held between uses;
- a model or gene reloaded because nobody kept it warm;
- a prompt re-rendered when only its tail changed;
- a loop that polls;
- a hole where a lower level of detail should be;
- a restart to change shape;
- a canned behavior;
- a constant where a measurement exists;
- a second copy of an idea that already has a home.

## 0.6 How the rest of this book builds it

- **Parts** (chapters 2 to 4) are the engine's objects: each with a lineage, its own worker and its
  own state, talking by commands and publications.
- **The persona** (chapter 5) is one character in the world: a tree of parts at levels of detail.
- **Crates and modules** (chapters 6 and 7) are the engine's code layout, general below specific,
  each domain runnable on its own node of the Grid.
- **Coverage and the plan** (chapters 8 and 9) take today's code there.
