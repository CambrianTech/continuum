/**
 * Shared web render parts — the small Lit fragments the workspace renderers reuse.
 *
 * Extracted so both the roster/`Listing` rendering (left panel) and the per-purpose
 * `Content` renderers (center, via the Content registry) draw the SAME member/message
 * fragments without duplicating them ([[compression]]). No state, no substrate — pure
 * field→element maps, styled entirely from the design tokens.
 */

import { html, nothing, type TemplateResult } from 'lit';
import type { MemberKind, MessageRowVM, RosterMemberVM } from '@continuum/chat-view';

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
      ${entries.map(
        ([key, pct]) => html`
          <span class="vital" title="${key} ${pct}%">
            <span class="vital-label">${vitalTag(key)}</span>
            <span class="vital-track">
              <span class="vital-fill" style="width:${Math.max(0, Math.min(100, pct))}%"></span>
            </span>
          </span>
        `,
      )}
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
