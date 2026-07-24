/**
 * The web `Content` registry — the MIME-table that dispatches the center panel by
 * the focused room's `purpose` (ACTIVITY-ROOM-PATTERNS.md). This is the seam that
 * makes `activity=room=content=tab` real in the browser: a `chat` room renders the
 * conversation, a `foundry` room its model list, each registered here. The shell
 * calls `render(workspace.content)`; the registry routes on `content.purpose` and
 * **fails loud** on an unregistered purpose — an unknown activity is a wiring bug,
 * never a blank ([[fallbacks-are-illegal-fail-loud]]).
 */

import { html, type TemplateResult } from 'lit';
import {
  createContentRegistry,
  PERSONA_PURPOSE,
  type ContentRegistry,
  type PersonaContentBody,
} from '@continuum/patterns';
import type { ChatContentBody } from '@continuum/chat-view';
import { modelCell, type ForgeContentBody } from '@continuum/foundry-view';
import { listingCell, messageRow } from '../render/parts';
import { renderPersona } from '../persona/renderPersona';

/** The chat activity's center: the conversation (or an honest empty state). */
function chatContent(body: ChatContentBody): TemplateResult {
  return body.isEmpty
    ? html`<div class="empty">No messages yet — say hello.</div>`
    : html`<ul class="messages">
        ${body.messages.map(messageRow)}
      </ul>`;
}

/** The foundry activity's center: the model catalogue, drawn through the SAME
 *  generic `listingCell` the roster/rooms use — each model projected by foundry-view's
 *  `modelCell`. This is the outlier that proves the shell dispatches by purpose:
 *  chat → conversation, foundry → models, one registry, no shell change. */
function foundryContent(body: ForgeContentBody): TemplateResult {
  return body.models.length === 0
    ? html`<div class="empty">No models in the catalogue yet.</div>`
    : html`<ul class="cells">
        ${body.models.map((m) => listingCell(modelCell(m)))}
      </ul>`;
}

/** The app's Content registry. A new activity registers its renderer here keyed by
 *  purpose; the shell doesn't change. */
export const webContentRegistry: ContentRegistry<TemplateResult> =
  createContentRegistry<TemplateResult>();

webContentRegistry.register<ChatContentBody>('chat', (body) => chatContent(body));
webContentRegistry.register<ForgeContentBody>('foundry', (body) => foundryContent(body));
// The persona HOME — the profile + brain HUD center, dispatched when the
// focused tab is persona-kind (the projection publishes purpose "persona").
webContentRegistry.register<PersonaContentBody>(PERSONA_PURPOSE, (body) => renderPersona(body));
