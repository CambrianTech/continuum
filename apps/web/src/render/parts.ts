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
import { HERE_NOW_LISTING_ID, ROSTER_LISTING_ID, WORKING_NOW_LISTING_ID } from '@continuum/patterns';
import type { GaugeView, ListingCell, ListingView, MetricsView } from '@continuum/patterns';
import type {
  ActGroupVM,
  LoadoutVM,
  MemberKind,
  MessageRowVM,
  RosterMemberVM,
} from '@continuum/chat-view';

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
  // The FORM CURVE: an optional 0..=100 series drawn as a thin polyline with
  // an emphasized endpoint — "learning and improving" as a line, not prose
  // (Joel: growth the enthusiasts can SEE). Absent spark = stats only.
  const spark =
    view.spark !== undefined && view.spark.length > 1
      ? (() => {
          const pts = view.spark ?? [];
          const w = 72;
          const step = w / (pts.length - 1);
          const y = (v: number): number => 14 - (Math.max(0, Math.min(100, v)) * 12) / 100;
          const path = pts.map((v, i) => `${i === 0 ? 'M' : 'L'} ${(i * step).toFixed(1)} ${y(v).toFixed(1)}`).join(' ');
          const last = pts[pts.length - 1] ?? 0;
          return html`<svg class="metrics-spark" viewBox="0 0 72 16" aria-hidden="true">
            <path d=${path} />
            <circle cx=${w} cy=${y(last).toFixed(1)} r="1.6" />
          </svg>`;
        })()
      : nothing;
  return html`<div class="metrics-row">
    ${view.stats.map(
      (s) => html`<span class="metric" data-tone=${s.tone ?? 'muted'}>
        <span class="metric-val">${s.value}</span>
        <span class="metric-label">${s.label}</span>
      </span>`,
    )}
    ${spark}
  </div>`;
}

/** The composed select event a listing cell fires when the reader picks it — the
 *  `select(entityInList)` NavIntent reaching the web idiom. It bubbles out of the
 *  shadow tree to `<chat-widget>` (same pattern as `MESSAGE_EXPAND_TOGGLE`): the
 *  render fragments stay pure/stateless, the host owns what a selection DOES. */
export const LISTING_SELECT = 'listing-select';

/** Composed event: toggle the live face's MIC capture (the CallClient). */
export const LIVE_MIC_TOGGLE = 'live-mic-toggle';
export function fireLiveMicToggle(e: Event): void {
  (e.currentTarget as HTMLElement).dispatchEvent(
    new CustomEvent(LIVE_MIC_TOGGLE, { bubbles: true, composed: true }),
  );
}

/** Detail payload of a `LISTING_SELECT` event — which listing, which cell, and
 *  (when the cell carries one) its neutral `group` key: the nav tab's target
 *  kind for rooms-rail cells. The routing rule reads it to pick the select's
 *  activity kind — never a second vocabulary. */
export interface ListingSelectDetail {
  readonly listingId: string;
  readonly id: string;
  readonly group?: string;
  /** Which ELEMENT of the cell was picked (card 95844639): a tile's compass →
   *  `'brain'`, its genome block → `'genome'`. Absent = the cell itself. The
   *  routing rule maps it to the route's `anchor` — same destination activity,
   *  scrolled to the element's section. */
  readonly element?: string;
}

export function fireListingSelect(
  e: Event,
  listingId: string,
  id: string,
  group?: string,
  element?: string,
): void {
  (e.currentTarget as HTMLElement).dispatchEvent(
    new CustomEvent<ListingSelectDetail>(LISTING_SELECT, {
      detail: {
        listingId,
        id,
        ...(group !== undefined ? { group } : {}),
        ...(element !== undefined ? { element } : {}),
      },
      bubbles: true,
      composed: true,
    }),
  );
}

/** A routed listing selection — the `nav/select` verb's (target, kind) pair the
 *  host dispatches. `kind` is the wire `NavTargetKind` string. `anchor` is
 *  CLIENT-side presentation (scroll the destination to a section) — it never
 *  rides the wire; nav semantics stay (target, kind) pure. */
export interface NavSelectRoute {
  readonly target: string;
  readonly kind: 'room' | 'persona';
  readonly anchor?: string;
}

/** Tile elements that anchor into the persona HOME — the sections
 *  `renderPersona` gives ids to. A whitelist so an unknown element degrades to
 *  the plain persona select, never a dead scroll target. */
const PERSONA_ANCHOR_ELEMENTS: ReadonlySet<string> = new Set(['brain', 'genome']);

/** THE select-routing rule (`select(entityInList)` → NavIntent,
 *  NAVIGATION-ACROSS-MODALITIES.md §2), pure and DOM-free so it is unit-tested
 *  without a browser:
 *    - a ROOMS-rail / tab-bar pick routes by the cell's group (the tab's target
 *      kind): `persona` → a persona-kind select (the persona HOME tab),
 *      `content` → inert until content tabs route, anything else → a room switch;
 *    - a ROSTER pick (a citizen's name/avatar) IS the persona select — the
 *      profile is the persona's home activity, same verb, persona kind;
 *    - every other listing's select is not a navigation (`null`). */
export function navSelectTarget(detail: ListingSelectDetail): NavSelectRoute | null {
  if (detail.listingId === 'rooms') {
    if (detail.group === 'persona') return { target: detail.id, kind: 'persona' };
    if (detail.group === 'content') return null;
    return { target: detail.id, kind: 'room' };
  }
  // The profile rail's ACTIVE WORK cells are doors: a pick with a room-kind
  // group navigates to that run's room (same verb as a tab click).
  if (detail.listingId === 'p-active-work') {
    if (detail.group === 'room') return { target: detail.id, kind: 'room' };
    return null;
  }
  if (detail.listingId === 'roster') {
    const anchor =
      detail.element !== undefined && PERSONA_ANCHOR_ELEMENTS.has(detail.element)
        ? detail.element
        : undefined;
    return { target: detail.id, kind: 'persona', ...(anchor !== undefined ? { anchor } : {}) };
  }
  // The chat-room rail's people doors (every-name-is-a-door): a here-now pick
  // opens that member's persona home; a working-now pick stands in the run's
  // solve room (cells carry `group: 'room'` and the solve room's UUID).
  if (detail.listingId === HERE_NOW_LISTING_ID) {
    return { target: detail.id, kind: 'persona' };
  }
  if (detail.listingId === WORKING_NOW_LISTING_ID) {
    if (detail.group === 'room') return { target: detail.id, kind: 'room' };
    return null;
  }
  return null;
}

/** Which listing selections mean "switch room" — the room-only slice of
 *  [`navSelectTarget`], kept for callers/tests that only care about rooms. */
export function roomSelectTarget(detail: ListingSelectDetail): string | null {
  const route = navSelectTarget(detail);
  return route?.kind === 'room' ? route.target : null;
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
          fireListingSelect(e, selectFrom, cell.id, cell.group);
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
      data-nested=${cell.parent !== undefined ? '' : nothing}
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
    return html`<ul class="roster">${view.cells.map((c) => memberCardFromCell(c, view.id))}</ul>`;
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
export function cognitionDiamond(
  v: Readonly<Record<string, number>>,
  onSelect?: (e: Event) => void,
): TemplateResult {
  // FOUR distinct triangles pointing out like a compass (N=Focus, E=Reason, S=Recall,
  // W=Act), a gap in the centre — so it reads as four, and a strong faculty burns bright
  // while a dim one nearly vanishes (the SHAPE is the mind). No more solid blob.
  const lit = (k: string): number => 0.14 + 0.86 * (Math.max(0, Math.min(100, v[k] ?? 0)) / 100);
  const pct = (k: string): number => Math.round(Math.max(0, Math.min(100, v[k] ?? 0)));
  // Each faculty its own hue (color > monochrome) — readable by colour AND position.
  const tri = (pts: string, k: string, label: string, color: string): TemplateResult =>
    svg`<polygon points="${pts}" class="cog-tri" style="fill:${color};opacity:${lit(k)}"><title>${label} ${pct(k)}</title></polygon>`;
  // With a select handler the compass is its OWN click target (→ the brain HUD)
  // nested inside the tile's whole-card select: stop propagation so one click
  // means one navigation (card 95844639).
  const pick =
    onSelect === undefined
      ? undefined
      : (e: Event): void => {
          e.stopPropagation();
          onSelect(e);
        };
  return html`<svg
    viewBox="0 0 40 40"
    class="cog-diamond ${pick ? 'element-link' : ''}"
    aria-label="cognition"
    role=${pick ? 'button' : nothing}
    @click=${pick ?? nothing}
  >
    ${pick ? svg`<title>Open brain HUD</title>` : nothing}
    ${tri('20,2 12,15 28,15', 'focus', 'Focus', 'var(--faculty-focus)')}
    ${tri('38,20 25,12 25,28', 'reason', 'Reason', 'var(--faculty-reason)')}
    ${tri('20,38 12,25 28,25', 'recall', 'Recall', 'var(--faculty-recall)')}
    ${tri('2,20 15,12 15,28', 'act', 'Act', 'var(--faculty-act)')}
  </svg>`;
}

/** Genome slot count — TWO rows of four (the legacy tile's four equipment slots,
 *  doubled). The loadout is heading past four as skills go per-domain and expert
 *  granularity (#226) lands; eight visible slots read the growth honestly while
 *  staying a fixed HUD footprint. Personas with more genes keep the top-8 lit;
 *  the panel tooltip names the overflow count. */
const GENOME_SLOTS = 8;

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
  onElement?: (e: Event, element: 'brain' | 'genome') => void,
): TemplateResult {
  const lit = litGeneCount(v, genes);
  const title =
    lit > GENOME_SLOTS
      ? `GENOME — ${lit} genes loaded (top ${GENOME_SLOTS} shown)`
      : `GENOME — ${lit} gene${lit === 1 ? '' : 's'} loaded`;
  // Element navigation (card 95844639): the slots block opens the genome shelf,
  // the compass opens the brain HUD — each stops propagation so the tile's
  // whole-card select doesn't also fire.
  const pickGenome =
    onElement === undefined
      ? undefined
      : (e: Event): void => {
          e.stopPropagation();
          onElement(e, 'genome');
        };
  return html`<span class="genome-panel" title=${title}>
    <span class="genome-label">GENOME</span>
    <span
      class="genome-slots ${pickGenome ? 'element-link' : ''}"
      role=${pickGenome ? 'button' : nothing}
      title=${pickGenome ? 'Open genome shelf' : nothing}
      @click=${pickGenome ?? nothing}
    >
      ${Array.from({ length: GENOME_SLOTS }, (_, i) => {
        const isLit = i < lit;
        const name = genes[i];
        return html`<span
          class="genome-slot ${isLit ? 'lit' : ''}"
          title=${name ?? (isLit ? 'loaded gene' : 'empty slot')}
        ></span>`;
      })}
    </span>
    ${cognitionDiamond(v, onElement === undefined ? undefined : (e) => onElement(e, 'brain'))}
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

/** A micro-SPEEDOMETER — a 22x12 arc gauge with a needle, the density Joel
 *  spec'd ("speedometers not massive text; thin lines, no more real estate").
 *  Normalized 0..=100 sweeps the needle -90deg..+90deg; the arc is a single
 *  stroked path, the needle a 1px line — two DOM nodes, zero animation cost
 *  at rest (the needle eases via a CSS transform transition). */
function speedo(key: string, label: string, val: number): TemplateResult {
  const clamped = Math.max(0, Math.min(100, val));
  const angle = -90 + (clamped * 180) / 100;
  return html`<span class="speedo" data-key=${key} title="${label} ${clamped}%">
    <svg viewBox="0 0 22 12" class="speedo-svg" aria-hidden="true">
      <path class="speedo-arc" d="M 2 11 A 9 9 0 0 1 20 11" />
      <line
        class="speedo-needle"
        x1="11" y1="11" x2="11" y2="3.2"
        transform="rotate(${angle} 11 11)"
      />
    </svg>
    <span class="speedo-label">${label}</span>
  </span>`;
}

/** The meter stack — label · track · value per row, the info-dense heart of the
 *  glass-box tile, each row hoverable ([[persona-tile-is-a-live-game-hud]]). Fill
 *  hues key off `data-key` in CSS (named theme tokens, one per vital). */
export function personaReadout(v: Readonly<Record<string, number>>): TemplateResult | typeof nothing {
  const stats = STAT_ORDER.filter(([k]) => v[k] !== undefined);
  // The SPEEDLINE: decode + prefill needles when the speed pulse is fresh —
  // absent keys draw nothing (a stale needle is a lie the radiator refuses).
  const needles = [
    ['tps', 'T/S'] as const,
    ['pfx', 'PFX'] as const,
  ].filter(([k]) => v[k] !== undefined);
  if (stats.length === 0 && needles.length === 0) return nothing;
  return html`<span class="meters">
    ${needles.length > 0
      ? html`<span class="speedline">
          ${needles.map(([k, label]) => speedo(k, label, v[k] ?? 0))}
        </span>`
      : nothing}
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
/** A mind with vitals wired but every cognition pulse dark AND no live stream:
 *  resting/not-currently-able-to-engage. The row DIMS slightly (Joel 2026-07-30:
 *  'dim the entire row slightly in the user list') — the afterglow (~17s decay)
 *  keeps recently-active minds bright, so brightness reads as recency of thought.
 *  Members without vitals (plain agents/humans) never dim on this signal. */
function isDormant(v: Readonly<Record<string, number>>): boolean {
  if (Object.keys(v).length === 0) return false;
  const pulse =
    (v.focus ?? 0) + (v.reason ?? 0) + (v.recall ?? 0) + (v.act ?? 0) + (v.speaking ?? 0);
  return pulse === 0;
}

/** Hover text for a roster row: the citizen's identity card when one is
 *  published (name · pronouns · role, then the bio line), else the plain
 *  open-affordance label. Native `title` keeps this zero-cost until the
 *  richer hover-card component lands (#245). */
export function memberTooltip(m: RosterMemberVM): string {
  if (!m.bio && !m.pronouns && !m.roleLabel) return `Open ${m.name}`;
  const head = [m.name, m.pronouns, m.roleLabel].filter(Boolean).join(' · ');
  return m.bio ? `${head}\n${m.bio}` : head;
}

/** Cell-flavored sibling of [`memberTooltip`] — the same identity-card hover
 *  built from the neutral `ListingCell` (subtitle = pronouns · role, detail =
 *  bio), for the rail that renders cells rather than the rich view-model. */
export function cellTooltip(cell: ListingCell): string {
  if (!cell.detail && !cell.subtitle) return `Open ${cell.title}`;
  const head = cell.subtitle ? `${cell.title} · ${cell.subtitle}` : cell.title;
  return cell.detail ? `${head}\n${cell.detail}` : head;
}

export function memberCard(m: RosterMemberVM): TemplateResult {
  const hasVitals = Object.keys(m.vitals).length > 0;
  return html`
    <li class="member clickable ${m.active ? 'online' : 'idle'} ${isDormant(m.vitals) ? 'dormant' : ''}" data-kind=${m.kind} tabindex="0"
        title=${memberTooltip(m)}>
      ${agoStamp(m.lastSeenMs)}
      <span class="avatar" data-state=${avatarState(m.vitals, m.active)}>
        <span class="glyph">${kindGlyph(m.kind)}</span>
        ${avatarImage(m.avatarUrl)}
        <span class="status-dot"></span>
        ${emojiOverlay(m.vitals)}
      </span>
      <span class="info">
        <span class="name">${m.name}</span>
        <span class="idline">
          <span class="meta">
            <span class="kind-badge">${kindLabel(m.kind)}</span>
            ${runtimeBadge(m.runtime)}
          </span>
          ${loadoutStrip(m.loadout)}
        </span>
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
export function memberCardFromCell(cell: ListingCell, listingId?: string): TemplateResult {
  const active = cell.status === 'active';
  const kind = cell.badges?.[0] ?? 'agent';
  const runtime = cell.badges?.[1] ?? '';
  // A tile pick IS the persona select (`select(entityInList)` → nav/select with
  // a persona-kind target — the citizen's HOME tab). Fires the SAME composed
  // LISTING_SELECT seam the rooms rail uses; the host owns the dispatch.
  const select =
    listingId === undefined
      ? undefined
      : (e: Event): void => {
          fireListingSelect(e, listingId, cell.id, cell.group);
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
  // Element navigation (card 95844639): compass → brain HUD, genome block →
  // genome shelf. Same LISTING_SELECT seam, `element` rides the detail and the
  // routing rule turns it into the persona route's anchor.
  const selectElement =
    listingId === undefined
      ? undefined
      : (e: Event, element: 'brain' | 'genome'): void => {
          fireListingSelect(e, listingId, cell.id, cell.group, element);
        };
  return html`
    <li class="member clickable ${active ? 'online' : 'idle'} ${isDormant(cell.meters ?? {}) ? 'dormant' : ''}" data-kind=${kind} tabindex="0"
        title=${cellTooltip(cell)} @click=${select ?? nothing} @keydown=${keySelect ?? nothing}>
      ${agoStamp(cell.lastActiveMs)}
      <span class="avatar" data-state=${avatarState(cell.meters ?? {}, active)}>
        <span class="glyph">${cell.glyph ?? ''}</span>
        ${avatarImage(cell.image)}
        <span class="status-dot"></span>
        ${emojiOverlay(cell.meters ?? {})}
      </span>
      <span class="info">
        <span class="name">${cell.title}</span>
        <span class="idline">
          <span class="meta">
            <span class="kind-badge">${kind}</span>
            ${runtimeBadge(runtime)}
          </span>
          ${loadoutStrip(cell.loadout)}
        </span>
        ${cell.meters ? personaReadout(cell.meters) : nothing}
      </span>
      ${cell.meters ? genomePanel(cell.meters, cell.genes ?? [], selectElement) : nothing}
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
    parts.push(codeBlock(lang, code));
    last = fence.lastIndex;
  }
  if (last < text.length) parts.push(inlineCode(text.slice(last)));
  return html`${parts}`;
}

/** Code is SHOWN, not hidden: blocks up to this many lines render fully open
 *  (the old n<=3 collapse hid a 4-line snippet behind a "▸ RUST" bar — backwards). */
const CODE_OPEN_ALL_LINES = 40;
/** Above the open-all threshold: this many head lines stay visible, the rest sits
 *  behind a "+K more lines" expander — same show-the-start policy every consumer
 *  (human px, persona/Claude tokens) applies at its own budget. */
const CODE_HEAD_LINES = 25;

/** One highlighted, line-numbered run of code. `startLine` lets the expanded
 *  remainder continue the gutter where the head stopped. NOTE: the template is
 *  deliberately whitespace-TIGHT — `.content` is `white-space: pre-wrap`, so any
 *  pretty-printed newline between tags renders as a phantom blank line (the
 *  giant-empty-padding bug, glass-boxed 2026-07-30). */
function codeChunk(code: string, language: string | null, startLine: number): TemplateResult {
  // hljs escapes the code, so the resulting HTML is inert for unsafeHTML.
  const value =
    language && hljs.getLanguage(language)
      ? hljs.highlight(code, { language, ignoreIllegals: true }).value
      : hljs.highlightAuto(code).value;
  const n = code.length === 0 ? 0 : code.split('\n').length;
  const gutter = Array.from({ length: n }, (_, i) => String(startLine + i)).join('\n');
  return html`<div class="code-body"><div class="code-gutter" aria-hidden="true">${gutter}</div><pre><code class="hljs">${unsafeHTML(value)}</code></pre></div>`;
}

/** A fenced block as a code card: header (language + line count), line-numbered
 *  body. Small blocks render whole; big ones show the head with a "+K more lines"
 *  expander. Language for highlighting: the fence tag when hljs knows it, else
 *  ONE auto-detection over the full text reused for both chunks (so head and
 *  remainder never highlight as different languages). */
function codeBlock(fenceLang: string, code: string): TemplateResult {
  const n = code.length === 0 ? 0 : code.split('\n').length;
  const language =
    fenceLang && hljs.getLanguage(fenceLang)
      ? fenceLang
      : (hljs.highlightAuto(code).language ?? null);
  const label = fenceLang || language || 'code';
  const header = html`<summary>${label}<span class="code-count">${n} ${n === 1 ? 'line' : 'lines'}</span></summary>`;
  if (n <= CODE_OPEN_ALL_LINES) {
    return html`<details class="code-collapsible" open>${header}${codeChunk(code, language, 1)}</details>`;
  }
  const lines = code.split('\n');
  const head = lines.slice(0, CODE_HEAD_LINES).join('\n');
  const rest = lines.slice(CODE_HEAD_LINES).join('\n');
  return html`<details class="code-collapsible" open>${header}${codeChunk(head, language, 1)}<details class="code-more"><summary>+${n - CODE_HEAD_LINES} more lines</summary>${codeChunk(rest, language, CODE_HEAD_LINES + 1)}</details></details>`;
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

/** The composed toggle event the live-face affordances fire — the header's
 *  Go-live button opens the room's call face, the call bar's hang-up closes it
 *  (returning to the chat face). Bubbles to `<chat-widget>`, which owns the
 *  face state — fragments stay pure/stateless (the MESSAGE_EXPAND_TOGGLE
 *  pattern). The face itself is renderer state until room recipes declare
 *  purpose "live" ([[room-purpose-is-per-recipe-not-an-enum]] follow-up). */
export const LIVE_FACE_TOGGLE = 'live-face-toggle';

/** Detail payload of a `LIVE_FACE_TOGGLE` event. */
export interface LiveFaceToggleDetail {
  /** true = open the live face; false = hang up (back to the chat face). */
  readonly open: boolean;
}

export function fireLiveFaceToggle(e: Event, open: boolean): void {
  (e.currentTarget as HTMLElement).dispatchEvent(
    new CustomEvent<LiveFaceToggleDetail>(LIVE_FACE_TOGGLE, {
      detail: { open },
      bubbles: true,
      composed: true,
    }),
  );
}

/** The composed event a tab's × fires — close that open activity. Bubbles to
 *  `<chat-widget>`, which dispatches the injected `nav/close` handler; the tab
 *  disappears when the re-projected nav envelope streams back — substrate
 *  truth only, no optimistic local removal (the LISTING_SELECT discipline). */
export const NAV_TAB_CLOSE = 'nav-tab-close';

/** Detail payload of a `NAV_TAB_CLOSE` event. */
export interface NavTabCloseDetail {
  /** The tab's target ref (the open activity being closed). */
  readonly target: string;
}

export function fireNavTabClose(e: Event, target: string): void {
  (e.currentTarget as HTMLElement).dispatchEvent(
    new CustomEvent<NavTabCloseDetail>(NAV_TAB_CLOSE, {
      detail: { target },
      bubbles: true,
      composed: true,
    }),
  );
}

/** The composed toggle event the call bar's CC button fires — flips the live
 *  caption strip (a REAL control: the strip is the streaming transcript line). */
export const LIVE_CAPTIONS_TOGGLE = 'live-captions-toggle';

export function fireLiveCaptionsToggle(e: Event): void {
  (e.currentTarget as HTMLElement).dispatchEvent(
    new CustomEvent(LIVE_CAPTIONS_TOGGLE, { bubbles: true, composed: true }),
  );
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

/** A collapsed run of tool acts — the transcript's RECEIPT row (#243, the
 *  Claude-iOS pattern). Renders as a native `<details>` disclosure: the
 *  collapsed line reads "⚙ Asha · Read 2 files, ran a command ›" and expands
 *  IN PLACE (Joel's law: web expands inline; mobile opens a sheet) to one
 *  line per act — status mark, tool name, object. No JS state — the browser
 *  owns the open/closed bit, so a re-render never fights the reader. */
export function actGroupRow(group: ActGroupVM): TemplateResult {
  // The actor's name is a DOOR to her page — every name everywhere navigates
  // (pages = rooms = activities = content; Joel, 2026-08-31). stopPropagation
  // so opening a profile never also toggles the receipt disclosure.
  const openActor = (e: Event): void => {
    e.stopPropagation();
    e.preventDefault();
    fireListingSelect(e, ROSTER_LISTING_ID, group.actorId);
  };
  return html`
    <li class="act-group" data-actor=${group.actorId} title=${group.time}>
      <details>
        <summary>
          <span class="act-gear${group.anyFailed ? ' act-failed' : ''}">⚙</span>
          <span
            class="act-actor element-link"
            role="button"
            tabindex="0"
            title="Open ${group.actorName}'s profile"
            @click=${openActor}
            @keydown=${(e: KeyboardEvent): void => {
              if (e.key === 'Enter') openActor(e);
            }}
            >${group.actorName}</span
          >
          <span class="act-line">${group.summaryLine}</span>
          <span class="act-count">${group.receipts.length}</span>
        </summary>
        <ul class="act-list">
          ${group.receipts.map(
            (r) => html`<li class="act-item${r.ok ? '' : ' act-failed'}">
              <span class="act-mark">${r.ok ? '✓' : '✗'}</span>
              <span class="act-tool">${r.tool}</span>
              ${r.summary ? html`<span class="act-obj">${r.summary}</span>` : nothing}
              <span class="act-time">${r.time}</span>
            </li>`,
          )}
        </ul>
      </details>
    </li>
  `;
}

/** One conversation row — WHAT was said. */
export function messageRow(msg: MessageRowVM): TemplateResult {
  // A continuation row (same sender, close in time — projected upstream) drops
  // the avatar + head and hangs the body in the same column: readable runs
  // instead of bubble-per-line sprawl. Time surfaces on hover via title.
  if (msg.continues) {
    return html`
      <li class="msg continues" data-kind=${msg.kind} data-sender=${msg.senderId} title=${msg.time}>
        <div class="msg-body">${messageBody(msg)}</div>
      </li>
    `;
  }
  // The sender's NAME is a live link to their profile — same composed
  // LISTING_SELECT the roster tiles fire (listing 'roster' → the routing rule's
  // persona select), so a name click opens that citizen's home tab through the
  // one nav verb, never a parallel route.
  const openProfile = (e: Event): void => {
    e.stopPropagation();
    fireListingSelect(e, ROSTER_LISTING_ID, msg.senderId);
  };
  const keyProfile = (e: KeyboardEvent): void => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      openProfile(e);
    }
  };
  return html`
    <li class="msg" data-kind=${msg.kind} data-sender=${msg.senderId}>
      <span class="msg-glyph">
        ${msg.senderAvatarUrl
          ? html`<img class="msg-avatar" src=${msg.senderAvatarUrl} alt="" />`
          : kindGlyph(msg.kind)}
      </span>
      <div class="msg-body">
        <div class="msg-head">
          <span
            class="sender element-link"
            role="button"
            tabindex="0"
            title="Open ${msg.senderName}'s profile"
            @click=${openProfile}
            @keydown=${keyProfile}
            >${msg.senderName}</span
          >
          ${runtimeBadge(msg.runtime)}
          <span class="time">${msg.time}</span>
        </div>
        ${messageBody(msg)}
      </div>
    </li>
  `;
}

/** The composed event the header's Settings affordance fires — open/close the
 *  operator panel. The widget owns the face state (mirrors LIVE_FACE_TOGGLE). */
export const SETTINGS_FACE_TOGGLE = 'settings-face-toggle';

export interface SettingsFaceToggleDetail {
  readonly open: boolean;
}

export function fireSettingsFaceToggle(e: Event, open: boolean): void {
  (e.currentTarget as HTMLElement).dispatchEvent(
    new CustomEvent<SettingsFaceToggleDetail>(SETTINGS_FACE_TOGGLE, {
      detail: { open },
      bubbles: true,
      composed: true,
    }),
  );
}

/** The composed event the settings face's covenant buttons fire — accept
 *  (true) or revoke (false). The widget routes it through the SAME
 *  `genome/sharing` verb the terminal uses; the face re-renders from the
 *  refetched substrate truth, never from optimistic local state. */
export const SETTINGS_AGREE = 'settings-agree';

export interface SettingsAgreeDetail {
  readonly agree: boolean;
}

export function fireSettingsAgree(e: Event, agree: boolean): void {
  (e.currentTarget as HTMLElement).dispatchEvent(
    new CustomEvent<SettingsAgreeDetail>(SETTINGS_AGREE, {
      detail: { agree },
      bubbles: true,
      composed: true,
    }),
  );
}
