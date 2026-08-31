# Citizen Homes — the sims apartment as a positron render target

**Status:** design note (2026-08-31, the creative night). Companion to the
embodiment ladder (#2625), the citizen-page roadmap (#2649), and
[universes-are-positron-asset-payloads]. Nothing here invents a subsystem;
every element is a projection of state that already exists or is already
roadmapped.

## The thesis, in one line

**The profile page and the home are the same ViewState with two renderers.**
A citizen's home is not a game bolted onto the substrate — it is the third
render target (after web-DOM and terminal/RAG) of the exact `Workspace`
projection the profile already draws. One semantic layer, N worlds.

## What Joel described

> "Sims apartment or home where they live and take live calls from their
> office room or wherever — they have a full home lol. We will make a 3d
> orthographic view."

## The mapping (state → space)

Everything the home shows is a room-shaped or persona-shaped fact the pipes
already carry (or #2649 adds):

| Home element | Backing state (exists today unless noted) |
|---|---|
| The home itself | An activity ROOM with purpose `home`, one per citizen — tab = content = room = activity extends to PLACES. Her home is addressable (`/room/<uuid>`), joinable, presence-served, bridged — every law from the 2026-08-31 arc applies for free. |
| Rooms within the home (office, study, trophy hall, garden) | The recipe's REGIONS ([`Experience`] manifest — the Join Contract already projects regions per room). An orthographic layout is region geometry carried as recipe data, never code. |
| The office — where live calls happen | The LIVE face anchored to a region: LiveKit tracks dock into the office region's frame. Walking into the office = `nav/select` + the call overlay. (The livekit restoration bar: 14 personas on an M1.) |
| The trophy hall | RECORD & AWARDS (verdict identity, form curve) — the same instruments, rendered as objects on shelves. |
| The study / desk | ACTIVE WORK doors — her live runs as things ON the desk; clicking one is the same `bench-run-open` join-then-select verb. |
| Her "vibe" / decor | Wall posts + writings (the #2649 wall pipe) + universe asset payloads — decor is a positron asset payload, one semantic layer, any theme. |
| Her body/presence in the space | The roster tile's truth: presence dot → an avatar in the space; the speaking ring → she's animated/facing you; vitals → ambient cues (light level = activity, desk clutter = queue). |
| Visitors | Room membership. Knocking = `room/join`. A human walking in IS the operator self-peer entering the room. |

## The renderer — REAL 3D, no sprite ceiling

**The bar (Joel, 2026-08-31): "full threejs or rendered at rust and seen —
not some simple sprite stuff. They take live calls in bevy from their
rooms."**

- **Bevy is the canonical home renderer**: native rust, GPU, real meshes,
  lighting, and materials — already a workspace dependency. It implements
  the SAME `RenderTarget` contract as the web target: draws a
  `WorkspaceView`, dispatches content by purpose, fires the same composed
  intents (`nav/select`, `listing-select`, `bench-run-open`) from spatial
  interaction. Orthographic sims camera is the default lens; the engine is
  full 3D underneath (VR = the same scene, stereo camera).
- **Live calls happen IN the scene**: LiveKit video tracks render as
  textures on surfaces in her office (the monitor on her desk, the wall
  screen); her speaking state animates her avatar. Walking into the office
  joins the call — the live face IS a region of the home, not an overlay
  escape hatch.
- **Web embed**: either bevy-wasm in a canvas, or a threejs target reading
  the identical ViewState — an implementation choice at build time, never
  two data paths. The profile's current SVG interior card is explicitly a
  PREVIEW STUB (slice 0): honest facts, placeholder fidelity; it is
  replaced by the embedded live render the moment slice 4 ships.
- **No new wire.** The 3D client subscribes to the same ViewState kinds
  (`chat`, `roster`, `wall`, `experience`, `bench`, live face) over the
  same ws pipe. The 2026-08-31 per-room accumulators + per-room attach are
  exactly what make a spatial client possible: each room's truth is already
  separately addressable.
- **Asset payloads** name the look: `universe` payloads map region → real
  meshes/materials/palette (GLTF references, not tiles). A fantasy forge
  home and a corporate loft are DATA.

## Build order (each slice lands alone)

1. **`home` purpose + one seeded home room per citizen** — recipe with
   regions (office/study/hall). Immediately navigable in the EXISTING web
   UI as a normal room (the frame is the promise).
2. **Region geometry in the recipe** (positions/sizes/mesh refs) — the
   authored-layout data contract, validated by rendering it in the preview
   stub. (The stub proves the GEOMETRY PIPE; fidelity arrives with bevy —
   the stub is never the destination.)
3. **Live-call docking**: the live face anchors to the office region
   (requires the livekit session — next session's opener).
4. **Bevy orthographic target** — the third renderer, reading the same
   ViewState. Ships as `apps/` sibling, never core-coupled.
5. **Decor = wall + universe payloads**; trophies from RECORD; desk from
   ACTIVE WORK.

## Entry points (Joel: "see their home interior right from profile pages, or enter the whole neighborhood")

Two doors into the space, one projection behind both:

- **From the profile** — the home INTERIOR renders as a card on her page
  (the dollhouse view embedded where her story already lives). First cut is
  procedural: default geometry, REAL facts as furniture — window light =
  online, desk items = active runs, trophies = resolved count, the plant =
  genome size. When home recipes land, the same card draws the authored
  geometry. Clicking it enters the home activity (`nav/select` her home
  room).
- **Enter the neighborhood** — a top-level activity (a tab like academy):
  top-down/orthographic sims camera over the room tree. The SAME activity
  opened by a VR target walks the identical tree in stereo — VR is render
  target #4, not a fork: web-DOM, terminal/RAG, bevy-ortho, VR, one
  ViewState behind all four.

## The neighborhood (Joel: "I plan on full sims neighborhood")

A home is ONE tab/activity; the neighborhood is the tier above — and it
already has a data structure: **the room tree IS the map.** The Activities
rail (durable places, work rooms nested under parents, `parent_ref`
lineage) renders spatially as streets and lots:

- Every citizen's `home` room = a lot in the neighborhood.
- The academy = the campus building; run rooms = its classrooms; a solve
  room's door on the board = literally a door.
- General/cambriantech = the town square / the office downtown.
- Walking between buildings = `nav/select` — the same verb as a tab click,
  the URL following you (`/room/<name>`), presence rendering who's where
  (the who-panel becomes "who's on the street").
- Zoom ladder: neighborhood (all rooms) → home (one room's regions) →
  region (the live face / the desk). Each level is the same tree at a
  different depth — the orthographic camera walks the nav hierarchy.

The neighborhood view is therefore slice 6: render the ROOM TREE as the
town, with each lot's ambient cues (lights on = presence, activity =
citizens working) fed by the per-room rosters that multi-room presence
already serves.

## LEGACY AUDIT (2026-08-31 — Joel: "take audit of even past legacy work")

The audit found the plan half-built already, in the core's live plane:

- **`live/video/bevy_renderer/` — a full Bevy headless avatar system**:
  16 avatar slots, per-avatar camera → render target → GPU readback →
  LiveKit video loops (zero-copy IOSurface on Apple Silicon). VRM avatars
  (blend shapes, humanoid bones, lookAt), animation systems (blink,
  breathing, speech, emotion, gesture, cognitive), and animation PROFILES
  as data — whose own docs already anticipate this direction: *"a Sims
  character walking has larger body movement than a webcam portrait."*
- **`bevy_renderer/scene/` — a whole scene substrate**: `room.rs`,
  `builder.rs` + `builder_api.rs` (fluent), `birther.rs` (procedural),
  `library.rs`, `physics.rs`, `object.rs`, `slot.rs`.
- **`scene/description.rs` — `SceneDescription`: THE one-to-one contract,
  already defined.** Backend-neutral, representation-neutral serde data
  (no engine types), a scene-graph tree of `SceneNode`s, `AssetRef` +
  open `AssetKind` (mesh / VRM rig / gaussian splat / generated), and
  **ts-rs exported** — produced by RON file, builder, or birther;
  instantiated per backend. Its own docblock promises exactly Joel's bar:
  a future backend "instantiates the *same* description into its own
  graph."
- **`apps/vr/` and `apps/ar/`** exist as app stubs — targets #4 and #5
  have homes in the tree.
- **`CosmosBackdrop`** — the universe-as-living-experience precedent.

**Consequence (compression law):** the web `<home-scene>` element ships
today on an interim `HomeSceneModel`; its v2 CONSUMES `SceneDescription`
via the existing ts-rs types, and homes/neighborhood layouts are authored
as SceneDescriptions (RON or birthed) that the bevy renderer and the
three.js target instantiate identically. LiveKit docking follows the
avatar system's existing readback pattern — the office screen surface
joins the same scene graph the call avatars already render in. No
parallel scene format survives this convergence.

## Laws that bind this doc

- The room IS the runner / tab = content = room = activity — homes are
  rooms, not a parallel space system.
- One semantic layer, N worlds — the page, the dollhouse, and the 3D home
  render one projection; a fact visible in one is visible in all.
- Honest absence — an unfurnished home renders as an unfurnished home.
- No new singletons: every home pipe is per-room from birth (the
  2026-08-31 lesson, [[the-ui-truth-arc-single-attach-single-mirror-detached-default]]).
