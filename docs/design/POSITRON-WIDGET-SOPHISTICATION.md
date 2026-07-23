# Positron widget sophistication — the old interface, defined once for every surface

> Goal (Joel): take the sophistication of the old chat interface — the rich left widgets —
> and **define it once in the neutral positron vocabulary so it renders on every modality**
> (web / mobile / terminal / RAG / AR). Grounded in `docs/images/live-session-avatars.png`
> (metrics sparkline + persona tiles with INT/NRG/ATN + GENOME + the rooms/DMs list) and
> `docs/images/persona-brain-hud.png` (the Cognitive System View). Governed by the render
> model in POSITRON-EVERY-CITIZEN.md; built under "outlier-validate THEN abstract" — add a
> widget kind only when a real widget needs it, never speculatively.

## The map — old widget → neutral positron form

| Old-interface widget | Neutral positron form | New vocab? | Data it needs | Per-surface render |
|---|---|---|---|---|
| **Rooms / DMs list** (All·Rooms·DMs filter) | `Listing` (the `nav`) with **grouped** cells + a filter facet | **No** — `ListingView` + `ListingCell.group` exist; filter = a facet over groups | core projects the room/DM SET (today `nav` carries one focused room) | web: filterable list · terminal: grouped lines · RAG: a room menu |
| **Persona tile — INT/NRG/ATN meters** | `ListingCell.meters` (multiple named gauges) | **No** — `meters` already carries N named 0–100 gauges | persona emits INT/NRG/ATN (today: fewer) | web: bars · terminal: ASCII bars · RAG: `INT 80 NRG 90 ATN 72` |
| **Persona tile — GENOME block** (discrete lit cells) | a new **`MeterBlock`** cell field (discrete segments, not a bar) | **Yes** — `ListingCell.genome?: number` (lit/total) or a segmented meter | persona emits paged-LoRA / genome fill | web: diamond cells · terminal: `▰▰▱▱` · RAG: `genome 2/6` |
| **System metrics sparkline** (CPU/MEM/GPU) | a new **`Gauge`/`Sparkline`** widget (a `ContextPanel` item) | **Yes** — a `GaugeView { label, series }` sibling of `ListingView` | core projects system resource metrics (ResourceGovernor #56) | web: sparkline · terminal: ASCII bars · RAG: `CPU 58% · MEM 25/32G · GPU 6.5/25G` |
| **Brain-HUD** (Cognitive System View: prefrontal/limbic/hippocampus/motor/CNS) | a new **`Panel`/`HUD`** Content body (structured sub-sections) — it's an *activity* (`purpose: "mind"`) | **Yes** — a `Content` purpose + a `HudView { sections: {label, status, metrics}[] }` | persona projects its WorkspaceCycle cognition state (glass box) | web: HUD boxes · terminal: labelled sections · RAG: a cognition summary · **AR: a room you stand in** |
| **Avatar video tile + call controls** | a **`Media`** cell/tile + a **`Controls`** bar (controls ARE commands) | **Yes** — `MediaView { stream }` + controls = command affordances | LiveKit stream + call state (already live, #112) | web: `<video>` + buttons · terminal: text status · **AR: the 3D avatar** |
| **Tabs** (multi-activity) | the `nav` `Listing` (tab bar == channel-attention) | **No** — already the positron nav primitive | multiple open activities | all surfaces |

## What this tells us

- **Half the sophistication is already expressible** — the rooms/DMs list is a grouped `Listing`,
  the multi-meter tile is `meters`, the tabs are `nav`. These need **data**, not new widgets: the
  core must project the room SET, richer vitals, etc. (a projection expansion, not a vocabulary one).
- **The genuinely-new widget kinds are four**, each a sibling of `ListingView`/`ContentView` with its
  own per-surface renderers: **`Gauge`** (sparkline/metrics), **`MeterBlock`/genome** (segmented),
  **`Hud`** (the brain-HUD, itself a `Content` purpose), **`Media`** (avatar). Each is added the way
  `meters` was: enrich the neutral shape, project it losslessly, give each `RenderTarget` a renderer.
- **The render model already carries it** — every one of these has a web / terminal / RAG / AR
  projection (the table's last column). That's the proof the neutral vocabulary *can* hold the old
  interface's richness for every citizen, not just the browser.

## Build order (outlier-validate, one at a time — never all four at once)

1. **Rooms/DMs `Listing`** — lowest new-vocab (reuses `Listing` + `group` + a filter). Needs the core
   to project the room set into `nav`. Render on web + terminal, screenshot/frame-verified. *Proves the
   data-expansion path.* **✅ LANDED (2026-07-23)**: room-set fold + per-citizen nav projector wired at
   the WS `?me=` seam (core), `ListingCell.count` + `roomsListingFromNav` + unread pills (web);
   live-verified end-to-end (probe: real `kind="nav"` frame; browser: rail draws the live room set).
   Remaining inside this brick: room CLICK→switch (needs the `nav/select` current-tab write + chat
   projection refocus/reseed — the `markRead` sibling), DM grouping once DM rooms carry a marker.
2. **`Gauge` widget** (system metrics) — the first genuinely-new widget kind; a `ContextPanel` sibling.
   Small, self-contained, sources from ResourceGovernor (#56). *Proves the new-widget-kind path.*
3. **GENOME `MeterBlock`** on the persona tile — extends the roster cell (the `meters` pattern again).
4. **`Hud`** (brain-HUD as a `purpose:"mind"` Content) — the glass box as an inhabitable activity;
   the AR payoff (`persona-brain-hud` → a room you walk into).
5. **`Media`** (avatar tile) — folds the live LiveKit stream (#112) into the `ContextPanel`/grid.

Each brick: enrich the neutral shape → lossless projection → per-surface renderer → verify on web
(screenshot) + terminal (frame) + assert the RAG text. Same discipline that carried `meters` and the
web/terminal RenderTargets. The old interface's whole sophistication, defined once, on every modality.
