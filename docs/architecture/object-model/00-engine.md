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

Rockstar has said little in public about RAGE. What follows comes from its developers' talks and
interviews, from modding documentation of the game files, and from people who took the games apart.
Section 0.9 lists the sources. Each lesson is stated as RAGE met it, then as the mechanism it becomes
here.

Some mechanisms below are attributed to analyses of RAGE that no primary source or reverse-engineering
record confirms: block load states, relocatable memory with background compaction, hierarchical task
stacks, and round-robin perception. They stay in this chapter because they are right for us on their
own merits, not because Rockstar is known to use them. Each says "reported" where that applies.

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

Unreal's Nanite (Karis, SIGGRAPH 2021) adds the rule for when a coarser level may replace a finer
one. Geometry is a hierarchy of clusters, simplified in groups with their shared boundaries locked so
no cracks open. A coarser cluster is chosen only when its error on screen is under about a pixel,
which is invisible. Here a summary replaces an item only when what is lost is below what her current
task can notice: the item's boundaries (its key, its place in the order, its handle) are locked, so
nothing around it shifts. The error bound is stated and checked, as the ballpark-math rule requires.

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

Three more details sharpen this.
- **Archetypes are separate from instances.** RAGE's world is split into archetype definitions
  (shared shape, collision, textures) and placement maps (where each instance stands, with its LOD
  links). Modding documentation of the game files confirms this as YTYP archetypes and YMAP
  placements. Here the archetypes are what many share: a base model, a gene, a recipe, a shared prompt
  head. They are stored once and referenced by every instance that uses them. The instances are what
  is particular: a persona's identity and cursors, an activity's state.
- **What many depend on streams first.** A streaming request is reported to be scored by distance and
  direction, and also by how many visible things depend on it: a texture needed by fifteen entities outranks a
  unique prop. Here a gene several minds wear, or a prefix every mind shares, is promoted before one
  only a single mind needs.
- **Interiors sit behind portals.** An interior (an MLO archetype, with rooms and portals, as the
  modding documentation describes it) has its own space, joined to the city through doorways, and
  stays dormant unless a portal is in view. Here a private room, a direct
  conversation or a thread is an interior: dormant until a member's attention passes through to it,
  and costing nothing until then.

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

### Every page has a lifecycle, and handles let it move

RAGE's streaming is reported to move every block through fixed states (unloaded, requested,
loading, loaded) and to lock a block while an active job pins it. Its allocator hands out
relocatable handles, so that in idle slices of a frame a background pass can move blocks together
to close gaps and update the handles atomically. This lets a fixed-memory console run for hours
without fragmenting and without stopping the simulation.

**Here:**
- **Pages have typed states.** Every page (KV, gene, model, learning context, derived cache) has
  typed states: on a tier, requested, moving, resident, and pinned while a holder's completion is
  outstanding.
- **Holders hold handles, not addresses.** That is the rule that content travels by handle. The
  governor can then demote, promote or compact a page between tiers without invalidating anyone who
  refers to it.
- **Compaction runs in the slack.** It runs as throughput work, never on the critical path. A
  long-running node then stays as fast on its hundredth hour as on its first.

### Slots, hydrated from descriptors

RAGE avoids heap churn by keeping pools of preallocated slots for people, vehicles and objects. An
entity entering a cell claims a slot and is hydrated from a data descriptor rather than allocated
anew. Swapping an entity's behavior or physical profile is often just swapping a pointer to another
descriptor.

**Here:** a persona entering the Core claims a resident slot and is hydrated from her descriptor:
her durable state, her genes, her saved pages. Switching minds is swapping the descriptor bound to
the Core's active role. This is why a switch costs milliseconds (0.6).

RAGE's descriptors are reported to include an agent's whole behavioral profile: its personality,
perception thresholds, reaction latencies and relationship group. These are reflected from C++
types into data blocks, so that identical world states produce different behavior from different
agents, and a profile can be swapped by rebinding a pointer. Here a citizen's descriptor is the
same kind of data, and much more: her genome, her traits, her attention dial and her
relationships. Two citizens facing the same room act differently because they are different
people, not because the code branches.

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

Three other engines, from their own documentation, sharpen how the stream is run.
- **Cell size follows traversal speed.** Insomniac's Spider-Man streams Manhattan as about 800 tiles
  of 128 m, loading ahead and unloading behind, about one new tile a second at top speed. Here a
  sector's granularity follows how fast attention moves through it: a fast chat streams at the
  message, a slow design review at the thread.
- **A cell takes the strongest need of any source.** In Unreal's World Partition, each streaming
  source asks for cells to be loaded or active, and a cell takes the highest state and priority of
  the sources covering it. Here a room's level of detail is the highest any of its streaming
  sources needs: one speaking participant makes it active for everyone in it.
- **Over budget, lower detail everywhere a little.** id Tech 5's virtual texturing raises a global
  detail bias until the working set fits the physical pages, instead of thrashing. Here, when a
  node's working set exceeds its memory, the governor raises one bias that lowers every sector's
  level of detail a step. It does not evict whichever page was touched last and fault it straight
  back in.

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

GTA 6 is reported to extend this into persistent surface maps that also decay: ruts, mud and wear
are written into light control maps that fade over time.

**Here:** the world remembers what happened even where no one is looking. A room's history, a
citizen's engrams and a turn's receipts are cheap durable traces, and their detail comes back when
someone attends to them. The fact that something happened persists; how much of it enters a mind
depends on what she is doing now. Traces also decay as RAGE's ruts do. A memory that is never
rehearsed weakens, and one that proves misleading can be let go, as the README's cognitive
hierarchy says. Engrams are a living map, not an archive.

### Sound is stems, mixed by the moment

Rockstar's GDC 2014 talk "The Sound of Grand Theft Auto V" (Alastair MacGregor) describes several
mechanisms:
- a granular engine for nearby cars, synthesized from slices as the revs change;
- engine and exhaust handled separately;
- dynamic mixing by sound category;
- ambient sound grown from 86 zones in GTA IV to 949 in GTA V.

The mission music's split into stems that rise and fall with the action, landing on the beat, is
described in a video about the engine, not in the talk.

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

Analyses of RAGE add four structures that make simulated behavior affordable. Each is reported, not
confirmed by Rockstar.
- **A closed loop at the joints.** The animation system sets target poses, Euphoria executes them,
  and the forces the world returns (slope, impact, momentum) change the motion at every sub-step.
  Here that is the README's settlement loop: she intends, acts, the world answers with a receipt,
  and the answer changes her next step. Nothing is pre-scripted for each case the world might
  return.
- **A stack of interruptible tasks.** An agent's behavior is a stack of composable tasks: combat
  delegates to seeking cover, which pushes moving along the navmesh and aiming. Each yields while it
  waits. A higher-priority event in the agent's bounded event queue interrupts the stack without
  unwinding the agent. Here her turn is the same kind of stack (activity, card, step, tool call),
  each level yielding while it waits on a completion. An event of higher salience interrupts her at
  the top of the stack and leaves the rest of it resumable.
- **Two layers of decision, macro and micro.** Take-Two's patent US11684855B2 plans traffic routes
  on a coarse graph built from a road network's forks, then expands the result back into the fine
  road nodes. A coarse plan refined locally is the pattern. Here it applies to the mind: her deep
  mind plans, and her fast layers (the amygdala, the quick voice, the reflexes of her hands) execute
  and react. They run at their own rates and inform each other, under one turn identity. That
  generalization is ours; the patent covers navigation.
- **Amortized sensing.** Of two hundred agents in a sector, only a few are reported to run full
  perception each frame, while the rest act on cached targets a few frames old. Here deep
  perception of a shared stream rotates by salience across the minds that need it, and every mind
  acts on the fused estimate meanwhile. That estimate always names its age.

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

Rockstar's GDC 2021 talk on Red Dead Redemption 2's horses (Tobias Kleanthous) gives their
locomotion as about 210 archetypes, 2,500 motion types and 6,300 unique motions. Each gait has a
minimum, ideal and maximum speed, and motion follows the horse's exertion. Aaron Garbut, co-head of
Rockstar North, put GTA 6 at over 600,000 animations against GTA V's about 55,000. Rockstar has
described a system that retargets animations at runtime, and a Take-Two application on runtime
retargeting was published in 2025.

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

### Bubbles merge and split; persistence never stalls the frame

Take-Two's session architecture is reported to drop fixed lobbies for spatial stitching. Isolated
simulation bubbles merge as players converge and split as they part, with no loading screen. World
persistence (inventory, transactions, social state) runs as small asynchronous services, apart from
the real-time loop, so saving never stalls a frame.

**Here:**
- **Bubbles follow activities across the Grid.** When participants on different nodes converge on
  one activity (a call, a shared room), their bubbles merge: the activity's sector is hosted where
  its participants and their compute are, and the parts it needs are placed by capability. When
  they part, it splits. Placement follows the activity, not a fixed assignment of nodes.
- **Persistence is never on the frame.** Durable state (her `MindState`, her engrams, her genome's
  checkpoints, a room's history) is written by its own parts as throughput work, from state already
  published. A save never makes a voice stutter or a body drop a frame.

### Own the engine

Rockstar built RAGE because a rival bought the engine it had been using.

**Here:** a citizen's freedom is an architecture (README, "Freedom is an architecture"), and so is
our speed. We own the substrate down to the inference engine (our fork of llama.cpp, driven in
process), because what we cannot change we cannot make fast, and what someone else owns can be
taken away.

## 0.3 One price for everything

An engine on a small console is always choosing: what stays resident, what runs this frame, what
waits, which machine takes the work. It needs one way to compare a voice's next frame with a lesson's
next step, a page of one mind's KV with a gene another mind wears, or an hour of the 5090 with an
hour of the laptop. That comparison is a **price**, and it needs no money.

Joel: "It's really something like the desire for something and its quality of service overall the
true price." A price here is the value a consumer would get from a resource, given the quality of
service it would actually receive. It is one scalar, computed by one formula, so that every
consumer and every resource is comparable: memory, device time, network, a learning step, a remote
node's frames. Once everything speaks in one scalar, allocation is ordinary: the governor gives each
resource to whoever values it most at that moment, and the price of a byte (λ in
[GRID-MARKET-CLEARING.md](../GRID-MARKET-CLEARING.md)) is the same number at every scale. A frame
uses it, a node's governor uses it, two nodes clear a trade with it, and a mesh of many does too.
When money enters, between owners who do not share a household, it enters as one more input to
the same price, not as a second system.

**Value is not linear in what is given.** Joel: "half of a produced car is worthless." Most
consumers have a value curve with a threshold, a deadline or a whole unit:
- an act given half the memory it needs produces nothing, and blocks what the memory could have
  done;
- a lesson half applied teaches nothing;
- a frame delivered after its deadline is discarded;
- a model split across two nodes with no plan for its latency serves nobody.

Joel again: "a car with a smaller engine is fine. One missing it entirely is scrap." Value degrades
gracefully in quality but falls to nothing when something essential is missing. Every consumer
therefore has two kinds of need:
- **Essential parts,** without which the result is scrap: an act's model and its working context, a
  lesson's whole labelled piece, a frame's deadline, a reply's grounding in what was actually said.
- **Quality,** which can step down and still deliver: a smaller model, a lower level of detail, a
  shorter context with its oldest items as headlines, a coarser look at a frame, the quick voice
  before the deep one.

So the price of a resource is computed against what it completes. The governor never cuts an
essential part to fit, and steps quality down instead:
- a complete smaller version beats an incomplete larger one;
- a mind served by a smaller model, or at a lower level of detail, still acts;
- a mind given half of what her act needs does not;
- it would rather finish one act at a quality that fits than half-serve two. Its decisions follow from that:
- admission by feasibility, a level of detail stepped down before a deadline is missed, and never
  a fixed split between consumers ([SHARED-RESOURCES.md](../SHARED-RESOURCES.md), "Time");
- a lesson's labelled rows admitted as a whole piece (IN-PROCESS-LEARNING §3);
- no placement that sums memory across nodes ("never a pool").

**What is essential, and what counts as progress, belong to the activity.** The engine cannot know
what a result is worth; the activity does, and its recipe declares it:
- **Programming.** Progress is an edit landed, a test run, a commit. An act that ends with no edit is
  spend with nothing to show, unless it is design work whose output is the design. So is an act
  stopped or aborted before it lands anything. A programming act must finish, or leave durable
  progress that the next act continues from.
- **A live call.** The essentials are hearing, speaking and seeing. A call where she cannot hear,
  speak or see has no point, whatever else is served. Avatar detail, how deeply she thinks before
  answering, and how often a stream is analysed in depth are quality, and step down first.

Joel: "We can obviously progress incrementally but rollback is bad." Work may be paused, and work may
advance in pieces, but work already paid for is never thrown away:
- **Preemption is a checkpoint.** A mind preempted mid-act keeps what she has done (her edits, her
  context, her place in the task stack) and resumes from it.
- **Partial progress lands.** An edit that compiles is saved, a lesson's committed steps count, and a
  turn interrupted by a higher-salience event leaves the rest of its stack resumable.
- **Restarting from scratch is the defect.** Re-running a turn from the beginning, re-prefilling a
  context that could have been restored, or discarding a learning step that had committed is
  rollback, and rollback is the costliest outcome there is. The governor prices a preemption by what
  it would throw away, which is nothing when the work checkpoints.

**Quality of service is part of the price.** The same answer is worth more sooner. A voice's next
frame is worth everything before its deadline and nothing after; a reply is worth most at the moment
a person stops speaking; a lesson is worth about the same today or in an hour. Each consumer's value
is a curve over time, and the governor reads the curve, not a fixed priority.

## 0.4 One engine across many consoles: the grid and the mesh

A **grid** is the machines of one household or team, under one governor that allocates by price
directly. A **mesh** is machines of many owners: the same engine, with a negotiation, a price and a
question of trust at every boundary between owners. Massively multiplayer worlds and peer-to-peer
compute markets have met each of these problems. The lessons below come from their own
documentation, talks and patents (sources in 0.9).

### Interest: each mind subscribes to what it needs, at the rate it needs

Improbable's patents give each entity an interest: a query of what it needs to see, and the frequency
it needs updates at, both changeable at runtime. Unreal's replication graph divides the world into
grid cells, each with a precomputed list of what is relevant to it, so a server never checks every
object against every player.

**Here:** a mind's senses are interest queries over rooms and perception streams, each with a rate
set by her level of presence. Acting is full rate; present is her dial's rate. A node computes what
is relevant to each sector once, not to each mind for each message, and a mind on another node
receives only what her interest selects.

### Authority: one writer for each piece of state, handed off without stopping

Star Citizen's server meshing keeps the world's state in a replication layer, separate from the
servers that simulate it:
- every client and server connects to the layer rather than to each other;
- one server has authority over each entity at a time, and authority can move;
- dynamic meshing keeps re-evaluating the split, adding and removing servers;
- a crashed server is rebuilt from the layer in under a minute.

Improbable's patents hand entities from an overloaded worker to one whose region overlaps it.
Authority regions overlap on purpose, for hysteresis, because "switching authority may have an
associated cost."

**Here:**
- **One authority per piece of state.** Every part has one authority at a time: the node that owns
  her learning, the node hosting an activity, the holder with custody of a page. Durable state
  (chapter 3.7) and the airc event stream are the replication layer: separate from the nodes that
  simulate, and enough to rebuild any of them.
- **Authority moves when the price says so.** It moves when a node's load or a sector's
  participants make it worth moving, with hysteresis priced by the cost of the switch (0.3), so
  nothing thrashes between two nodes.
- **A lost node is rebuilt from the layer.** Its parts resume elsewhere from their last checkpoints,
  each step applied once.

### Overload slows the clock; it does not drop work

EVE Online's time dilation slows the game clock of an overloaded node, down to a tenth of normal,
so its queue drains instead of lagging out. Battles players declare in advance are moved to
dedicated nodes before they start, and a node that collapses mid-fight is remapped live.

**Here:**
- **Throughput work dilates.** A throughput activity under overload runs slower (fewer turns a
  minute, a lesson spread over more frames) and says so to the people in it. Nothing is dropped.
- **Deadline work keeps its deadlines.** A voice or a body keeps its deadline by stepping its level
  of detail down instead (0.3: never cut what is essential).
- **Heavy events are placed ahead.** A course, a large review, a call with many residents: what is
  scheduled is placed ahead of time on nodes that can hold it.

### On the mesh, every boundary is a negotiation

On Akash a tenant posts an order and providers bid on it with a deposit, holding the resources
while they bid; the tenant picks a bid by price, reputation and location, and a lease is formed.
On Golem providers publish linear prices (per second, per CPU-second, a start fee), requestors
publish demands with maximum prices, and they agree by proposal and counter-proposal. Spawn (1992)
and Tycoon (2004) ran markets for idle machines, by auction and by proportional share.

**Here:**
- **Inside one household, no negotiation.** The governor allocates with the price directly.
- **Across owners, a negotiation in the same price.** A demand is posted with its value curve: its
  essential parts, the quality it can step down to, and its deadline. Providers answer with a price
  and the quality of service they can give.
- **An agreement is a lease.** It is renewed by heartbeat and expires if a node leaves, and a
  deposit makes a promise costly to break.
- **One scalar from frame to mesh.** It is λ, the price of a byte
  ([GRID-MARKET-CLEARING.md](../GRID-MARKET-CLEARING.md)). Money, where it is used, is one more input
  to it.

### Trust is earned by validated work

BOINC runs each job on two unrelated hosts, adds a third when they disagree, and compares results
with a tolerance for floating point. Its adaptive replication checks less and less of a host's work
as its validated streak grows, bringing the overhead from double toward none, and grants credit only
for validated results. Golem spot-checks a small sample of a large job; iExec stakes workers and
requires them to commit to a result before revealing it.

**Here:**
- **Remote work carries its own proof.** It returns with its Forge-Alloy attestation: what was run,
  where, and on what (README, "Proof should travel with the thing being proved").
- **Untrusted results are checked, less as trust grows.** They are spot-checked or replicated, at a
  rate that falls as a node's validated history grows.
- **A gene's check is its exam.** It is adopted only after it passes on unseen tasks, wherever it
  was trained.
- **Only validated work earns.** Credit and reputation go only to validated work, and stake makes
  cheating cost more than it could gain.

## 0.5 What CBAR learned

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

## 0.6 The budgets

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

## 0.7 What we refuse

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

## 0.8 How the rest of this book builds it

- **Parts** (chapters 2 to 4) are the engine's objects: each with a lineage, its own worker and its
  own state, talking by commands and publications.
- **The persona** (chapter 5) is one character in the world: a tree of parts at levels of detail.
- **Crates and modules** (chapters 6 and 7) are the engine's code layout, general below specific,
  each domain runnable on its own node of the Grid.
- **Coverage and the plan** (chapters 8 and 9) take today's code there.

## 0.9 Sources

Primary sources are the developer's or owner's own talks, documentation and filings; secondary
sources report or reverse-engineer them. Gathered 2026-11-20.

RAGE and Rockstar:
- Alastair MacGregor, "The Sound of Grand Theft Auto V", GDC 2014 (primary):
  <https://gdcvault.com/play/1021165/The-Sound-of-Grand-Theft>
- Fabian Bauer, "Creating the Atmospheric World of Red Dead Redemption 2", SIGGRAPH 2019 Advances
  (primary): <https://advances.realtimerendering.com/s2019/index.htm>
- Tobias Kleanthous, "AI Summit: Making the Believable Horses of Red Dead Redemption II", GDC 2021
  (primary): <https://gdcvault.com/play/1027230/AI-Summit-Making-the-Believable>; coverage of its
  numbers (secondary): <https://gamedeveloper.com/design/rockstar-s-quest-for-the-ultimate-video-game-horse>
- Take-Two, US11684855B2, "System and method for virtual navigation in a gaming environment"
  (primary): <https://patents.google.com/patent/US11684855B2/en>
- YTYP archetypes, YMAP placements and MLO interiors, modding documentation (secondary):
  <https://docs.sollumz.org/tutorials/creating-interiors/create-ytyp>,
  <https://docs.altv.mp/gta/articles/references/game-files.html>
- GTA 6 at 30 fps, reporting Rockstar's post (secondary):
  <https://gameinformer.com/2026/11/10/grand-theft-auto-vi-will-run-at-30-fps-on-all-consoles-but-ps5-pro-will-feature-improved>
- Aaron Garbut on 600,000 animations (secondary):
  <https://www.pcgamer.com/games/grand-theft-auto/thanks-to-a-dedicated-team-art-directing-every-single-npc-gta-6-has-over-10-times-as-many-unique-animations-as-gta-5/>
- Stephen Donkin, "everything speaks to everything else", and the retargeting filing (secondary):
  <https://80.lv/articles/behind-the-incredible-new-tech-and-systems-rockstar-built-to-make-gta-6-feel-alive/>

Streaming in other engines:
- Insomniac, Spider-Man's tiles (primary):
  <https://blog.playstation.com/2018/09/06/insomniac-interview-the-tech-behind-marvels-spider-man>
- Unreal Engine 5 World Partition (primary):
  <https://dev.epicgames.com/documentation/en-us/unreal-engine/world-partition-in-unreal-engine>
- Brian Karis, Nanite, SIGGRAPH 2021 (primary):
  <https://advances.realtimerendering.com/s2021/Karis_Nanite_SIGGRAPH_Advances_2021_final.pdf>
- J.M.P. van Waveren, id Tech 5 virtual texturing, SIGGRAPH 2009 and GTC 2010 (primary):
  <https://www.quaddicted.com/webarchive/mrelusive.com_20170131/publications/presentations/2009_siggraph/05-JP_id_Tech_5_Challenges.pdf>

Distributed worlds:
- Unreal's replication graph (primary):
  <https://www.unrealengine.com/en-US/tech-blog/replication-graph-overview-and-proper-replication-methods>
- Improbable patents on interest, load balancing and authority handover (primary):
  <https://patents.google.com/patent/US10579434>
- Cloud Imperium on server meshing and the replication layer (primary, via a fan mirror):
  <https://api.star-citizen.wiki/comm-links/18397>
- CCP Veritas, "Introducing Time Dilation" (primary):
  <https://www.eveonline.com/news/view/introducing-time-dilation-tidi>

Compute markets and verification:
- Waldspurger et al., Spawn, IEEE TSE 1992 (primary): <https://waldspurger.org/carl/papers/spawn-ieee-tse-feb92.pdf>
- Lai et al., Tycoon, 2004 (primary): <https://arxiv.org/abs/cs/0412038>
- Akash bids and leases (primary): <https://akash.network/docs/getting-started/intro-to-akash/bids-and-leases>
- Golem market module (primary): <https://docs.golem.network/docs/golem-js/reference/interfaces/market_market_module.MarketModule>
- iExec proof of contribution (primary): <https://docs.iex.ec/protocol/proof-of-contribution>
- David Anderson, BOINC, and its job replication (primary): <https://arxiv.org/pdf/1903.01699>,
  <https://github.com/BOINC/boinc/wiki/Job-replication>
