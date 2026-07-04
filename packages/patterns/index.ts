/**
 * `@continuum/patterns` — the consumer-neutral widget pattern contract.
 *
 * The precedence-winning code expression of `docs/architecture/ACTIVITY-ROOM-PATTERNS.md`.
 * A "widget" is NOT a component; it is a **pattern** — a projection of who/what/where
 * that renders to any consumer. This package holds only the **shapes** (the "props")
 * and the **render contract**; it has NO DOM, NO ANSI, NO RAG specifics, and no
 * dependencies. Each surface (apps/web Lit, apps/tui ANSI, the Rust persona RAG
 * renderer, a future mobile shell) implements a `RenderTarget` over these shapes.
 *
 * The four primitives — everything decomposes into these:
 *   - `Listing`      — a repeating list (users, rooms/DMs, HF-models, cohorts…). One
 *                      primitive, reused everywhere a list appears.
 *   - `Content`      — the centre, **dispatched by room purpose** (a MIME-type handler).
 *   - `ContextPanel` — the right-hand activity widgets (often themselves Listings).
 *   - `Workspace`    — the shell: the rooms-`Listing` (tab bar == channel-attention)
 *                      + left listings + focused-room content + context panel.
 *
 * The thesis: `activity == airc room == content == tab`, and RAG is a peer
 * `RenderTarget` alongside web/mobile/terminal — so the human's UI and the persona's
 * grounding are ONE projection that cannot drift.
 */

// ── Listing ────────────────────────────────────────────────────────────────

/** Presence/liveness of a listing cell, when it has one (a member is active/idle;
 *  a room has unread; a model is loaded). `'none'` = the cell carries no status dot. */
export type CellStatus = 'active' | 'idle' | 'none';

/** One already-projected row of a `Listing`. Display fields only — the projection
 *  (e.g. `rosterListing` in `@continuum/chat-view`) has already resolved names,
 *  glyphs, and badges from the domain state; a `RenderTarget` only draws these. */
export interface ListingCell {
  /** Stable identity of the item this cell represents (member id, room id, model id). */
  readonly id: string;
  /** Primary label (a person's name, a room name, a model name). */
  readonly title: string;
  /** Optional secondary line (a role, a last-seen, a param count). */
  readonly subtitle?: string;
  /** Optional leading glyph — an emoji/icon token (the avatar in a people listing). */
  readonly glyph?: string;
  /** Optional short tags (kind, runtime, provider) — the target styles them. */
  readonly badges?: readonly string[];
  /** Optional presence/liveness; `'none'` draws no status indicator. */
  readonly status?: CellStatus;
  /** Optional grouping/category key (the "bookmarked menus + categories" axis). */
  readonly group?: string;
}

/** The `Listing` pattern — a repeating list. The SAME shape backs the users list,
 *  the rooms/DMs list, Foundry's HF-model list, and (in RAG) a categorized grounding
 *  menu. Different data + cells, one primitive, every surface. */
export interface ListingView {
  /** Which listing this is (`"roster"`, `"rooms"`, `"hf-models"`) — targets may key on it. */
  readonly id: string;
  /** Panel header ("Users & Agents", "Rooms", "Models"). */
  readonly title: string;
  /** The rows, already projected. */
  readonly cells: readonly ListingCell[];
}

// ── Content (dispatched by room purpose) ─────────────────────────────────────

/** The `Content` pattern — the centre of a workspace, selected by the room's
 *  **purpose** (the content-type / MIME key: `"chat"`, `"foundry"`, `"scada"`…).
 *  `body` is purpose-specific and opaque here; a per-purpose renderer registered on
 *  the target interprets it. This is what makes "any activity is a room": the purpose
 *  chooses the transform, not a bespoke container. */
export interface ContentView<Body = unknown> {
  /** The room purpose — the dispatch key. */
  readonly purpose: string;
  /** Purpose-specific payload the registered content renderer understands. */
  readonly body: Body;
}

// ── ContextPanel + Workspace ─────────────────────────────────────────────────

/** The right-hand `ContextPanel` — activity-scoped supporting widgets. Today a set
 *  of `Listing`s (Foundry's HF-models, a chat thread's participants); richer widget
 *  kinds join this union as they land. */
export interface ContextPanelView {
  readonly listings: readonly ListingView[];
}

/** The `Workspace` shell — the whole who/what/where for one focused room. `nav` is
 *  the rooms-`Listing`: the tab bar for a human, the channel-attention set for a
 *  persona. `left`/`content`/`context` are the three zones. */
export interface WorkspaceView {
  /** The rooms-`Listing` — tab bar == persona channel-attention (one nav primitive). */
  readonly nav: ListingView;
  /** Left-panel listings (users, rooms/DMs) — identical across every activity. */
  readonly left: readonly ListingView[];
  /** The focused room's content, dispatched by its purpose. */
  readonly content: ContentView;
  /** The right-hand supporting widgets for the focused activity. */
  readonly context: ContextPanelView;
}

// ── RenderTarget — the consumer-neutral render contract ──────────────────────

/** A per-purpose content renderer: given the `ContentView.body` and its purpose,
 *  produce this target's output. Registered on a `ContentRegistry` — the MIME table. */
export type ContentRenderer<Out, Body = unknown> = (body: Body, purpose: string) => Out;

/** The content-dispatch table for one target: `purpose → renderer`. `render` looks up
 *  by `view.purpose` and **fails loud** on an unregistered purpose — an unknown
 *  activity is a wiring bug, never a silent blank ([[fallbacks-are-illegal-fail-loud]]). */
export interface ContentRegistry<Out> {
  register<Body>(purpose: string, renderer: ContentRenderer<Out, Body>): void;
  render(view: ContentView): Out;
}

/** A consumer that renders the patterns to its own output type `Out` — web
 *  (a Lit `TemplateResult`), terminal (a `string` of cells), mobile (a native node),
 *  RAG (a grounding block `string`). One contract, every surface. The `content`
 *  method dispatches through the target's `ContentRegistry`, so adding an activity is
 *  registering a content renderer — never a new target. */
export interface RenderTarget<Out> {
  listing(view: ListingView): Out;
  content(view: ContentView): Out;
  contextPanel(view: ContextPanelView): Out;
  workspace(view: WorkspaceView): Out;
}

/** Build an empty content-dispatch table for a target. Fail-loud on unknown purpose. */
export function createContentRegistry<Out>(): ContentRegistry<Out> {
  const table = new Map<string, ContentRenderer<Out>>();
  return {
    register<Body>(purpose: string, renderer: ContentRenderer<Out, Body>): void {
      if (table.has(purpose)) {
        throw new Error(`content renderer already registered for purpose "${purpose}"`);
      }
      table.set(purpose, renderer as ContentRenderer<Out>);
    },
    render(view: ContentView): Out {
      const renderer = table.get(view.purpose);
      if (!renderer) {
        const known = [...table.keys()].join(', ') || '(none)';
        throw new Error(
          `no content renderer for room purpose "${view.purpose}" — registered: ${known}`,
        );
      }
      return renderer(view.body, view.purpose);
    },
  };
}
