/**
 * Shared web render parts — the small Lit fragments the workspace renderers reuse.
 *
 * Extracted so both the roster/`Listing` rendering (left panel) and the per-purpose
 * `Content` renderers (center, via the Content registry) draw the SAME member/message
 * fragments without duplicating them ([[compression]]). No state, no substrate — pure
 * field→element maps, styled entirely from the design tokens.
 */

import { html, nothing, type TemplateResult } from 'lit';
import type { ListingCell } from '@continuum/patterns';
import type { MemberKind, MessageRowVM, RosterMemberVM } from '@continuum/chat-view';

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
      ${cell.badges && cell.badges.length > 0
        ? html`<span class="cell-badges">
            ${cell.badges.map((b) => html`<span class="cell-badge">${b}</span>`)}
          </span>`
        : nothing}
    </li>
  `;
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

/** Short 3-letter tag for a vital key — the old persona-tile's INT/NRG/QUE label. */
function vitalTag(key: string): string {
  return key.slice(0, 3).toUpperCase();
}

/** The live genome-energy meters — one thin glowing bar per vital the persona
 *  surfaces (energy/attention/compute). Renders nothing when the member reports
 *  no vitals (a human, a remote peer) — no fabricated bars. This is the readout
 *  that makes a persona feel *alive* in the roster (the PX target). */
function vitalsMeters(vitals: Record<string, number>): TemplateResult | typeof nothing {
  const entries = Object.entries(vitals);
  if (entries.length === 0) return nothing;
  return html`
    <span class="vitals">
      ${entries.map(([key, pct]) => {
        const clamped = Math.max(0, Math.min(100, pct));
        return html`
          <span class="vital" title="${key} ${pct}%">
            <span class="vital-label">${vitalTag(key)}</span>
            <span class="vital-track">
              <span class="vital-fill" style="width:${clamped}%"></span>
            </span>
            <span class="vital-value">${Math.round(clamped)}</span>
          </span>
        `;
      })}
    </span>
  `;
}

/** One member card — avatar + presence dot, name, kind/runtime, live vitals —
 *  the old Users & Agents persona-tile as the `Listing` cell (INTERFACE-PORT-MAP.md). */
export function memberCard(m: RosterMemberVM): TemplateResult {
  return html`
    <li class="member ${m.active ? 'online' : 'idle'}" data-kind=${m.kind}>
      <span class="avatar">
        <span class="glyph">${kindGlyph(m.kind)}</span>
        <span class="status-dot" title=${m.active ? 'active' : 'idle'}></span>
      </span>
      <span class="info">
        <span class="name">${m.name}</span>
        <span class="meta">
          <span class="kind-badge">${kindLabel(m.kind)}</span>
          ${runtimeBadge(m.runtime)}
        </span>
        ${vitalsMeters(m.vitals)}
      </span>
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
    <li class="member ${active ? 'online' : 'idle'}" data-kind=${kind}>
      <span class="avatar">
        <span class="glyph">${cell.glyph ?? ''}</span>
        <span class="status-dot" title=${active ? 'active' : 'idle'}></span>
      </span>
      <span class="info">
        <span class="name">${cell.title}</span>
        <span class="meta">
          <span class="kind-badge">${kind}</span>
          ${runtimeBadge(runtime)}
        </span>
        ${cell.meters ? vitalsMeters(cell.meters) : nothing}
      </span>
    </li>
  `;
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
        <div class="content">${msg.content}</div>
      </div>
    </li>
  `;
}
