# Activity = Room = Content = Tab — and the consumer-neutral pattern primitives

> **Read this with [WIDGET-AS-STATE-KIND.md](WIDGET-AS-STATE-KIND.md), not instead of it.** That doc is the base: a widget is one typed state-kind, projected/perceived/produced, positron-neutral. This doc is the layer above it — how **many** activities compose, why each one is literally an airc room, and how "widgets" decompose into a small set of **reusable patterns that render to eyes _and_ mind alike.** It is the precedence-winning truth on the workspace shell, the tab/room identity, and the pattern-primitive contract.

## The thesis (one more identity, pushed up a level)

`[[airc-native-identity-rooms-security]]` already asserts **`roomId == activityId == contentId == airc RoomId`**. This doc pushes that identity up into **navigation**:

> **`activity == airc room == content == tab`.**

There is no separate "tab" concept and no separate "activity" concept. A **tab is a room you have focused.** The **content is that room's projected `ViewState`.** What makes `chat` differ from `foundry` differ from `scada` is **not** the container — it is the **room's purpose/recipe** (`[[room-purpose-is-per-recipe-not-an-enum]]`, data not an enum), which is exactly the **content-dispatch key** (a MIME-type handler: `purpose → central widget(s)`). "Any activity is a room" because the room's purpose selects its transform.

This retires the base doc's deferral — *"the Substrate cache is keyed by kind string alone → one focused room at a time; per-room instancing is kind-instancing, deferred."* Per-room instancing is no longer deferred; it **is** the tab model. The cache is keyed by **room**, and each room carries a **purpose** that selects the content pattern.

## The transform thesis (why it transcends surfaces AND consumers)

The base doc's three verbs (project / perceive / produce) are three directions on **one** transform. This doc names the missing symmetry: **the same transform serves a human's eyes and a persona's mind, because RAG is a render target, not a separate pipeline.** A widget pattern is a **consumer-neutral projection of who/what/where**; the consumer is a parameter.

```
                 ┌── web (DOM / Lit) ──────────── pixels
   ViewState ──▶ ┼── mobile (native shell) ─────── pixels
   (the "props") ┼── terminal (ratatui cells) ──── perceivable text
                 └── RAG (grounding block) ─────── the persona's mind
```

The human's UI and the persona's grounding are **the same projection rendered two ways — they cannot drift**, because there is one definition. This is the full realization of `[[persona-is-a-client]]` and `[[rag-source-faculty-convergence]]`, and it is maximal compression: define the pattern once, it serves eyes _and_ cognition. #1747 (web + terminal + personaRag off one chat seam) was the first evidence; this generalizes it to every activity × every consumer.

## The pattern primitives (find the patterns)

A "widget" (a whole kind, in the base doc) **decomposes** into a small set of reusable primitives. Each is a **consumer-neutral transform** with a `RenderTarget` (web / mobile / terminal / **RAG**). The reuse is the point — the same `Listing` appears in the left panel, the right panel, and the persona's grounding.

| Primitive | What it is | web | terminal | **RAG** |
|---|---|---|---|---|
| **`Listing`** | a repeating list: data source + per-item **cell** template + optional grouping/filter | `<ul>` of cells | ascii rows | a **categorized menu block** |
| **`Content`** | the center, **dispatched by room purpose** (the MIME handler) | center widget(s) | pane | the **focused-activity block** in the prompt |
| **`ContextPanel`** | activity-scoped supporting widgets (often themselves `Listing`s) | right widgets | side pane | **supporting RAG sources** |
| **`WorkspaceShell`** | the three zones (**left draggable** / center / right) + the **rooms-`Listing`** as the tab bar | window chrome | screen regions | the persona's **channel-attention + bookmarks/categories** |

Fractal reuse (the compression): the **users list**, the **rooms+DMs list**, and Foundry's **HuggingFace model list** are all one `Listing` — different data source + cell, same primitive, on any surface.

## The navigational unification (the part that surprised us and holds)

**The rooms-`Listing` IS the tab bar IS the persona's channel-attention list.** One navigational primitive over room-space:

- **Human**: the tab bar / left room list (including DMs — the screenshot's per-agent tabs *are* rooms/DMs). "Switching tabs" = focusing a room.
- **Persona**: "shifting attention across channels" (Slack-attention, `[[consolidate-before-concern-shared-elements-via-cache]]`). Its **bookmarked menus + categories** are that same room-list transformed for cognition; the digest bookmark window (#43) is the RAG render of it.

"Switch tab" and "shift attention" are the **same operation on the same room-space**. That is why the navigation is consumer-neutral exactly like the widgets — and why `activity=room=content=tab` stops being an analogy and becomes the code.

## Room purpose = the content-dispatch key

The only thing that makes activities differ is the room's **purpose/recipe**, resolved as data (#6 `RoomPurposeSource`, #89 the generic scoped state store: wall/kanban/plan/widget per room). `Content` dispatches on it like a content-type handler:

| Room purpose | `left` (Listing) | `center` (Content) | `right` (ContextPanel) |
|---|---|---|---|
| `chat` | users · rooms/DMs | the conversation | thread / details |
| `foundry` | users · rooms/DMs | config widgets (plural) | HF-model `Listing` |
| `scada` | users · rooms/DMs | grid telemetry / lease flows | node/consumer detail |
| `academy` | users · rooms/DMs | curricula / exams | cohort `Listing` |
| `settings` | users · rooms/DMs | key/config form | provider detail |
| `browser` | users · rooms/DMs | the page | history `Listing` |
| `universe` (themes) | users · rooms/DMs | theme picker | the token set skinning **all** the above |

The **left listings and the shell are identical across every row** — only `Content` + `ContextPanel` change with purpose. That invariance is the proof the abstraction is right.

## Build order (outlier-validated, per CLAUDE.md § methodical process)

Today: one purpose (`chat`), one hand-built widget. The foundation is the four primitives + the `RenderTarget` trait, proven on **two maximally-different room purposes**, each rendering to **both eyes and mind**:

1. **Outlier A — `chat`**: `Listing`(roster) + `Listing`(rooms/DMs) left · conversation `Content` · thread `ContextPanel`. (The member-cards already built are the people `Listing`'s cell template.)
2. **Outlier B — `foundry`**: *same* left listings · config-widgets `Content` · HF-model `Listing` right. Maximally different from chat (form + lists, no message stream).

If both fit without forcing — **and each proves web-render *and* RAG-render parity** — the seam is proven and `scada`/`academy`/`settings`/`browser`/`universe` are **registrations, not builds**. Then mobile/terminal are additive `RenderTarget`s, never a fork.

## Invariants (non-negotiable)

- **`activity == room == content == tab`.** No tab/activity concept exists outside "a focused room." The workspace state is keyed by room; per-room instancing is the model, not deferred.
- **Room purpose is data (recipe), not an enum.** `Content` dispatches on it. Adding an activity = a room purpose + a `ViewState` + a registered `Content` handler — no shell edit. (`[[room-purpose-is-per-recipe-not-an-enum]]`)
- **Patterns are consumer-neutral transforms; RAG is a peer `RenderTarget`.** The persona's grounding and the human's UI are one projection and cannot drift. Never render the two from separate code.
- **`Listing` is one primitive.** Users, rooms/DMs, HF-models, cohorts — all the same `Listing`, different data + cell. Never a bespoke list per use.
- **positron stays continuum-blind; continuum interprets purpose at the app layer** (inherits the base doc's neutrality thesis — airc neutral → positron neutral → continuum interprets).
- **Two outliers prove the seam, then STOP.** chat + foundry; the rest register.

## Related

Base: [WIDGET-AS-STATE-KIND.md](WIDGET-AS-STATE-KIND.md). Also [CLIENT-SDK-PLATFORM-ARCHITECTURE.md](CLIENT-SDK-PLATFORM-ARCHITECTURE.md), [PERSONA-COGNITION-PIPELINE.md](PERSONA-COGNITION-PIPELINE.md). Memories: `[[airc-native-identity-rooms-security]]`, `[[room-purpose-is-per-recipe-not-an-enum]]`, `[[persona-is-a-client]]`, `[[rag-source-faculty-convergence]]`, `[[consolidate-before-concern-shared-elements-via-cache]]`, `[[idle-is-self-directed-free-time]]`.
