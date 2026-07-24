/**
 * Shared web render parts — the small Lit fragments the workspace renderers reuse.
 *
 * Extracted so both the roster/`Listing` rendering (left panel) and the per-purpose
 * `Content` renderers (center, via the Content registry) draw the SAME member/message
 * fragments without duplicating them ([[compression]]). No state, no substrate — pure
 * field→element maps, styled entirely from the design tokens.
 */

import { html, svg, nothing, type TemplateResult } from 'lit';
import { unsafeHTML } from 'lit/directives/unsafe-html.js';
import hljs from 'highlight.js/lib/common';
import type { GaugeView, ListingCell, ListingView, MetricsView } from '@continuum/patterns';
import type { LoadoutVM, MemberKind, MessageRowVM, RosterMemberVM } from '@continuum/chat-view';

/** Per-series hues for the SYS gauge — the old sidebar's legend palette (CPU
 *  red · MEM green · GPU purple), keyed by label with a cyan fallback for any
 *  future series so an unknown label still draws. */
const GAUGE_HUES: Record<string, string> = {
  CPU: '#ff5c5c',
  MEM: '#3fb950',
  GPU: '#a78bfa',
};

/** One series → an SVG polyline over a fixed 0..=100 viewBox. Points are
 *  already normalized upstream; x spreads the window across the width so a
 *  short (fresh-boot) series draws from the left edge outward, honestly. */
function sparkline(points: readonly number[], hue: string, w: number, h: number): TemplateResult {
  if (points.length < 2) return svg``;
  const step = w / (points.length - 1);
  const pts = points.map((p, i) => `${(i * step).toFixed(1)},${(h - (p / 100) * h).toFixed(1)}`);
  return svg`<polyline points=${pts.join(' ')} fill="none" stroke=${hue} stroke-width="1.5" vector-effect="non-scaling-stroke" />`;
}

/** The SYS gauge body — multi-series sparkline over a faint grid + the legend
 *  row (hue dot · LABEL · current reading). Shared by the `'gauge'` rail widget
 *  and `<sys-panel>`'s SYS face ([[compression]]). */
export function renderGaugeBody(view: GaugeView): TemplateResult {
  const W = 240;
  const H = 56;
  return html`<div class="gauge">
    <svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" aria-label="resource history">
      ${[0.25, 0.5, 0.75].map(
        (f) => svg`<line x1="0" y1=${H * f} x2=${W} y2=${H * f} class="gauge-grid" />`,
      )}
      ${view.series.map((s) => sparkline(s.points, GAUGE_HUES[s.label] ?? '#7dd3fc', W, H))}
    </svg>
    <div class="gauge-legend">
      ${view.series.map(
        (s) => html`<span class="gauge-key">
          <span class="gauge-dot" style="background:${GAUGE_HUES[s.label] ?? '#7dd3fc'}"></span>
          <span class="gauge-label">${s.label}</span>
          <span class="gauge-val">${s.current}</span>
        </span>`,
      )}
    </div>
  </div>`;
}

/** The team-cognition stat row (value over label, tone-coloured) — shared by the
 *  `'metrics'` rail widget and `<sys-panel>`'s AI face ([[compression]]). */
export function renderMetricsRow(view: MetricsView): TemplateResult {
  return html`<div class="metrics-row">
    ${view.stats.map(
      (s) => html`<span class="metric" data-tone=${s.tone ?? 'muted'}>
        <span class="metric-val">${s.value}</span>
        <span class="metric-label">${s.label}</span>
      </span>`,
    )}
  </div>`;
}

/** The composed select event a listing cell fires when the reader picks it — the
 *  `select(entityInList)` NavIntent reaching the web idiom. It bubbles out of the
 *  shadow tree to `<chat-widget>` (same pattern as `MESSAGE_EXPAND_TOGGLE`): the
 *  render fragments stay pure/stateless, the host owns what a selection DOES. */
export const LISTING_SELECT = 'listing-select';

/** Detail payload of a `LISTING_SELECT` event — which listing, which cell. */
export interface ListingSelectDetail {
  readonly listingId: string;
  readonly id: string;
}

export function fireListingSelect(e: Event, listingId: string, id: string): void {
  (e.currentTarget as HTMLElement).dispatchEvent(
    new CustomEvent<ListingSelectDetail>(LISTING_SELECT, {
      detail: { listingId, id },
      bubbles: true,
      composed: true,
    }),
  );
}

/** Which listing selections mean "switch room" — a pick in the rooms listing
 *  yields its room id; every other listing's select is not a room switch
 *  (`null`). Pure and DOM-free (it lives here, not on the widget) so the
 *  routing decision is unit-tested without a browser. */
export function roomSelectTarget(detail: ListingSelectDetail): string | null {
  return detail.listingId === 'rooms' ? detail.id : null;
}

/** GENERIC listing-cell renderer — the first real positron web *component*: it draws
 *  ANY already-projected `ListingCell` (a foundry model, a room, a cohort) the same
 *  way, on any target. The roster's rich member card stays bespoke while it carries
 *  live vitals meters; every other Listing routes through this. Two consumers
 *  (foundry now, more later) is exactly what earns the extraction — outliers first,
 *  then the component ([[positron-is-a-framework-not-vanilla-pages]]).
 *
 *  With `selectFrom` (the owning listing's id) the cell is SELECTABLE: click or
 *  Enter/Space fires `LISTING_SELECT` up to the host. No local active state — the
 *  active cell moves only when the substrate's next envelope arrives (same
 *  no-optimistic-append discipline as chat send). */
export function listingCell(cell: ListingCell, selectFrom?: string): TemplateResult {
  const select =
    selectFrom === undefined
      ? undefined
      : (e: Event): void => {
          fireListingSelect(e, selectFrom, cell.id);
        };
  const keySelect =
    select === undefined
      ? undefined
      : (e: KeyboardEvent): void => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            select(e);
          }
        };
  return html`
    <li
      class="cell"
      data-status=${cell.status ?? 'none'}
      data-selectable=${select ? '' : nothing}
      tabindex=${select ? '0' : nothing}
      @click=${select ?? nothing}
      @keydown=${keySelect ?? nothing}
    >
      ${cell.glyph ? html`<span class="cell-glyph">${cell.glyph}</span>` : nothing}
      <div class="cell-body">
        <div class="cell-title">${cell.title}</div>
        ${cell.subtitle ? html`<div class="cell-subtitle">${cell.subtitle}</div>` : nothing}
      </div>
      ${cell.count ? html`<span class="cell-count" title="unread">${cell.count}</span>` : nothing}
      ${cell.badges && cell.badges.length > 0
        ? html`<span class="cell-badges">
            ${cell.badges.map((b) => html`<span class="cell-badge">${b}</span>`)}
          </span>`
        : nothing}
    </li>
  `;
}

/** Render a `ListingView`'s rows — the roster as rich member cards (the neutral cell
 *  carries glyph/name/badges/status/meters), every other listing as generic cells.
 *  Single-sourced here so `webTarget.listing` AND the `'listing'` rail-widget renderer
 *  draw the SAME rows without duplication ([[compression]]). Generic cells carry their
 *  listing id so a pick fires `LISTING_SELECT` — the host decides which listings a
 *  selection means anything for (rooms today; an unhandled select is inert). */
export function renderListing(view: ListingView): TemplateResult {
  if (view.id === 'roster') {
    return html`<ul class="roster">${view.cells.map(memberCardFromCell)}</ul>`;
  }
  return html`<ul class="cells">${view.cells.map((c) => listingCell(c, view.id))}</ul>`;
}

/** Short glyph per author kind — the neutral human/agent/system discriminant. */
export function kindGlyph(kind: MemberKind): string {
  switch (kind) {
    case 'human':
      return '🧑';
    case 'agent':
      return '🤖';
    case 'system':
      return '⚙️';
  }
}

/** Human-readable member-kind label for the card badge. */
export function kindLabel(kind: MemberKind): string {
  switch (kind) {
    case 'human':
      return 'human';
    case 'agent':
      return 'agent';
    case 'system':
      return 'system';
  }
}

/** The runtime-origin badge, only when the substrate resolved one. */
export function runtimeBadge(runtime: string): TemplateResult | typeof nothing {
  return runtime ? html`<span class="runtime" title="runtime origin">${runtime}</span>` : nothing;
}

/** The avatar's live inference state — the SAME signal the chat header's "Asha is thinking…"
 *  reads. Drives the ring colour + pulse. `speaking` is the widget's overlay of the LIVE
 *  token rail (#170 StreamDelta — real tokens flowing right now), stamped onto the vitals
 *  the same way the expand overlay rides message rows; it outranks the slower (2s-sampled)
 *  radiator-derived thinking state. Error > speaking > thinking > presence. */
export function avatarState(v: Readonly<Record<string, number>>, active: boolean): string {
  if ((v.error ?? 0) > 0) return 'error';
  if ((v.speaking ?? 0) > 0) return 'speaking';
  if ((v.activity ?? 0) > 25 || (v.reason ?? 0) > 55) return 'thinking';
  return active ? 'active' : 'idle';
}

/** An emoji over the avatar for an emotional event. Today a proxy for the dominant cognitive
 *  faculty; real emotion events (mood/reaction) wire in later — that's the hard part. */
function emojiOverlay(v: Readonly<Record<string, number>>): TemplateResult | typeof nothing {
  const faces: [string, number][] = [
    ['🎯', v.focus ?? 0],
    ['🤔', v.reason ?? 0],
    ['💭', v.recall ?? 0],
    ['⚡', v.act ?? 0],
  ];
  faces.sort((a, b) => b[1] - a[1]);
  const top = faces[0];
  if (!top || top[1] < 66) return nothing;
  return html`<span class="emoji-overlay">${top[0]}</span>`;
}


/** The cognition diamond — four faculties of a working mind as four triangles, each lit by
 *  its live value. The SHAPE of the diamond is the shape of the mind that instant: a persona
 *  deep in thought skews toward Reason, one running tools skews toward Act. Dynamic from
 *  cognition events ([[design-the-persona-as-a-being]]). */
export function cognitionDiamond(v: Readonly<Record<string, number>>): TemplateResult {
  // FOUR distinct triangles pointing out like a compass (N=Focus, E=Reason, S=Recall,
  // W=Act), a gap in the centre — so it reads as four, and a strong faculty burns bright
  // while a dim one nearly vanishes (the SHAPE is the mind). No more solid blob.
  const lit = (k: string): number => 0.14 + 0.86 * (Math.max(0, Math.min(100, v[k] ?? 0)) / 100);
  const pct = (k: string): number => Math.round(Math.max(0, Math.min(100, v[k] ?? 0)));
  // Each faculty its own hue (color > monochrome) — readable by colour AND position.
  const tri = (pts: string, k: string, label: string, color: string): TemplateResult =>
    svg`<polygon points="${pts}" class="cog-tri" style="fill:${color};opacity:${lit(k)}"><title>${label} ${pct(k)}</title></polygon>`;
  return html`<svg viewBox="0 0 40 40" class="cog-diamond" aria-label="cognition">
    ${tri('20,2 12,15 28,15', 'focus', 'Focus', 'var(--faculty-focus)')}
    ${tri('38,20 25,12 25,28', 'reason', 'Reason', 'var(--faculty-reason)')}
    ${tri('20,38 12,25 28,25', 'recall', 'Recall', 'var(--faculty-recall)')}
    ${tri('2,20 15,12 15,28', 'act', 'Act', 'var(--faculty-act)')}
  </svg>`;
}

/** Fixed genome slot count — the legacy tile's four equipment slots (RPG loadout).
 *  Personas with more genes keep the top-4 slots lit; the panel tooltip names the
 *  overflow count. */
const GENOME_SLOTS = 4;

/** How many genome slots are lit. Gene NAMES (when the radiator reported them) are
 *  the count truth; an older core radiating only the numeric `genome` percent maps
 *  it back through the radiator's 6-gene full scale. */
function litGeneCount(v: Readonly<Record<string, number>>, genes: readonly string[]): number {
  if (genes.length > 0) return genes.length;
  const g = v.genome ?? 0;
  return g > 0 ? Math.max(1, Math.round((Math.min(100, g) / 100) * 6)) : 0;
}

/** The GENOME panel — the legacy persona-tile's labelled instrument block, faithfully:
 *  a rotated GENOME caption on the left, four FULL-HEIGHT gene slots (dark until a
 *  gene pages in — the reference's empty-but-visible equipment slots, never half-mast
 *  bars), and the cognition compass at the panel's top-right. Each lit slot is named
 *  by its real paged-in adapter ([[persona-tile-is-a-live-game-hud]]). Drawn for any
 *  member with live vitals; base-model personas show four dark slots — an honest
 *  "nothing loaded", not a hidden panel. */
export function genomePanel(
  v: Readonly<Record<string, number>>,
  genes: readonly string[] = [],
): TemplateResult {
  const lit = litGeneCount(v, genes);
  const title =
    lit > GENOME_SLOTS
      ? `GENOME — ${lit} genes loaded (top ${GENOME_SLOTS} shown)`
      : `GENOME — ${lit} gene${lit === 1 ? '' : 's'} loaded`;
  return html`<span class="genome-panel" title=${title}>
    <span class="genome-label">GENOME</span>
    <span class="genome-slots">
      ${Array.from({ length: GENOME_SLOTS }, (_, i) => {
        const isLit = i < lit;
        const name = genes[i];
        return html`<span
          class="genome-slot ${isLit ? 'lit' : ''}"
          title=${name ?? (isLit ? 'loaded gene' : 'empty slot')}
        ></span>`;
      })}
    </span>
    ${cognitionDiamond(v)}
  </span>`;
}

/** The labelled vitals stack — the legacy tile's INT/NRG/QUE rows, reborn on the live
 *  radiator's vocabulary: ACT (cognition tempo) and QUE (staged unread depth) are
 *  always radiated — present-at-0 DRAWS (an idle persona shows empty tracks, exactly
 *  like the reference's empty QUE row, never a blank tile); SPD/PAR are capability
 *  meters a richer source may attach. Absent keys draw nothing (honest). */
const STAT_ORDER: readonly (readonly [string, string])[] = [
  ['activity', 'ACT'],
  ['queue', 'QUE'],
  ['speed', 'SPD'],
  ['size', 'PAR'],
];

/** The meter stack — label · track · value per row, the info-dense heart of the
 *  glass-box tile, each row hoverable ([[persona-tile-is-a-live-game-hud]]). Fill
 *  hues key off `data-key` in CSS (named theme tokens, one per vital). */
export function personaReadout(v: Readonly<Record<string, number>>): TemplateResult | typeof nothing {
  const stats = STAT_ORDER.filter(([k]) => v[k] !== undefined);
  if (stats.length === 0) return nothing;
  return html`<span class="meters">
    ${stats.map(([k, label]) => {
      const val = Math.round(Math.max(0, Math.min(100, v[k] ?? 0)));
      return html`<span class="meter" data-key=${k} title="${label} ${val}">
        <span class="meter-label">${label}</span>
        <span class="meter-track"><span class="meter-fill" style="width:${val}%"></span></span>
        <span class="meter-val">${val}</span>
      </span>`;
    })}
  </span>`;
}

/** RAW parameter count → a compact unit label: `24_000_000_000` → "24B",
 *  `671_000_000_000` → "671B", `2_800_000_000_000` → "2.8T", `300_000_000` → "300M".
 *  The renderer owns the unit so the wire carries the honest raw count. */
function formatParams(n: number): string {
  if (n >= 1e12) return `${+(n / 1e12).toFixed(1)}T`;
  if (n >= 1e9) return `${Math.round(n / 1e9)}B`;
  if (n >= 1e6) return `${Math.round(n / 1e6)}M`;
  return String(n);
}

/** RAW context window (tokens) → a compact label: `32768` → "32k", `262144` → "256k",
 *  `200000` → "200k", `1_000_000` → "1M". Context windows come in TWO conventions: local
 *  GGUF models quote powers of two (32768, 131072, 262144), read in 1024-units so they land
 *  on the round 32k/128k/256k a model is actually known by; cloud models quote round base-10
 *  (200000, 1_000_000), read in 1000-units. Keying the unit on `% 1024` picks the right one. */
function formatCtx(n: number): string {
  if (n <= 0) return String(n);
  const unit = n % 1024 === 0 ? 1024 : 1000;
  const mega = unit * unit;
  if (n >= mega) return `${+(n / mega).toFixed(n % mega === 0 ? 0 : 1)}M`;
  if (n >= unit) return `${Math.round(n / unit)}k`;
  return String(n);
}

/** The LOADOUT strip — the model backing a persona, `model · size · ctx`. Each part
 *  drawn only when present; an all-absent loadout renders nothing (a human, an
 *  unresolved agent — honest, never a fabricated model line). The "model size,
 *  context size" Joel asked the tile to surface. */
export function loadoutStrip(lo: LoadoutVM | undefined): TemplateResult | typeof nothing {
  if (!lo) return nothing;
  const parts: string[] = [];
  if (lo.model) parts.push(lo.model);
  if (lo.params) parts.push(formatParams(lo.params));
  if (lo.contextWindow) parts.push(`${formatCtx(lo.contextWindow)} ctx`);
  if (parts.length === 0) return nothing;
  return html`<span class="loadout" title="model · size · context">
    ${parts.map(
      (p, i) =>
        html`${i > 0 ? html`<span class="loadout-sep">·</span>` : nothing}<span class="loadout-part"
            >${p}</span
          >`,
    )}
  </span>`;
}

/** Raw last-active epoch ms → the tile's relative stamp: `"55m ago"`, `"2h ago"`,
 *  `"3d ago"`, `"now"` under a minute. `undefined` when the fact is absent/unusable
 *  (no stamp drawn — honest, never a fabricated recency). Renderer-owned unit
 *  formatting, the sibling of `formatParams`/`formatCtx`. */
export function agoLabel(lastActiveMs: number | undefined, nowMs = Date.now()): string | undefined {
  if (!lastActiveMs || lastActiveMs <= 0 || lastActiveMs > nowMs + 60_000) return undefined;
  const mins = Math.floor((nowMs - lastActiveMs) / 60_000);
  if (mins < 1) return 'now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

/** The top-right recency stamp — the old tile's "55m ago". Nothing when unknown. */
function agoStamp(lastActiveMs: number | undefined): TemplateResult | typeof nothing {
  const label = agoLabel(lastActiveMs);
  return label === undefined
    ? nothing
    : html`<span class="ago" title="last active">${label}</span>`;
}

/** The avatar IMAGE layered over the glyph — the glyph stays underneath as the
 *  fallback, and a load failure simply removes the image (never a broken-image
 *  box, never a fabricated face). Absent url → nothing rendered. */
function avatarImage(url: string | undefined): TemplateResult | typeof nothing {
  if (!url) return nothing;
  const hide = (e: Event): void => {
    (e.currentTarget as HTMLElement).remove();
  };
  return html`<img class="avatar-img" src=${url} alt="" @error=${hide} />`;
}

/** One member card — avatar + presence dot, name, kind/runtime, live vitals —
 *  the old Users & Agents persona-tile as the `Listing` cell (INTERFACE-PORT-MAP.md). */
export function memberCard(m: RosterMemberVM): TemplateResult {
  const hasVitals = Object.keys(m.vitals).length > 0;
  return html`
    <li class="member clickable ${m.active ? 'online' : 'idle'}" data-kind=${m.kind} tabindex="0"
        title="Open ${m.name}">
      ${agoStamp(m.lastSeenMs)}
      <span class="avatar" data-state=${avatarState(m.vitals, m.active)}>
        <span class="glyph">${kindGlyph(m.kind)}</span>
        ${avatarImage(m.avatarUrl)}
        <span class="status-dot"></span>
        ${emojiOverlay(m.vitals)}
      </span>
      <span class="info">
        <span class="name">${m.name}</span>
        <span class="meta">
          <span class="kind-badge">${kindLabel(m.kind)}</span>
          ${runtimeBadge(m.runtime)}
        </span>
        ${loadoutStrip(m.loadout)}
        ${personaReadout(m.vitals)}
      </span>
      ${hasVitals ? genomePanel(m.vitals, m.genes ?? []) : nothing}
    </li>
  `;
}

/** One member card, reproduced from the NEUTRAL `ListingCell` (not the rich VM).
 *  This is what lets the web `RenderTarget` draw the roster from a `WorkspaceView`
 *  alone — the cell now carries everything the card needs: `glyph` (kind glyph),
 *  `title` (name), `badges` = [kind, runtime?], `status` (active/idle), `meters`
 *  (vitals). Byte-for-byte the same markup as `memberCard(vm)` — verified by the
 *  before/after screenshot of the live three-panel — so routing apps/web through the
 *  framework's neutral projection does not regress the ACT meters. */
export function memberCardFromCell(cell: ListingCell): TemplateResult {
  const active = cell.status === 'active';
  const kind = cell.badges?.[0] ?? 'agent';
  const runtime = cell.badges?.[1] ?? '';
  return html`
    <li class="member clickable ${active ? 'online' : 'idle'}" data-kind=${kind} tabindex="0"
        title="Open ${cell.title}">
      ${agoStamp(cell.lastActiveMs)}
      <span class="avatar" data-state=${avatarState(cell.meters ?? {}, active)}>
        <span class="glyph">${cell.glyph ?? ''}</span>
        ${avatarImage(cell.image)}
        <span class="status-dot"></span>
        ${emojiOverlay(cell.meters ?? {})}
      </span>
      <span class="info">
        <span class="name">${cell.title}</span>
        <span class="meta">
          <span class="kind-badge">${kind}</span>
          ${runtimeBadge(runtime)}
        </span>
        ${loadoutStrip(cell.loadout)}
        ${cell.meters ? personaReadout(cell.meters) : nothing}
      </span>
      ${cell.meters ? genomePanel(cell.meters, cell.genes ?? []) : nothing}
    </li>
  `;
}

/** Render message text with light markdown: fenced ```code``` blocks and inline `code`.
 *  Personas speak commands (`code/list --filter src/**`, ```bash … ```) constantly, and
 *  raw backticks in the transcript are noise — a monospace block reads as an action. Lit
 *  auto-escapes every interpolation, so the code text is inert (no HTML injection). */
export function formatContent(text: string): TemplateResult {
  const parts: TemplateResult[] = [];
  const fence = /```([\w+#.-]*)[ \t]*\n?([\s\S]*?)```/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = fence.exec(text)) !== null) {
    if (m.index > last) parts.push(inlineCode(text.slice(last, m.index)));
    const lang = (m[1] ?? '').toLowerCase();
    const code = (m[2] ?? '').replace(/\n+$/, '');
    const n = code.length === 0 ? 0 : code.split('\n').length;
    // Syntax-highlight (fence language, else auto-detect); hljs escapes the code, so the
    // resulting HTML is inert for unsafeHTML. Expandable: short blocks open, long ones
    // collapse behind a summary so a big command/output never buries the conversation.
    const highlighted =
      lang && hljs.getLanguage(lang)
        ? hljs.highlight(code, { language: lang, ignoreIllegals: true }).value
        : hljs.highlightAuto(code).value;
    parts.push(html`<details class="code-collapsible" ?open=${n <= 3}>
      <summary>${lang || 'code'}<span class="code-count">${n} ${n === 1 ? 'line' : 'lines'}</span></summary>
      <pre><code class="hljs">${unsafeHTML(highlighted)}</code></pre>
    </details>`);
    last = fence.lastIndex;
  }
  if (last < text.length) parts.push(inlineCode(text.slice(last)));
  return html`${parts}`;
}

/** Inline `code` spans → styled <code>; everything else passes through as text. */
function inlineCode(text: string): TemplateResult {
  const out: (TemplateResult | string)[] = [];
  const rx = /`([^`\n]+)`/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = rx.exec(text)) !== null) {
    if (m.index > last) out.push(text.slice(last, m.index));
    out.push(html`<code class="inline-code">${m[1] ?? ''}</code>`);
    last = rx.lastIndex;
  }
  if (last < text.length) out.push(text.slice(last));
  return html`${out}`;
}

/** The composed toggle event a digest row's expand/collapse affordance fires —
 *  it bubbles out of the shadow tree to `<chat-widget>`, which owns the Set of
 *  expanded message ids (the render fragments stay pure/stateless). */
export const MESSAGE_EXPAND_TOGGLE = 'message-expand-toggle';

/** Detail payload of a `MESSAGE_EXPAND_TOGGLE` event. */
export interface MessageExpandToggleDetail {
  readonly id: string;
}

function fireExpandToggle(e: Event, id: string): void {
  (e.currentTarget as HTMLElement).dispatchEvent(
    new CustomEvent<MessageExpandToggleDetail>(MESSAGE_EXPAND_TOGGLE, {
      detail: { id },
      bubbles: true,
      composed: true,
    }),
  );
}

/** The composed drag-start event a column resize handle fires — bubbles to
 *  `<chat-widget>`, which owns the `WorkspaceLayout` state and the pointer
 *  tracking (fragments stay stateless — the MESSAGE_EXPAND_TOGGLE pattern).
 *  Pointer events, so mouse and touch (iPad) drag through the same path. */
export const PANEL_RESIZE_START = 'panel-resize-start';

/** Detail payload of a `PANEL_RESIZE_START` event. */
export interface PanelResizeStartDetail {
  /** Which column the handle borders. */
  readonly panel: 'who' | 'context';
  /** The pointer's starting clientX — the drag delta's origin. */
  readonly startX: number;
  /** The pointer id, so the widget can track this drag exclusively. */
  readonly pointerId: number;
}

/** A column resize handle — a slim hit target between workspace columns. */
export function resizeHandle(panel: 'who' | 'context'): TemplateResult {
  const start = (e: PointerEvent): void => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).dispatchEvent(
      new CustomEvent<PanelResizeStartDetail>(PANEL_RESIZE_START, {
        detail: { panel, startX: e.clientX, pointerId: e.pointerId },
        bubbles: true,
        composed: true,
      }),
    );
  };
  return html`<div
    class="col-handle"
    data-panel=${panel}
    role="separator"
    aria-orientation="vertical"
    aria-label="resize ${panel === 'who' ? 'left rail' : 'context panel'}"
    @pointerdown=${start}
  ></div>`;
}

/** The message body at its display tier ([[perception-resolution-contract]]):
 *  a digested row renders head + mechanical tail line (+ repetition histogram)
 *  collapsed by default — no message floods the transcript — with the full
 *  original one toggle away. The widget stamps `expanded` per its own state. */
function messageBody(msg: MessageRowVM): TemplateResult {
  const digest = msg.digest;
  if (!digest) return html`<div class="content">${formatContent(msg.content)}</div>`;
  const toggle = (e: Event): void => {
    fireExpandToggle(e, msg.id);
  };
  // NOTE: `.content` is `white-space: pre-wrap`, so these templates must stay
  // whitespace-tight — pretty-printed newlines would render as literal blank lines.
  if (msg.expanded) {
    return html`<div class="content">${formatContent(msg.content)}<button class="digest-toggle" @click=${toggle}>collapse</button></div>`;
  }
  return html`<div class="content" data-collapsed>${formatContent(digest.head)}<div
      class="digest-tail" title="collapsed — mechanical summary of the hidden remainder">${digest.tailSummary}${digest.histogram
        ? html` <span class="digest-histogram">· ${digest.histogram}</span>`
        : nothing}</div><button class="digest-toggle" @click=${toggle}>show full message</button></div>`;
}

/** One conversation row — WHAT was said. */
export function messageRow(msg: MessageRowVM): TemplateResult {
  return html`
    <li class="msg" data-kind=${msg.kind} data-sender=${msg.senderId}>
      <span class="msg-glyph">${kindGlyph(msg.kind)}</span>
      <div class="msg-body">
        <div class="msg-head">
          <span class="sender">${msg.senderName}</span>
          ${runtimeBadge(msg.runtime)}
          <span class="time">${msg.time}</span>
        </div>
        ${messageBody(msg)}
      </div>
    </li>
  `;
}
