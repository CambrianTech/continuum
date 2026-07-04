# Interface Port Map (R1) — old three-panel look → pattern primitives + tokens

> The R1 research deliverable of [LIVE-INTERFACE-BUILD-PLAN.md](../planning/LIVE-INTERFACE-BUILD-PLAN.md):
> a faithful map from the **old signature interface** (SCSS + screenshots) onto the
> **consumer-neutral pattern primitives** ([ACTIVITY-ROOM-PATTERNS.md](../architecture/ACTIVITY-ROOM-PATTERNS.md))
> and the **design-token "universe"**. No code renders differently from this doc; it is the
> spec P3 (the web `WorkspaceShell`) and P4 (styling) execute against, and the reference I
> diff screenshots against.

## Sources studied
- **Screenshots** (the ground truth of the look):
  - `docs/images/general-chat.png` — the **chat** activity, `base` universe (dark blue-gray, cyan `#00d4ff`). Three panels: left AI-Performance + Rooms + Users&Agents; center conversation + participant chips; right (collapsed).
  - `docs/images/right-panel-layout.png` — a **persona brain-HUD** activity ("Cognitive System View"), `cyberpunk` universe (near-black + magenta/cyan/green). **The same shell, a totally different center activity + a right-hand context chat** — the visual proof of `activity=room=content=tab` + Content-dispatch.
- **SCSS**: `src/widgets/chat/user-list/{user-list.scss, persona-tile.scss}` (the member card), `src/widgets/chat/room-list/room-list-widget.scss`, `src/widgets/sidebar/public/{sidebar-widget.scss, sidebar-panel.scss}`.
- **Tokens**: `src/widgets/shared/styles/_variables.scss` (legacy SCSS `$vars`) and `src/widgets/shared/themes/{base,classic,cyberpunk,light,monochrome,retro-mac}/theme.css` (the runtime CSS custom-props). `apps/web/src/theme.css` (the new partial subset).

## Decision 1 — the "universe" token layer (the reconciliation)

There are **two** token systems today; unify on **one**:

| System | What | Verdict |
|---|---|---|
| CSS custom-props `--token` | `themes/*/theme.css` — `--content-*`, `--background-*`, `--border-*`, `--button-*`, `--input-*`, `--message-*`, `--resizer-*`, `--right-panel-*`, `--scrollbar-*`, `--radius-*`, `--font-*`, `--shadow-glow`. Runtime-swappable at `:root`. | **CANONICAL.** This IS the "universe" — a theme = one `theme.css`. |
| SCSS `$var` | `_variables.scss` — `$color-primary`, `$spacing-*`, `$font-*`, `glow()` mixins. Compile-time, per-widget. | **RETIRE.** Port each `$var` reference to its `--token` equivalent; the primitives never see SCSS vars. |

**Action (P4 groundwork):** `apps/web/src/theme.css` currently carries a *subset*. Grow it to the **full `base/theme.css` token set** (it's already the same names — `--content-accent`, `--resizer-*`, `--right-panel-*`, etc.), then the other five universes drop in as sibling files. A theme swap = swap the `:root` block; nothing in a primitive or widget changes.

Signature `base` values to preserve: accent `--content-accent: #00d4ff` (cyan), `--status-online`/`--content-success` green glow, dark blue-gray gradient `--background-primary`, translucent surfaces, `--font-primary` SF Pro / `--font-mono`. Each universe re-skins ALL of it (cyberpunk = magenta/cyan/green on near-black, per `right-panel-layout.png`).

## Decision 2 — the shell anatomy (maps 1:1 onto the primitives)

From both screenshots, the shell is **fixed across every activity**; only center + right vary:

```
┌───────────────────────────────────────────────────────────────────────────┐
│ TAB BAR  = rooms-Listing (Pantheon·General·<active>·…)   [Theme][Settings]  │  ← WorkspaceShell nav
├──────────────┬────────────────────────────────────────┬─────────────────────┤
│ LEFT (drag)  │ CENTER = Content(dispatched by purpose) │ RIGHT = ContextPanel │
│ · metrics    │  chat → conversation                   │  chat → thread       │
│ · Rooms      │  foundry → config widgets              │  foundry → HF models │
│   Listing    │  persona → brain-HUD                    │  persona → help chat │
│ · Users&     │                                        │                     │
│   Agents     │   (resizer)                            │   (resizer)         │
│   Listing    │                                        │                     │
└──────────────┴────────────────────────────────────────┴─────────────────────┘
```

- **Resizers are already tokenized** — `--resizer-{width,background,border,glow}` (+ `:hover`/`:active`) and `--right-panel-{width,min,max,collapsed-width,collapse-threshold}`. Port the drag behavior; the look is token-driven.
- **Left panel = a vertical stack of widgets**, top to bottom: an optional **metrics** widget (AI Performance / Pipeline / Requests sparkline), the **Rooms `Listing`**, the **Users & Agents `Listing`**. The metrics widget is its own future activity-kind; Rooms + Users are the `Listing` primitive.

## The port map — old rule/widget → new primitive slot → tokens

| Old SCSS / widget | New home | Primitive slot | Tokens | Port / Adapt / Drop |
|---|---|---|---|---|
| `persona-tile.scss` (member card: 42px avatar + comet ring per cognitive phase, status dot, name, `PERSONA/HUMAN` kind, provider, `INT/NRG/QUE` bars, `GENOME` diamonds, last-seen) | `Listing` **cell** for people | `ListingCell` (glyph→avatar, title→name, subtitle→provider/last-seen, badges→kind+provider, status→dot) | `--border-subtle`, `--border-accent` (AI ring), `--status-online` (+glow), `--content-*`, `--radius-*` | **Adapt** — the richest cell; genome bars/GENOME diamonds need `RosterSlotView` to carry genome/energy data (a wire-additive follow-up), so ship the card now (avatar+dot+name+badges) and grow the meters when the data lands. |
| `user-list.scss` (panel header "Users & Agents (N)", search box, filter chips all/people/starred/bots, scroll) | left panel | `Listing` container (header + count + optional filter/search) | `--sidebar-background`, `--content-secondary`, `--input-*` | **Adapt** — header + count already in the web `<chat-widget>`; add search/filter to the `Listing` primitive as optional affordances. |
| `room-list-widget.scss` (Rooms header (N), room rows = name + description, active row highlight) | left panel | `Listing` (nav) — the rooms-Listing = tab source | magenta/accent header per universe, `--content-accent` active, `--content-secondary` desc | **Port** — this is the nav `Listing`; a room cell = `{title: name, subtitle: description, status: active}`. |
| `sidebar-widget.scss` / `sidebar-panel.scss` (draggable left panel shell, resizer, collapse) | `WorkspaceShell` left zone | shell chrome + resizer | `--resizer-*`, `--right-panel-*`, `--background-*` | **Adapt** — the shell owns layout + resizers; token-driven. |
| tab bar (screenshots; `main-widget`) | `WorkspaceShell` nav | rooms-`Listing` rendered as tabs + Theme/Settings/Help | `--content-accent` active underline, `--button-secondary-*` | **Port** — one nav primitive; tabs are its web RenderTarget. |
| participant chips row (general-chat.png header) | chat `Content` header | (chat renderer detail) | `--button-secondary-*` | **Adapt** — a chat-Content affordance, not a shell concern. |
| message bubbles | chat `Content` | (chat renderer) | `--message-{user,assistant}-*` | **Port** — already in the web `<chat-widget>`. |
| `_variables.scss` `$vars` + `glow()`/`glow-text()` mixins | — | — | → `--token`s + a `--shadow-glow` utility | **Retire** — replace refs with tokens; the glow becomes `box-shadow: var(--shadow-glow, …)`. |

## The signature member card (the one to get right)

Web RenderTarget of a people-`Listing` cell (`ListingCell`), styled from tokens:
- **avatar** — 34–42px circle, emoji/image; **AI members get the cyan/accent ring** (`--border-accent` + subtle glow); a live cognitive phase animates the ring (the old "comet-orbit" per `data-ai-status`).
- **status dot** — bottom-right, `--status-online` (+glow) when active, `--status-offline` idle.
- **name** — `--content-primary`, 600.
- **meta row** — kind badge (`human`/`agent`) + runtime/provider badge (`--content-accent`, accent-bordered) + last-seen (`--content-secondary`).
- **genome meters** (later, when `RosterSlotView` carries them) — `INT/NRG/QUE` bars + `GENOME` diamonds; this is what makes a persona feel *alive* in the list (the PX Joel wants).

## Hand-off to P3 / P4
- **P3 (WorkspaceShell + Content registry):** build the shell zones (nav / left stack / center / right) + resizers off the tokens; register the chat + foundry Content renderers; dispatch on `purpose`. First screenshot target: the shell renders a chat room, then a foundry room, in the same frame.
- **P4 (styling):** execute this table — port the member card + room list + shell chrome onto the full token set; add the other universes; diff against `general-chat.png` (base) and `right-panel-layout.png` (cyberpunk).
- **Data follow-ups surfaced:** `RosterSlotView` needs genome/energy fields for the meters; the metrics widget (AI Performance) is a future activity-kind; search/filter are optional `Listing` affordances.
