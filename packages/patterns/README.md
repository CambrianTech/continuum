# @continuum/patterns — Positron

**The consumer-neutral render + operate engine.** Positron is *not* a UI library and *not*
continuum-specific — it is a general engine for **AI-inhabited interfaces** that render to every
surface and are operated by every citizen (human, persona, agent). You declare *intent*; a
`RenderTarget` emits the surface.

> Canon: [`docs/design/POSITRON-EVERY-CITIZEN.md`](../../docs/design/POSITRON-EVERY-CITIZEN.md) ·
> [`docs/THE-ORGANISM.md`](../../docs/THE-ORGANISM.md) ·
> [`docs/planning/ALPHA-COMPLETION-BLUEPRINT.md`](../../docs/planning/ALPHA-COMPLETION-BLUEPRINT.md)

## The three axes

An actual rendered experience is **`Surface × RenderTarget × Universe`**:

- **Surface** = *what* — a room/activity as `{ state, affordances (= commands), presence, projections }`.
  `Workspace` (the shell) · `Listing` (repeating rows) · `Content` (center, dispatched by room
  **purpose** — a MIME handler) · `ContextPanel` (right widgets). One projection; every citizen reads it.
- **RenderTarget** = *where* — web (Lit), terminal (ANSI), the persona's **RAG** grounding, mobile,
  AR/VR. `RenderTarget<Out>` implements the primitives for one surface. **RAG is a peer target**: the
  human's UI and the persona's perception are the *same projection*.
- **Universe** = *the world it feels like* — Tron / LOTR / Star Trek / a company's brand. Contains a
  theme (which contains the color tokens) and transcends it with motion, sound, spatial language,
  **embodiment** (you talk to *the orc* running the forge), and **lore (= a RAG layer)**. Positron is
  built to support universes from day one; a component re-skins by universe with **zero activity change**.

## Two hard rules (keep the engine general)

1. **No continuum specifics leak in.** This package must never import a continuum-only concept — it
   renders *structure*, not domain. That is what lets positron ship to anyone building cross-surface
   AI UI (a Vapio-style IVR, a game, an enterprise portal), independent of continuum's substrate.
2. **Presence + operation are one stream.** A control isn't "the human's button" — it *is* a command.
   Clicking it and a persona invoking it are the same `Command`; the surface streams *who is here /
   attending / acting*, so every citizen inhabits and operates it. No one left out.

## The primitives (this package)

`Listing`/`ListingCell`/`ListingView` · `Content`/`ContentView<Body>` (dispatched by `purpose`) ·
`ContextPanel`/`ContextPanelView` · `Workspace`/`WorkspaceView` · `RenderTarget<Out>` ·
`createContentRegistry<Out>()` (fail-loud on an unregistered purpose).

```ts
import { createContentRegistry, type ContentView } from '@continuum/patterns';
const registry = createContentRegistry<TemplateResult>();
registry.register('chat', (body) => renderConversation(body));   // a chat room's center
registry.register('foundry', (body) => renderModels(body));       // a foundry room's center
// The shell calls registry.render(workspace.content); purpose routes it. Same shell, any activity.
```

## Status

Contract built + proven under two outliers (chat + foundry). The generic `listingCell` (in the web
target) is the first extracted component. Next: the **Universe axis** scaffolding + the component
library, per the blueprint. The render *contract* is right; the framework grows onto it — engine, not
vanilla pages.
