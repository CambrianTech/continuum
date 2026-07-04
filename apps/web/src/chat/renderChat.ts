/**
 * `renderChat` — the pure Lit template for the three-panel chat surface.
 *
 * It takes an already-projected `ChatViewModel` (from `chatViewModel`) and
 * returns markup — no substrate calls, no state merges, no name resolution. All
 * "how it reads" logic lives upstream in the view model; this file only maps
 * fields to elements. That split is why the presentation logic is unit-tested
 * (the view model, without a browser) while this stays a thin, obvious template.
 *
 * The layout IS Joel's three-panel who/what/where design:
 *   ┌─────────────────────────────────────────────┐
 *   │ header — WHERE/WHICH (room + counts)         │
 *   ├───────────────┬─────────────────────────────┤
 *   │ roster — WHO  │ messages — WHAT             │
 *   │ (presence)    │ (the conversation)          │
 *   └───────────────┴─────────────────────────────┘
 * (the compose bar under WHAT is owned by `<chat-widget>`, which needs the input
 * state + send handler — this function renders only the read surface.)
 */

import { html, nothing, type TemplateResult } from 'lit';
import type { ChatViewModel, MemberKind, MessageRowVM, RosterMemberVM } from '@continuum/chat-view';

/** Short glyph per author kind — the neutral human/agent/system discriminant. */
function kindGlyph(kind: MemberKind): string {
  switch (kind) {
    case 'human':
      return '🧑';
    case 'agent':
      return '🤖';
    case 'system':
      return '⚙️';
  }
}

/** The runtime origin badge, only when the substrate resolved one. */
function runtimeBadge(runtime: string): TemplateResult | typeof nothing {
  return runtime ? html`<span class="runtime" title="runtime origin">${runtime}</span>` : nothing;
}

/** Human-readable member-kind label for the card badge. */
function kindLabel(kind: MemberKind): string {
  switch (kind) {
    case 'human':
      return 'human';
    case 'agent':
      return 'agent';
    case 'system':
      return 'system';
  }
}

/** One roster row — a member card (avatar + presence dot, name, kind/runtime),
 *  the visual language of the old Users & Agents tile, off the design tokens. */
function rosterRow(m: RosterMemberVM): TemplateResult {
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
      </span>
    </li>
  `;
}

/** One conversation row — WHAT was said. */
function messageRow(msg: MessageRowVM): TemplateResult {
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

/** The read surface: header + roster + messages, from one view model. */
export function renderChat(vm: ChatViewModel): TemplateResult {
  return html`
    <header class="room">
      <div class="room-name">${vm.roomName}</div>
      <div class="room-meta">
        <span class="count" title="active / total">${vm.activeCount}/${vm.memberCount} here</span>
        <span class="room-id" title="room id">${vm.roomId}</span>
      </div>
    </header>
    <div class="panels">
      <aside class="who" aria-label="roster">
        <div class="who-head">
          <span class="who-title">Users &amp; Agents</span>
          <span class="who-count">${vm.memberCount}</span>
        </div>
        <ul class="roster">
          ${vm.members.map(rosterRow)}
        </ul>
      </aside>
      <section class="what" aria-label="conversation">
        ${vm.isEmpty
          ? html`<div class="empty">No messages yet — say hello.</div>`
          : html`<ul class="messages">
              ${vm.messages.map(messageRow)}
            </ul>`}
      </section>
    </div>
  `;
}
