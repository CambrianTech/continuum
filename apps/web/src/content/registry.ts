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
import { createContentRegistry, type ContentRegistry } from '@continuum/patterns';
import type { ChatContentBody } from '@continuum/chat-view';
import { messageRow } from '../render/parts';

/** The chat activity's center: the conversation (or an honest empty state). */
function chatContent(body: ChatContentBody): TemplateResult {
  return body.isEmpty
    ? html`<div class="empty">No messages yet — say hello.</div>`
    : html`<ul class="messages">
        ${body.messages.map(messageRow)}
      </ul>`;
}

/** The app's Content registry. Register a new activity's renderer here (foundry's
 *  model-list renderer lands the same way, keyed `"foundry"`); nothing else changes. */
export const webContentRegistry: ContentRegistry<TemplateResult> =
  createContentRegistry<TemplateResult>();

webContentRegistry.register<ChatContentBody>('chat', (body) => chatContent(body));
