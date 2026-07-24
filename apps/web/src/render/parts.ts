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
import type { ListingCell, ListingView } from '@continuum/patterns';
import type { LoadoutVM, MemberKind, MessageRowVM, RosterMemberVM } from '@continuum/chat-view';

/** GENERIC listing-cell renderer — the first real positron web *component*: it draws
 *  ANY already-projected `ListingCell` (a foundry model, a room, a cohort) the same
 *  way, on any target. The roster's rich member card stays bespoke while it carries
 *  live vitals meters; every other Listing routes through this. Two consumers
 *  (foundry now, more later) is exactly what earns the extraction — outliers first,
 *  then the component ([[positron-is-a-framework-not-vanilla-pages]]). */
export function listingCell(cell: ListingCell): TemplateResult {
  return html`
    <li class="cell" data-status=${cell.status ?? 'none'}>
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
 *  draw the SAME rows without duplication ([[compression]]). */
export function renderListing(view: ListingView): TemplateResult {
  if (view.id === 'roster') {
    return html`<ul class="roster">${view.cells.map(memberCardFromCell)}</ul>`;
  }
  return html`<ul class="cells">${view.cells.map(listingCell)}</ul>`;
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
 *  reads. Drives the ring colour + pulse. Real thinking/error events are the wiring
 *  (persona:vitals emitting `error`/inference state); today derived from activity/reason
 *  (thinking) + presence (active/idle). */
export function avatarState(v: Readonly<Record<string, number>>, active: boolean): string {
  if ((v.error ?? 0) > 0) return 'error';
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


/** The four faculties the cognition diamond reads — Focus / Reason / Recall / Act. */
const COGNITION = ['focus', 'reason', 'recall', 'act'] as const;

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
    ${tri('20,2 12,15 28,15', 'focus', 'Focus', '#00d4ff')}
    ${tri('38,20 25,12 25,28', 'reason', 'Reason', '#ffb020')}
    ${tri('20,38 12,25 28,25', 'recall', 'Recall', '#3fb950')}
    ${tri('2,20 15,12 15,28', 'act', 'Act', '#ff6a3d')}
  </svg>`;
}

/** Genome bars — the persona's loaded LoRA genes as filled segments. Nothing when running
 *  the base model (honest — not a fabricated 0% bar). */
export function genomeBlock(v: Readonly<Record<string, number>>): TemplateResult | typeof nothing {
  const g = v.genome;
  if (g === undefined || g <= 0) return nothing;
  const filled = Math.max(1, Math.round((Math.min(100, g) / 100) * 6));
  return html`<span class="genome" title="genome ${Math.round(g)}%">
    ${Array.from(
      { length: 6 },
      (_, i) => html`<span class="gene ${i < filled ? 'on' : ''} ${i % 3 === 2 ? 'hot' : ''}"></span>`,
    )}
  </span>`;
}

/** The rich per-persona readout: cognition diamond + genome bars + the tempo/other meters.
 *  Each part appears only when its data is present — a persona with no cognition emitted yet
 *  simply shows its activity, honestly. */
/** The engine meters shown as bars — the cognition faculties are the DIAMOND, not bars
 *  (showing them both ways made the tile tall + redundant). Just speed + size here. */
const STAT_ORDER: readonly (readonly [string, string])[] = [
  ['speed', 'SPD'],
  ['size', 'PAR'],
];

/** The dense meter grid — the info-packed heart of the glass-box tile. Tons of tiny
 *  labelled value-meters close together, each hoverable ([[persona-tile-is-a-live-game-hud]]).
 *  Plus the cognition diamond (compact personality glyph) + genome bars alongside. */
export function personaReadout(v: Readonly<Record<string, number>>): TemplateResult | typeof nothing {
  const stats = STAT_ORDER.filter(([k]) => v[k] !== undefined);
  if (stats.length === 0) return nothing;
  return html`<span class="stat-grid">
    ${stats.map(([k, label]) => {
      const val = Math.round(Math.max(0, Math.min(100, v[k] ?? 0)));
      return html`<span class="stat" data-key=${k} title="${label} ${val}">
        <span class="stat-label">${label}</span>
        <span class="stat-bar"><span class="stat-fill" style="width:${val}%"></span></span>
        <span class="stat-val">${val}</span>
      </span>`;
    })}
  </span>`;
}

/** The RIGHT pane of the glass-box tile — the cognition diamond (personality glyph) + genome
 *  bars. Compact, ~one avatar tall, pushed to the right edge. */
export function cognitionCluster(v: Readonly<Record<string, number>>): TemplateResult | typeof nothing {
  const hasCog = COGNITION.some((k) => (v[k] ?? 0) > 0);
  const genome = genomeBlock(v);
  if (!hasCog && genome === nothing) return nothing;
  return html`<span class="cog-cluster">${hasCog ? cognitionDiamond(v) : nothing}${genome}</span>`;
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

/** One member card — avatar + presence dot, name, kind/runtime, live vitals —
 *  the old Users & Agents persona-tile as the `Listing` cell (INTERFACE-PORT-MAP.md). */
export function memberCard(m: RosterMemberVM): TemplateResult {
  return html`
    <li class="member clickable ${m.active ? 'online' : 'idle'}" data-kind=${m.kind} tabindex="0"
        title="Open ${m.name}">
      <span class="avatar" data-state=${avatarState(m.vitals, m.active)}>
        <span class="glyph">${kindGlyph(m.kind)}</span>
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
      ${cognitionCluster(m.vitals)}
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
      <span class="avatar" data-state=${avatarState(cell.meters ?? {}, active)}>
        <span class="glyph">${cell.glyph ?? ''}</span>
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
      ${cell.meters ? cognitionCluster(cell.meters) : nothing}
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

/** The message body at its display tier ([[perception-resolution-contract]]):
 *  a digested row renders head + mechanical tail line (+ repetition histogram)
 *  collapsed by default — no message floods the transcript — with the full
 *  original one toggle away. The widget stamps `expanded` per its own state. */
function messageBody(msg: MessageRowVM): TemplateResult {
  const digest = msg.digest;
  if (!digest) return html`<div class="content">${formatContent(msg.content)}</div>`;
  const toggle = (e: Event): void => fireExpandToggle(e, msg.id);
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
