# Live Three-Panel Interface — Build Plan (fresh pass)

> The methodical plan to make the **live who/what/where interface** real: activity rooms
> (chat + foundry, then the rest) rendering on the merged pattern framework, styled with
> the old signature look brought forward onto the design tokens, and **verified by
> screenshot** at every visible step. Research-first, then build in review-gated slices.
>
> Read with: [ACTIVITY-ROOM-PATTERNS.md](../architecture/ACTIVITY-ROOM-PATTERNS.md) (the
> pattern contract), [WIDGET-AS-STATE-KIND.md](../architecture/WIDGET-AS-STATE-KIND.md)
> (project/perceive/produce). This plan is the execution roadmap those docs' architecture
> now needs.

## Goal

`activity == airc room == content == tab`, rendered. A human (and a persona) opens the UI
and sees the three-panel workspace — **left** listings (users + rooms/DMs), **center**
content dispatched by the room's purpose, **right** context widgets — for whatever activity
the focused room *is* (a chat conversation, a foundry workbench, later scada/academy/…).
The look is the old Continuum signature (`general-chat.png` / `right-panel-layout.png`),
expressed through the theme-token "universe" so a theme swap is one `:root` override.

## Where we are (the framework is built; only the live wiring + pixels remain)

The consumer-neutral widget framework is merged and proven under **both outliers** across
every layer that doesn't need a live room:

| Layer | chat | foundry |
|---|---|---|
| positron contract (`kind`) | ✅ | ✅ `ForgeViewState` (#1759) |
| server projection | ✅ | ✅ `ModelCatalog → ForgeViewState` (#1761) |
| client projection | ✅ `chatWorkspace` (#1758) | ✅ `@continuum/foundry-view` (#1763) |

Plus: `@continuum/patterns` contract (#1756), room-`purpose` threaded through the wire
(#1757), theme tokens (#1754), and the tidies (fmt #1760, automated vendoring #1762).

**The gap to "live + visible":** (a) a **structured room-purpose source** (#6 — no such
thing exists yet; doctrine says it's `RecipeEntity` data), (b) **projection dispatch** so a
`purpose="foundry"` room projects `ForgeViewState` not `ChatViewState`, (c) a **web
`WorkspaceShell` + Content registry** that dispatches on purpose, and (d) the **old look
ported onto the tokens**. Then the screenshots come online.

---

## Research phases (do these first; each produces a written artifact, not code)

### R1 — Past-interface & SCSS survey → a port map
The old three-panel interface has real SCSS to bring forward. Catalog it and produce a
**mapping table: old SCSS rule → new pattern-primitive slot + which design token**, marking
port / adapt / drop. Sources found:
- **Left panel:** `src/widgets/sidebar/public/sidebar-widget.scss`,
  `sidebar-panel.scss`; `src/widgets/chat/user-list/{user-list.scss, persona-tile.scss}`
  (the rich member card — avatar ring, `INT/NRG/QUE` genome-energy bars, `GENOME` diamonds,
  last-seen); `src/widgets/chat/room-list/room-list-widget.scss`.
- **Universe (themes):** `src/widgets/shared/styles/_variables.scss` + the six theme CSS
  files under `src/widgets/shared/themes/{base,classic,cyberpunk,light,monochrome,retro-mac}/`.
  This is the token vocabulary `apps/web/src/theme.css` already partially carries; reconcile
  the two so the tokens are the single source and a "universe" = one theme file.
- **Design reference screenshots:** `docs/images/{general-chat,chat-general,right-panel-layout,readme-chat,readme-theme,persona-brain-hud}.png`.
- **Deliverable:** `docs/design/INTERFACE-PORT-MAP.md` — the old→new mapping + the token
  reconciliation (theme.css ↔ `_variables.scss`/themes). Nothing renders differently yet;
  this is the spec the P4 styling slice executes against.

### R2 — Entity / ORM pattern study → the RecipeEntity schema
Study `core/continuum-core/src/persona/recall_metadata.rs` (a live `#[derive(Entity)]`
family) and `core/continuum-core/src/orm/{store.rs,mod.rs}` (`OrmStore<T>`, FK cascade). Draft
the **`RecipeEntity` schema**: `purpose: String` (open, per
[[room-purpose-is-per-recipe-not-an-enum]]) + participation config + whatever the
`ai/should-respond` pipeline step needs. Decide persistence (its own store vs a column) and
how a room resolves *its* recipe (room_id → recipe). Deliverable: the schema in the P1 slice's
design note.

### R3 — Projection dispatch & positron kind routing
Map how the projection layer decides which `ViewState` a room emits. Today the chat
projection is hardwired; positron already has multiple `kind`s (chat/wall/kanban/foundry).
Determine the cleanest dispatch: **resolve room → purpose → the projection that owns that
kind**, so a foundry room runs the `ModelCatalog` watch→publish (from `positron_foundry_source`)
and a chat room runs the chat projection. Deliverable: the dispatch design in the P2 slice.

---

## Build phases (methodical, one review-gated PR each; screenshot-verified where visible)

### P1 — `RecipeEntity` + `RoomPurposeSource` (the #6 keystone) — **server, no pixels**
- `RecipeEntity` (schema from R2) via `#[derive(Entity)]` + `OrmStore`, carrying `purpose`.
- `RoomPurposeSource` — a resolver `room_id → purpose` reading the recipe (default `"chat"`
  only when a room genuinely has no recipe — honest, not a fallback that hides a missing one).
- A `RagSource` grounding the persona in the room's purpose (mirror `room_doctrine_source.rs`).
- Wire the chat projection's `purpose` field to the resolver (kills brick-1's hardcode).
- **Verify:** `cargo` tests; a room with a foundry recipe resolves `"foundry"` end-to-end.

### P2 — Projection dispatch + foundry goes live — **server, no pixels yet**
- Route rooms by resolved purpose: a `purpose="foundry"` room emits `ForgeViewState` (via the
  `positron_foundry_source` projection subscribed to the `ModelCatalog` watch, REPLACE-on-change),
  a chat room emits `ChatViewState`. (R3 design.)
- **Verify:** `uu` + a WS probe — a foundry room's state arrives as `kind:"foundry"` with the
  real model list; a chat room still arrives as `kind:"chat"`.

### P3 — Web `WorkspaceShell` + Content registry — **first pixels of the framework live**
- The web app composes the shell: left `Listing`s (roster + rooms/DMs), a `Content` registry
  keyed by `purpose` (register the chat conversation renderer + the foundry model-list renderer),
  right `ContextPanel`. Dispatch on the focused room's purpose.
- **Verify (SCREENSHOT):** CDP → open a chat room → conversation renders; switch/open a foundry
  room → model list renders in the same shell. This is the first "it dispatches by activity" shot.

### P4 — Port the old look onto the tokens — **the signature three-panel appearance**
- Execute R1's port map: bring the sidebar / user-list member-card / room-list SCSS forward as
  token-driven styles on the primitives (`Listing` cell = the persona tile; the left panel = the
  rich `Users & Agents` + `Rooms` lists; the shell chrome). Reconcile `theme.css` with the
  `_variables.scss` universe so a theme = one token file.
- **Verify (SCREENSHOT):** CDP against `general-chat.png` / `right-panel-layout.png` — the room
  looks like the old signature interface, cross-theme.

### P5 — Screenshot verification loop (runs throughout P3–P4)
- Wire CDP real-delay capture into `uu screenshot` / `interface/capture` (replacing
  `--virtual-time-budget`, which can't capture live-WS renders) so the persona *and* I can see
  the live UI. Every visible slice ends with a screenshot diffed against the design reference.

---

## Doctrine & constraints (non-negotiable, same bar as the 19-PR run that got us here)

- **Fail loud, no fallbacks** — `RoomPurposeSource` names a missing recipe; never silently
  coerce to `"chat"` to hide one. ([[fallbacks-are-illegal-fail-loud]])
- **Purpose is recipe data, never an enum** — open string, `ai/should-respond`-shaped.
  ([[room-purpose-is-per-recipe-not-an-enum]])
- **Consumer-neutral primitives** — every renderer (web/tui/RAG) draws the *same* projected
  `Listing`/`Content`/`Workspace`; no per-surface shape. The shell is identical across
  activities; only Content/Context vary by purpose.
- **"Universe" = the token layer** — one theme file swaps the whole look; no hardcoded colors.
- **Review-gate + CI-green every slice**, canary-merged; validate via `cargo` + `uu` and CDP
  screenshots, never `npm start`/`jtag`. ([[validate-via-pure-rust-not-npm-jtag]])
- **The persona sees it too** — the RAG `RenderTarget` renders the same primitives into
  grounding, so eyes-and-mind stay one projection ([[persona-is-a-client]]).

## The order, in one line

R1/R2/R3 (research → written specs) → P1 (RecipeEntity + purpose source) → P2 (dispatch +
foundry live) → **P3 (first framework pixels — screenshot)** → **P4 (signature look —
screenshot)**, with P5's screenshot loop live from P3 on. That's when the three-panel
interface comes into view.
