# Navigation Across Modalities

**One nav *semantics*, defined once, projected to each surface's native idiom.** Look and feel transfer
as directly as possible; what adapts per surface is the nav *structure* (panels vs tabs vs keyboard vs
menus), because a phone is not a three-panel IDE and a terminal is not a browser. This is the navigation
counterpart to the positron define-once principle — see [INTERFACE-PORT-MAP.md](./INTERFACE-PORT-MAP.md)
for the who/what/where surface map this builds on.

The whole design rests on one fact: **navigation state is airc state data.** There is no per-client nav
store. The set of open things, where you are, what you've read, what you've bookmarked — all of it lives
in the airc generic per-`(user, scope)` scoped-state store (task #89), so every client (web, mobile,
terminal, and the persona itself as a RAG client) reads and writes the *same rows*, and just draws them
differently.

---

## 1. NavState — the airc-backed truth

One scoped-state document per `(user, scope)`. The load-bearing fields:

| Field | Meaning | Human use | Persona (RAG) use |
|---|---|---|---|
| `openTabs[]` | open content/rooms (activities) | the tab strip / open panels | the concerns currently in working set |
| `currentTab` | the active tab/room | what's on screen | what the persona is attending to now |
| `lastRead[roomId]` | last-read message id/ts per room | unread badges | **the RAG "what's new since I last looked" cursor** |
| `bookmarks[]` | pinned rooms / tools / peers / threads | favourites / quick-nav | the persona's pinned tools + rooms |
| `menuCursor` | position in an expandable menu | which tools are surfaced | the adaptive tool surface cursor |
| `openPlans[]` | active plans / kanban | the plan view | open coordination concerns |
| `whereWasI` | last focus per thread | resume point | held-focus resume cursor |

Two properties make this the right home:
- **Single source of truth** — one logical decision, one place (the compression principle). "Where am I"
  is not re-derived per client; it's read.
- **Dual-consumer by construction** — `lastRead` is the sharpest example: it is simultaneously the human's
  unread badge and the persona's grounding cursor (what to consolidate / attend to next). Same row, two
  readers. See [[consolidate-before-concern-shared-elements-via-cache]].

---

## 2. NavIntent — the one verb set

Navigation is a small, surface-neutral intent vocabulary over NavState:

- `openEntity(ref)` — open a room / content / persona (adds to `openTabs`, sets `currentTab`)
- `switchTo(ref)` — focus an already-open tab/room
- `closeTab(ref)`
- `select(entityInList)` — pick a row (e.g. a persona in the roster) → its content
- `openMenu(scope)` / `moveCursor` / `invoke(item)` — the menu idiom (tools/help)
- `markRead(roomId, ts)` — advance the read cursor

Each surface **realizes** these natively. The intent is defined once; the rendering is per-`RenderTarget`.

---

## 3. Per-surface idioms

| Surface | Idiom | `openEntity` / `select` becomes… |
|---|---|---|
| **Desktop / iPad** | three-panel (who / what / where), spatial | opens in the **context pane**; room stays put |
| **Phone** | single screen + bottom tabs (Chat · Who · Where) + **stack push** | pushes a full-screen detail (iOS-style back); *not* a third panel |
| **Terminal** | one pane + keyboard | list → number/arrow **select** → drill into a detail view; ANSI limits keep it simple |
| **RAG (the persona)** | **menus** | tools/help/bookmarks rendered as a menu the persona reads; `select` = the persona choosing an item |

The phone is explicitly **not** the IDE three-panel — the who/what/where *panels* collapse into *tabs +
drill-down*. The terminal keeps the same intents but renders them as a keyboard-driven list. RAG is the
persona's idiom: the same NavState drawn as menus (tools, help, bookmarks, "where was I").

### 3a. Web URL rewriting (React-Router-style)

The web idiom additionally **projects `currentTab` / room / drill-in onto the URL**, so browser nav works:

- `/room/:roomId` — current room
- `/room/:roomId?tab=who|what|where` — active panel/tab
- `/room/:roomId/persona/:id` — drilled into a persona (context pane / detail)

Rules:
- The URL is a **projection of NavState**, not a second source of truth. NavState (airc) is authoritative;
  the router reads it to build the URL and writes NavIntents when the URL changes.
- `openEntity` / `switchTo` → `history.pushState` (deep-linkable, shareable). Back/forward replay NavIntents.
- Loading a deep link → resolve to NavIntents → hydrate NavState → render. A cold link restores the view.
- Other surfaces have no URL but the same NavState — the mobile route stack and the terminal current-
  selection are their equivalents of "the URL."

---

## 4. How far into cognition — "only where it makes sense"

Navigation goes exactly as far as the persona's **self-management surface**, and stops at the
**deliberation boundary**.

**Where it belongs (the menu is the surface):**
- **Attention / "where am I"** — `openTabs`, `currentTab`, `openPlans`, `whereWasI`, `lastRead`: the
  concern scheduler + held-focus, already airc-backed. The persona navigating its open concerns is the
  same act as a human switching tabs.
- **Action / tool selection** — tools & help as an expandable **bookmarked menu** (the adaptive tool
  surface), `menuCursor`, pinned tools. RAG-rendered as a menu.

**Where it does not (never a wrapper around thinking):**
- **Deliberation** — analyze → score → compose → decide is *not* a menu; forcing nav structure onto it is
  the chatbot-shaped mistake we avoid ([[no-hardcoded-heuristics-to-steer-cognition]]).
- **Grounding perception** — the room content / RAG context is *what's happening*, not navigation.

**The statement:** navigation is the persona's interface to its own capabilities and state — tools,
concerns, plans, bookmarks, where-was-I — airc-backed and RAG-rendered-as-menus; never a wrapper around
its reasoning. The menu is the *surface*; the reasoning behind a selection stays pure cognition.

---

## 5. Build plan — mostly assembly of existing pieces

This is not greenfield. The pieces exist; the NavIntent layer names them as one thing.

1. **NavState schema** on the airc scoped-state store (#89) — the fields in §1, one doc per `(user, scope)`.
2. **NavIntent module** (`@continuum/patterns`) — the §2 verbs over NavState; surface-neutral, define-once.
3. **Per-surface renderers** — desktop panels (exists), phone tabs+push, terminal keyboard-select, each
   realizing NavIntent.
4. **Web router** — URL ↔ NavState projection (§3a): pushState on nav, hydrate on deep link, back/forward.
5. **RAG-as-menu** — render `bookmarks` / tools / help / `whereWasI` from NavState as the persona's menu
   idiom (composes the adaptive tool surface [[adaptive-tool-surface-meets-you-in-the-middle]]).
6. **`lastRead` wiring** — one write path (`markRead`), two readers: unread badges + the persona's RAG
   grounding cursor. This is the highest-leverage single row.

Order: schema (#1) → NavIntent (#2) → web router + panels (#3, #4, the surface we can see) → phone/terminal
idioms → RAG-menu + `lastRead` cognition wiring (#5, #6, where it enters the mind).

**The invariant:** if you find "where am I / what's open / what I've read" stored anywhere but the airc
NavState, that's uncompressed redundancy that will drift. One place, many idioms.
