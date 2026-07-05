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
  /** Optional named gauges (0–100), drawn as bars/meters by a target — a member's
   *  genome-energy vitals, a model's download %, a room's activity. Keeps the neutral
   *  cell LOSSLESS so rich rows (the roster's ACT meters) survive the projection instead
   *  of forcing the rich view-model across the boundary. Empty/absent = no meters. */
  readonly meters?: Readonly<Record<string, number>>;
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

// ── defineApp / mount — define an app ONCE, render it on every modality ───────

/**
 * An app, **defined once**: given domain `State`, `project` it to the neutral
 * who/what/where `WorkspaceView` — plus its `universe` (look/lore key a target maps
 * to styling). That is the ENTIRE app: purely neutral, ZERO dependency on the SDK,
 * the DOM, Flutter, or any target. The same `AppDefinition` mounts to web (Lit),
 * mobile (Flutter), terminal (ANSI), and RAG — the frameworks paint, positron
 * defines ([[three-separable-layers-recipe-positron-universe]]).
 */
export interface AppDefinition<State> {
  /** Domain state → the neutral view-model. The app's "what to show". */
  readonly project: (state: State) => WorkspaceView;
  /** Universe key (look/lore); a target maps it to its styling. Optional. */
  readonly universe?: string;
}

/** A live data source: pushes `State` on every change, returns an unsubscribe.
 *  Injected at `mount` so the SAME app runs against a real core, a replay, or a test
 *  fixture — the app never names its source ([[logical-portability-for-unknown-future-integrations]]). */
export type AppSource<State> = (onState: (state: State) => void) => () => void;

/** A surface sink: receives the target's rendered `Out` on each change and puts it on
 *  screen (DOM replace, ANSI write, a RAG buffer). Target-specific; the app is not. */
export type AppSink<Out> = (out: Out) => void;

/**
 * The framework seam: normalize/return a typed app definition. Identity today; the
 * single place future validation (universe resolution, activity-coverage checks)
 * hangs off — build now in a shape that welcomes the future.
 */
export function defineApp<State>(def: AppDefinition<State>): AppDefinition<State> {
  return def;
}

/**
 * Mount an app onto ONE modality: `source → project → target.workspace → sink`.
 * The SAME `app` + `source` mounts to ANY `RenderTarget` — **define once, render
 * everywhere.** Returns a teardown (unsubscribes the source).
 *
 * ```ts
 * const app = defineApp({ project: chatWorkspace });          // once
 * mount(app, sdkSource, webTarget,     el.replaceChildren);   // web (Lit)
 * mount(app, sdkSource, flutterTarget, flutterSink);          // mobile (Flutter)
 * mount(app, sdkSource, ragTarget,     ragBuffer);            // agents (RAG)
 * ```
 */
export function mount<State, Out>(
  app: AppDefinition<State>,
  source: AppSource<State>,
  target: RenderTarget<Out>,
  sink: AppSink<Out>,
): () => void {
  return source((state) => sink(target.workspace(app.project(state))));
}

/**
 * `createRagTarget` — a reusable **RAG adaptation rule**, authored ONCE, that any app
 * inherits. It proves the automatic-per-surface model ([[best-ux-per-portal-not-identical-projection]]):
 * given the SAME semantic `WorkspaceView` that web renders as a three-panel and terminal as
 * WHO/WHAT sections, this derives an entirely different, surface-appropriate output — a
 * **concise grounding block for a persona's LLM context** — automatically, by RULE, not by
 * per-app design. The rule: *room + who + primary content; DROP the nav, the secondary
 * listings, the context chrome* — only what an agent needs to act now. `content` dispatches
 * the primary body through the caller's registry (the app supplies a concise renderer).
 */
export function createRagTarget(content: ContentRegistry<string>): RenderTarget<string> {
  const names = (v: ListingView): string => v.cells.map((c) => c.title).join(', ');
  return {
    // In a grounding block a Listing collapses to just its members' names — no rows, no chrome.
    listing: (view: ListingView): string => names(view),
    content: (view: ContentView): string => content.render(view),
    // Context chrome is dropped entirely — an agent grounds on who + what, not side widgets.
    contextPanel: (_view: ContextPanelView): string => '',
    workspace: (ws: WorkspaceView): string => {
      const room = ws.nav.cells[0]?.title ?? 'a room';
      const who = ws.left[0] && ws.left[0].cells.length > 0 ? names(ws.left[0]) : 'no one else';
      return `You are in "${room}" with ${who}.\n${content.render(ws.content)}`;
    },
  };
}

// ── Mobile adaptation rule — authored + tested ONCE, every native painter inherits it ──

/** One bottom-nav destination on the mobile shell: a secondary listing (Who / Where) the
 *  phone reveals one tab at a time — never all panels crammed at once. */
export interface MobileTab {
  readonly id: string;
  readonly title: string;
  readonly cells: readonly ListingCell[];
}

/** The MOBILE adaptation of a `WorkspaceView`: the primary content owns the screen; the
 *  secondary listings become bottom-nav tabs; per-cell **dossier** detail (badges, meters,
 *  subtitle) is DROPPED — a phone shows presence, not a dossier. A native painter (Flutter,
 *  Swift, Kotlin) consumes THIS; the rule that produces it lives + is tested here, so the
 *  native app is a thin painter, not a place UX decisions are re-made
 *  ([[best-ux-per-portal-not-identical-projection]]). */
export interface MobileScreen {
  readonly title: string;
  readonly primary: ContentView;
  readonly tabs: readonly MobileTab[];
}

/**
 * `toMobileScreen` — the `@media (modality: mobile)` rule as a pure, testable function.
 * Derives a phone-native layout from the SAME neutral `WorkspaceView` the desktop paints as
 * a three-panel: conversation full-screen (`primary`), the who/where listings behind a
 * bottom nav (`tabs`), each cell stripped to presence essentials (id, title, glyph, status)
 * — the dossier badges/meters a desktop row can afford are dropped. Authored once; the
 * Flutter/Swift/Kotlin painter renders `MobileScreen` → native widgets. Verifiable without a
 * simulator — the rule is logic, the pixels are the grid's last mile.
 */
export function toMobileScreen(ws: WorkspaceView): MobileScreen {
  const presenceOnly = (c: ListingCell): ListingCell => ({
    id: c.id,
    title: c.title,
    ...(c.glyph !== undefined ? { glyph: c.glyph } : {}),
    ...(c.status !== undefined ? { status: c.status } : {}),
  });
  const tab = (v: ListingView): MobileTab => ({
    id: v.id,
    title: v.title,
    cells: v.cells.map(presenceOnly),
  });
  return {
    title: ws.nav.cells[0]?.title ?? '',
    primary: ws.content,
    tabs: ws.left.map(tab),
  };
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
