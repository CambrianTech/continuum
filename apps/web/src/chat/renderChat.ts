/**
 * `renderChat` — the pure Lit template for the three-panel who/what/where surface.
 *
 * Takes an already-projected `ChatViewModel` and returns markup. All "how it reads"
 * logic lives upstream in the view model + the pattern projections; this file only
 * lays out the panels and **dispatches the center by the room's `purpose`** through
 * the web Content registry — so the same shell renders chat today and foundry when
 * its renderer registers (ACTIVITY-ROOM-PATTERNS.md). The member cards + message
 * rows are shared fragments (`../render/parts`).
 *
 *   ┌─────────────────────────────────────────────┐
 *   │ header — WHERE/WHICH (room + counts)         │
 *   ├───────────────┬─────────────────────────────┤
 *   │ roster — WHO  │ Content — WHAT              │  ← dispatched by purpose
 *   │ (Listing)     │ (chat → conversation)       │
 *   └───────────────┴─────────────────────────────┘
 * (the compose bar under WHAT is owned by `<chat-widget>`, which needs the input
 * state + send handler — this function renders only the read surface.)
 */

import { html, type TemplateResult } from 'lit';
import { chatWorkspace, type ChatViewModel } from '@continuum/chat-view';
import { memberCard } from '../render/parts';
import { webContentRegistry } from '../content/registry';

/** The read surface: header + roster `Listing` + purpose-dispatched Content. */
export function renderChat(vm: ChatViewModel): TemplateResult {
  // Project onto the pattern primitives; the shell draws the pieces + routes the
  // center on `content.purpose` (the Content registry). One projection, both eyes
  // (this) and the persona's grounding read it.
  const ws = chatWorkspace(vm);
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
          ${vm.members.map(memberCard)}
        </ul>
      </aside>
      <section class="what" aria-label=${ws.content.purpose}>
        ${webContentRegistry.render(ws.content)}
      </section>
    </div>
  `;
}
