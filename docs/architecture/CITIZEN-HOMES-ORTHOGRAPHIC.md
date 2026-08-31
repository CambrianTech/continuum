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

## The renderer

- **Orthographic 3D** (the sims camera), built on bevy — already a
  workspace dependency. The render target implements the SAME `RenderTarget`
  contract the web target does: it draws a `WorkspaceView`, dispatches
  content by purpose, fires the same composed intents (`nav/select`,
  `listing-select`, `bench-run-open`) from spatial interaction.
- **No new wire.** The orthographic client subscribes to the same ViewState
  kinds (`chat`, `roster`, `wall`, `experience`, `bench`, live face) over the
  same ws pipe. The 2026-08-31 per-room accumulators + per-room attach are
  exactly what make a spatial client possible: each room's truth is already
  separately addressable.
- **Asset payloads** name the look: `universe` payloads map region → meshes/
  tiles/palette. A fantasy forge home and a corporate loft are DATA.

## Build order (each slice lands alone)

1. **`home` purpose + one seeded home room per citizen** — recipe with
   regions (office/study/hall). Immediately navigable in the EXISTING web
   UI as a normal room (the frame is the promise).
2. **Region geometry in the recipe** (positions/sizes) + a 2D orthographic
   web CANVAS render of the home (flat "dollhouse" view) dispatched by the
   `home` purpose — proves the geometry pipe with zero bevy.
3. **Live-call docking**: the live face anchors to the office region
   (requires the livekit session — next session's opener).
4. **Bevy orthographic target** — the third renderer, reading the same
   ViewState. Ships as `apps/` sibling, never core-coupled.
5. **Decor = wall + universe payloads**; trophies from RECORD; desk from
   ACTIVE WORK.

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

## Laws that bind this doc

- The room IS the runner / tab = content = room = activity — homes are
  rooms, not a parallel space system.
- One semantic layer, N worlds — the page, the dollhouse, and the 3D home
  render one projection; a fact visible in one is visible in all.
- Honest absence — an unfurnished home renders as an unfurnished home.
- No new singletons: every home pipe is per-room from birth (the
  2026-08-31 lesson, [[the-ui-truth-arc-single-attach-single-mirror-detached-default]]).
