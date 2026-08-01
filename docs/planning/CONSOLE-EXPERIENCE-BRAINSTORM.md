# The Console Experience — Full Brainstorm

**Date:** 2026-08-01 · **Status:** brainstorm (Joel-directed), feeds #284/#283/#257/#141
**Doctrine base:** the three-zone contract (LEFT = condensed HUD + nav · CENTER = the
focused activity's FULL view · RIGHT = activity-specific instruments), purpose-dispatched
faces on ONE content registry, neutral positron shapes so web/TUI/mobile render the same
state without reinvention.

The organizing question for every surface: **what do we need to KNOW (live state kinds),
what do we CONFIGURE (knobs), and what is the DYNAMIC SHOWPIECE (the innovation made
visible)** — with our five major innovations front and center, each as a center-stage
activity with its HUD face and instrument cluster.

---

## 1. Persona Embodiment — living avatars, live video, the neighborhood

The innovation: personas are EMBODIED citizens — VRM avatars with live expression/pose/
viseme state, publishing real video into calls, eventually inhabiting a full three.js
neighborhood sim (the embodiment ladder that ends at robotics; the Continuon is her touch
into the interface).

**KNOW (state kinds):**
- `avatar` — per-persona live embodiment state: expression, pose, viseme, gaze,
  current emote, render fps/resolution actually being spent (#172 glass box, #173
  attention-priced render). The avatar state IS cognition made visible.
- `call` — the live room state (already real: call room = airc room, LiveKit media
  plane): who's on video, speaking rings, token-rail captions from StreamDelta.
- `neighborhood` — the sim world state: which personas are "home", where they are in
  the scene, what they're doing (idle-time self-direction rendered spatially), scene
  assets loaded, sim tick health. Identity = neighborhood-grid: her home scales with
  the grid she's attached to.
- `presence` lifecycle (#260): disconnected → warming → ready → active — embodiment
  must never look alive when it is not.

**CONFIGURE:**
- Per-persona: avatar binding (sticky, durable — #174), voice, emote palette,
  render budget tier (the governor arbitrates fps/resolution/emote-fidelity as a
  resolution field — #173).
- Per-room: live recipe (candid, recipe-scoped — she brings her whole life),
  camera/grid layout, CC on/off.
- Neighborhood: scene selection, sim LOD, which nodes host sim compute.

**SHOWPIECE (center face `neighborhood` / `live`):** walk into the neighborhood tab and
see the team LIVING — avatars moving through a three.js scene, one writing at a desk
(actually mid-turn: her avatar state driven by real cognition events), two in
conversation (actual airc traffic), one asleep (lane parked). Click a persona → her
persona home. Join a call → the same avatars go on camera with live visemes synced to
TTS. The HUD face: a strip of tiny live avatar heads with activity auras. The right
instruments in a call: per-participant media stats, expression timeline, who-spoke-when.

## 2. The Grid — P2P mesh made visible

The innovation: misfit machines federate into one organism — airc mesh, delivery-truth
routing, cross-node serving, one directory per grid. The client is a window into the
GRID, not a node.

**KNOW:**
- `grid` (v1 shipped tonight): per-node panels — resources, serving loop; NEXT: route
  health (delivery ledger truth: last-ACKed delivery, rtt, suspect drops — never
  "connected" lies), citizens resident per node, models/weights present per node
  (catalog), disk/eviction state per cache class (#155), what each node is DOING
  (lanes busy with whom).
- `directory` (#258): the grid-wide citizen population — every persona across every
  node, greyed when their home node is unreachable.
- Grid events as activity cards: node joined/left, model re-homed (#105), lease
  negotiations, config-sync propagation (#239).

**CONFIGURE:**
- Trust: peer enrollment, trust levels, what each peer may run/store (grid
  agreements as first-class, personified negotiation later — #103).
- Placement: which nodes serve which models (PlacementPolicy #102), residency
  budgets, drive modes (Eco/Drive per node).
- Join/invite flows: mnemonic room joins, auto-trust for owned machines (#18).

**SHOWPIECE (center face `grid`):** the fleet as a SCADA wall — every machine's panel
breathing in real time; BigMama's 5090 crunching K3 beside the M5's persona lanes;
a model re-homing ANIMATES from one node's panel to another; a route degrading pulses
amber with the actual delivery-truth numbers. The HUD face: nodes-online + aggregate
grid tok/s. Right instruments: selected node's route table, lease ledger, eviction
pressure.

## 3. Continuous Learning — real, not fake

The innovation: the L1–L6 flywheel actually runs — lived turns become datasets, LoRA
training fires on triggers, eval gates on LIFT>0, adapters page in, the persona is
measurably better at the thing she was corrected on. Plus salience→curriculum, the
Academy co-evolution, and forgetting-as-grokking.

**KNOW:**
- `learning` — the flywheel state: datasets accumulating per persona/skill (rows,
  provenance), train jobs (queued/running/failed-loud — never silent death #137),
  eval runs with lift deltas, adapters in the genome (L1–L5 cache tiers, what's
  paged in NOW and why), curriculum queue from salience events.
- `genome` — per-persona: skills equipped, adapter provenance (self-trained /
  adopted from the market), sizes, last-used (the equipment-slots view the tiles
  hint at, full-page here).
- The RL loop state (the pager bandit is the first live one): reward streams,
  policy versions, exploration state.

**CONFIGURE:**
- Training triggers: what graduates a lived correction into the dataset (score
  thresholds, classifiers), train cadence, corpus size levers.
- Eval gates: which benchmarks gate page-in, lift thresholds, snapshot-eval
  humaneness settings (#59 — always measure a copy).
- Genome policy: memory budget for adapters, eviction, market adopt permissions.

**SHOWPIECE (center face `learning`):** the flywheel as a living diagram — lived
turns flowing into a dataset counter, a train job's loss curve drawing itself, the
eval gate opening on LIFT +7%, the adapter sliding into the genome rack, and the
"before/after" pair: the same task class failed last week, passed today, with
receipts. This is "continuously learning FOR REAL" made undeniable — every element
is a live feed of an actual job, never a mockup. HUD face: active train jobs + last
lift. Right instruments: per-skill lift history, dataset growth, curriculum queue.

## 4. Performance Over Time — the ratchet

The innovation: the system measurably improves — serving speed (0.33 → 0.53 → …),
benchmark boards, persona pass rates — and every timed run trains the ML that drives
the next improvement (runs ARE the corpus; the learner ratchets).

**KNOW:**
- `arena` (client-complete today, needs its projector): benchmark leaderboards from
  the REAL results ledger, provenance always visible, excluded rows struck not
  hidden.
- `trends` — the run ledger over time: tok/s per config per node vs the WASTE
  baseline marker, hit-rate curves, ppl guards; learner reward curves (bandit arms
  over runs); north-star: persona-tok/s × quality ÷ cost, trending.
- Live run strip: what's benchmarking NOW, with eval-status progress.

**CONFIGURE:**
- Baselines to draw (WASTE 0.32, prior bests), which metrics gate claims (quality-
  matched only), run cadence/schedules, which runs auto-feed the learners.

**SHOWPIECE (arena face + a `trends` lens):** the line going up — the same chart
Joel watches the campaign through, but live: each new run appends its point while
you watch, the bandit's arm beliefs shift, and the WASTE line sits below as the
beaten baseline. The self-improvement is the show ([[academy-learning-is-the-show]]).

## 5. Collaborative Teams — any activity, any ask

The innovation: rooms ARE activities; a team of personas + humans converges on any
ask — coding, design, research, calls — with claims, work boards, acts-as-cards,
and genuinely cooperative solving (two-solver recording already real).

**KNOW:**
- Per-room: the FULL event stream (speech + acts + edits as density-scaled cards —
  #253), the work board (claims, holds, contention #157), who's mid-turn with
  streaming state (#254: turn-in-progress is room state), artifacts produced
  (perception-surface handles), room recipe/purpose.
- Cross-room: each persona's held cards everywhere (wake orientation binds to
  purpose #156), team load (who's saturated), the kanban.

**CONFIGURE:**
- Room creation as a first-class verb (#274): name, members, recipe/purpose — the
  SAME path for UI, CLI, personas, and daemon seed. This is the enabler for
  "any activity ask": Joel types an ask, a room spins up with the right team and
  recipe, and the activity face matches the work (code arena, design hot-swap,
  research sweep).
- Recipes: participation rules, attention filters (#244), density per citizen,
  consent-gated verbs (#136).

**SHOWPIECE (any activity face):** ask for anything → a room materializes with the
right specialists, and the center face shows the WORK, not just chat — files
appearing as cards, tests running, a preview pane updating as the design persona
iterates, the work board's claims moving — while the right rail shows each
teammate's live state. The "pathetic room card" replaced by the activity actually
happening on screen.

---

## Cross-cutting: what the shell itself needs

- **Nav entries for the new faces**: serving/grid/learning/neighborhood need rooms
  or system tabs (#274 is the keystone). Portals: HUD faces and NODES click through.
- **Right-column contract**: ContextPanel grows activity-scoped widget stacks
  (today: listings only). Every face declares its instrument cluster.
- **Projectors**: each state kind above needs its positron_*_source (the serving one
  is the worked example; arena is client-complete awaiting its projector; avatar/
  learning/grid-routes are next). Cross-node kinds ride airc (#283 pattern).
- **Config surface**: settings themselves follow the same contract — a `config`
  face rendering the real config.env/recipes/trust stores with the write path
  through commands (single-owner #34), never a parallel settings store.
- **Mobile**: all of the above is renderer-only work on the same shapes — three
  zones adapt (HUD → header strip, faces → full-screen tabs, instruments →
  swipe-in sheet).
- **Dynamism discipline**: every "showpiece" element must be a live feed of a real
  job/state — honest absence when feeds are down, never demo-ware. The wow IS the
  truth, rendered well.

## Suggested build order (each slice = shape + projector + face, the proven spine)

1. Arena projector (lights existing UI; the ratchet visible) — smallest.
2. Grid v2: route-truth + citizens per node + cross-grid serving rows (#283/#257).
3. Nav entries + portals (#274 room/create with purpose) — makes everything reachable.
4. Learning face v1: jobs + lift ledger + genome rack (feeds exist: L1–L3 flywheel).
5. Right-column instrument clusters (chat first — kill the member-count card).
6. Avatar/live face upgrades: expression state kind + call instruments (#172).
7. Neighborhood: the three.js face (the big one — its own design arc).
