# Activity-Shell Convergence — first-principles diagnosis and repair (2026-08-14)

**Status: DIAGNOSIS + REPAIR DESIGN — no code changed yet. Written after the
2026-08-14 UI meltdown, from a full doctrine read + a full render-path trace,
at Joel's direction: "deep dive into architecture first… fix without haphazard
patches… first principles."**

---

## 0. The three systems, and the one identity law

- **airc** owns WHO and WHERE: identities (`PeerId`), rooms (`RoomId`), durable
  membership, the durable transcript, and the per-`(user, scope)` scoped-state
  store (nav, bookmarks, read cursors). It is the truth store.
- **continuum core** is the headless Rust organism: cognition, serving,
  benchmarks, commands. It consumes airc truth and produces behavior. Clients
  never own behavior.
- **positron** is the projection layer: typed, revisioned `ViewState`s
  *projected from* airc/continuum truth — "a projection of airc-owned truth,
  never a second store" (WIDGET-AS-STATE-KIND.md) — served over one WS to N
  renderers (web, mobile, tui, desktop) **and to AI observers**. Renderers are
  pure functions of ViewState.

**Identity law:** every id is a UUID, minted v4 / derived v5, passed and used
as a UUID. Names are display facts resolved *by* id. No composite strings, no
prefix-matching renderings, no `id: String`.

## 1. The design (doctrine, precedence-resolved)

`ACTIVITY-ROOM-PATTERNS.md` declares itself precedence-winning on shell/tab/
room identity and states:

> **activity == airc room == content == tab.** There is no separate "tab"
> concept and no separate "activity" concept. A tab is a room you have focused.

- **Recipe = template (class). Activity/room = instance (object).** The room's
  recipe/purpose is the content-dispatch key (a MIME-type handler:
  `purpose → central widget`). Rooms form URI-path trees; **benchmark runs are
  rooms** (`academy/bench/<run>`) under the academy *section* of the rooms
  tree — never a parallel panel.
- **Shell = three zones.**
  - **Left = GLOBAL widget stack (WHO + world), invariant under focus.** The
    purpose-dispatch table's `left` column is identical in every row: "the left
    listings and the shell are identical across every row — that invariance is
    the proof the abstraction is right."
  - **Center = the focused room's content**, dispatched on its purpose.
  - **Right = the focused room's ContextPanel** — **changes with
    content=tab=room=activity** (owner ruling 2026-08-14, resolving the
    four-doc conflict: not always-chat, not pinned, not a bolted-on board).
- **Substrate keying:** `ACTIVITY-ROOM-PATTERNS.md:13` explicitly retires the
  kind-alone cache: "per-room instancing is no longer deferred; it **is** the
  tab model. The cache is keyed by room." → **`(room_id, kind)`** for room
  content; node-scoped kinds (roster directory, system-metrics, serving, nav)
  for the global widgets. This is task #408.
- **Focus is a selection, never an eviction.** Every open room keeps its view;
  switching tabs re-points the center/right at an existing view.

## 2. The divergence — every symptom's mechanism (evidence: file:line)

The implementation still runs the *retired* design, and the shell has its axes
crossed: **the left changes when it must not; the right doesn't change when it
must; the center is empty when its content exists.**

1. **One mutable room cache, kind-keyed.** `positron_source.rs::switch_room`
   (:651-662) clears name/messages/acts/roster/vitals/loadout/genes on every
   focus change; `apply_focus` (:679-683) then publishes the empty view. A test
   (:1814-1845) pins the wipe as intended.
2. **The refills the wipe banks on don't exist.** Exactly ONE presence emitter
   runs, bound to the bootstrap room (`ipc/mod.rs:3221-3240`); the resync cue
   is room-agnostic (`positron_presence.rs:242-247`) so the one emitter
   re-asserts *its* room, which the focus pin then drops
   (`positron_source.rs:794-801`) — "yank" converted into *permanently empty*.
   The `chat/history` transcript backfill is named in three comments and
   implemented nowhere. Screenshotted live: academy 13 members → click general
   → 0 members, 0 messages, forever.
3. **The left rail is fed room-scoped data in global slots.**
   `patternProjections.ts:471-477`: `rosterListing(vm)`, `metricsWidget(vm)`,
   `continuonWidget(vm)` all derive from the per-room chat envelope. The
   node-global `kind="roster"` the core already publishes
   (`positron_source.rs:882-895`) is **dead code client-side** — its only TS
   consumer is referenced solely by its own spec. This is why a tab click
   zeroes USERS & AGENTS.
4. **One template, one error boundary, zero keys.** The rail, center, and
   context render from a single Lit template (`litTarget.ts:168-229`) inside
   one `render()` whose catch replaces the WHOLE page
   (`ChatWidget.ts:4194-4226`). The content registry **throws** on any room
   purpose outside its eight registered strings while the core emits recipe
   strings verbatim (e.g. `benchmark/hard-rs`) — one unregistered room blanks
   the entire UI including the rail. Lists are unkeyed positional `.map()`s;
   the nodes widget entering/leaving shifts indices and resets sibling element
   state; an `@error` handler imperatively `.remove()`s a node out of Lit's
   part tree (`parts.ts:503-509`). Full-page re-render every 2s from vitals +
   presence ticks. This is "random shit disappearing and coming back."
5. **The SYS graph "disappearing" is a designed 6s wall-clock face cycle**
   (`SysPanel.ts:37,73-82`) where two of three faces are not graphs and have
   different heights — the layout shift Joel observed. Owner ruling: all three
   faces are graphs, layout-stable.
6. **Nil-room wipes.** `apply_act` got a nil-room guard on 2026-08-12;
   `apply_message` (:716-723) and `apply_presence` (:794-802) did not — before
   any explicit focus, a stray event wipes and a later event restores:
   textbook flicker.
7. **The right rail is a parallel benchmark subsystem** bolted on room-blind —
   exactly what BENCHMARKS-ARE-ADAPTERS forbids. Runs already spawn per-run
   rooms (#329a); the UI never renders them as rooms.
8. **Personas render dead:** the ghost-union re-adds every remembered member
   with `active=false`, empty vitals (`positron_presence.rs:170-190`); presence
   slots hardcode empty vitals/genes (`positron_source.rs:346-359,402-405`);
   the vitals side-map is wiped on switch; presence never advances on turn
   completion (#260/#412). Grey rows, no genome blocks, by construction.

## 3. The repair — one design, dependency-ordered (no patches)

The invariant that makes the whole bug class impossible: **projection state is
keyed by `(room_id, kind)` plus node-scoped global kinds; nothing is ever
cleared on focus; the shell's three zones render and fail independently.**

- **R0 — the spine (#408).** Core: replace the single `ChatProjection`
  accumulator with a `RoomViews` map keyed by `RoomId` (UUIDs, by value in the
  key; structs by reference elsewhere). `nav/select` selects; it never clears.
  Wire: state envelopes for room content carry `room_id` (chat already does);
  the SDK caches per `(kind, room_id)` and replays `last_seen` per pair.
  Delete `switch_room`'s clear-set and the test that pins it.
- **R1 — global kinds feed global widgets.** The left rail subscribes to the
  node-scoped kinds only: `roster` (the citizen DIRECTORY — identity cards +
  presence overlay + vitals/genome, emitted node-level, not per-room),
  `system-metrics`, `serving`, `nav`. The existing `roster` publisher gets the
  vitals/genes merged in; the existing (currently dead) client consumer gets
  wired. `rosterListing`/`metricsWidget`/`continuonWidget` stop reading the
  chat envelope. Presence advances on turn completion, not only beacons.
- **R2 — shell isolation.** Left / center / right are separate render roots
  with per-zone error boundaries: a content-renderer failure renders an error
  card IN THE CENTER and never touches chrome. Content registry resolves
  unknown purposes to an honest fallback view (named "unregistered purpose
  <x>") instead of throwing. All lists keyed by UUID via `repeat()`. No
  imperative DOM mutation inside templates.
- **R3 — right = ContextPanel of the focused room** (owner ruling). The
  bolted-on bench rail is deleted; benchmark runs render as rooms under the
  academy section of the rooms tree; focusing one puts its board in the
  CENTER and its context on the RIGHT — perceivable by citizens through the
  same pipe (the BENCHMARKS-ARE-ADAPTERS acceptance test).
- **R4 — transcript truth.** Opening/focusing a room hydrates its
  `(room, chat)` view from the durable transcript (exists since #140); live
  events append. "No messages yet" only when the durable transcript is
  actually empty.
- **R5 — honest instruments.** SYS widget: three graph faces, fixed height;
  cycling allowed but layout-stable. Nil-room guards on `apply_message` /
  `apply_presence` (same as `apply_act`'s). Presence lifecycle per #260.

Acceptance, per zone: (a) click every tab 20× — left rail byte-identical
frames; (b) focus a benchmark room — board in center, context right, rail
unchanged; (c) kill a content renderer deliberately — center shows the error,
rail and right intact; (d) reload — every open room repaints from cache then
reconciles live.

## 4. What dies

The parallel bench rail; roster-derived-from-chat; the single bootstrap-room
presence emitter; the room-agnostic resync cue; `switch_room`'s clear-set; the
whole-page error boundary; unkeyed rail lists; the non-graph SYS faces.

## 5. The quality bar (owner, 2026-08-14): "like Flutter"

Positron's renderer contract is judged against Flutter, not against ad-hoc web
practice. Concretely, R2 means:

- **Rebuild scoping**: a widget rebuilds when ITS state kind changes — never
  because a sibling's did. One envelope must never repaint the page.
- **Keys everywhere**: children keyed by UUID (`repeat()`); composition changes
  never reset sibling element state.
- **Per-subtree error boundaries**: a failed build renders an error widget IN
  PLACE; ancestors and siblings are untouched.
- **Stateless by default**: widgets are pure functions of ViewState; the only
  stateful widgets are explicit (hover, drag) and own nothing durable.
- The same bar the owner applies from four hand-built CMSs (1999, 2004, 2011,
  2017/three.js): content outlives the view; navigation is selection, never
  destruction.

## 6. The widget contract (owner spec, 2026-08-14): one generic, written once

Every widget is an instance of ONE generic contract — "write one app, elegantly":

```
Widget<K: StateKind> {
  scope:    Global | Room(RoomId)          // which (room, kind) it observes
  store:    OrmStateStore                  // platform seam: IndexedDB (web),
                                           // SQLite (mobile/desktop), memory (tests)
  hydrate:  store.load(scope, K) -> paint immediately (offline-first; the
            Twitter model per WIDGET — intermittent connection matters not)
  live:     subscribe (scope, K) -> write-through to store -> rebuild SELF only
  render:   pure fn(ViewState<K>) -> tree, via RenderTarget (lit/flutter/tui)
  children: keyed by UUID, always
  boundary: own error widget in place; failure never escapes the subtree
}
```

Properties that fall out, rather than being bolted on:
- **Offline-first per widget**: every widget paints last-known state from its
  own ORM row before any socket exists; live reconciles. A dead core means a
  stale-badged UI, never a blank one.
- **Efficient redraw**: a widget rebuilds only when ITS (scope, kind) revision
  advances. No global re-render path exists.
- **Genericity is the compression**: users list, rooms tree, bench board,
  serving console — all `Widget<K>` with different K + cell templates. Never a
  bespoke widget with its own state plumbing.
- **One app, N platforms**: the widget tree + state kinds are the app; the
  RenderTarget and OrmStateStore are the only per-platform code. The SDK's
  existing `StateStorageAdapter` (IndexedDB/Memory, mirrored by swift/kotlin/
  flutter) is this seam's seed — generalize it, don't reinvent it.

### Authoring idiom (owner, 2026-08-14): SwiftUI / Jetpack Compose style

The widget contract is authored as **composable pure functions**, not markup
templates: XML-era declarativeness retained, the separate stringly layer
dropped. `workspace(left, center, right)` / `listing(cells)` /
`content(purpose, state)` compose like SwiftUI views; the framework owns
structural identity (keys), recomposition scope (only the subtree whose state
changed), and modifiers (theme/universe tokens as ambient environment, the way
Compose provides CompositionLocals). The existing pattern primitives are the
seed of exactly this — the rebuild disciplines them rather than replacing the
idiom.

### Future target: VR (owner, 2026-08-14)

VR/spatial is a planned RenderTarget, not a rewrite. The contract therefore
admits NO web-isms: widgets compose to an abstract tree; a spatial target
(wgpu/Bevy — the same stack as the universe payloads and VRM avatars) places
the three zones as panels in a scene, the roster as presence around you, a
room's content as the space you're standing in. activity == room == content ==
tab extends naturally: in VR a tab switch is walking into another room. Any
design decision that would break this target is wrong today.
