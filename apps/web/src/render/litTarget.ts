/**
 * `webTarget` — positron's web `RenderTarget`. Lit paints; positron defines.
 *
 * Draws the neutral `WorkspaceView` (who/what/where) to Lit `TemplateResult`s: the
 * header + roster `Listing` (as member cards) + the center dispatched by room purpose
 * through the web Content registry. This is the piece that lets `apps/web` flow through
 * the framework — `renderChat` now delegates here, and `mount(chatApp, …, webTarget, …)`
 * becomes the composition root. Byte-identical to the former inline `renderChat` markup,
 * verified by the before/after screenshot of the live three-panel.
 */

import { html, type TemplateResult } from 'lit';
import type {
  RenderTarget,
  WorkspaceView,
  ListingView,
  ContentView,
  ContextPanelView,
} from '@continuum/patterns';
import { memberCardFromCell, listingCell } from './parts';
import { webContentRegistry } from '../content/registry';

export const webTarget: RenderTarget<TemplateResult> = {
  /** A `Listing`. The roster draws as rich member cards (the neutral cell now carries
   *  glyph/name/badges/status/meters); every other listing uses the generic cell. */
  listing(view: ListingView): TemplateResult {
    if (view.id === 'roster') {
      return html`<ul class="roster">${view.cells.map(memberCardFromCell)}</ul>`;
    }
    return html`<ul class="cells">${view.cells.map(listingCell)}</ul>`;
  },

  /** The center, dispatched by room purpose through the web Content registry. */
  content(view: ContentView): TemplateResult {
    return webContentRegistry.render(view);
  },

  contextPanel(view: ContextPanelView): TemplateResult {
    return html`${view.listings.map((l) => this.listing(l))}`;
  },

  /** The three-panel who/what/where — reproduced from the WorkspaceView alone. */
  workspace(ws: WorkspaceView): TemplateResult {
    const room = ws.nav.cells[0];
    const rosterView = ws.left[0] ?? { id: 'roster', title: 'Users & Agents', cells: [] };
    const memberCount = rosterView.cells.length;
    const activeCount = rosterView.cells.filter((c) => c.status === 'active').length;
    return html`
      <header class="room">
        <div class="room-name">${room?.title ?? ''}</div>
        <div class="room-meta">
          <span class="count" title="active / total">${activeCount}/${memberCount} here</span>
          <span class="live" title="live · ${room?.id ?? ''}"><span class="live-dot"></span>live</span>
        </div>
      </header>
      <div class="panels">
        <aside class="who" aria-label="roster">
          <div class="who-head">
            <span class="who-title">Users &amp; Agents</span>
            <span class="who-count">${memberCount}</span>
          </div>
          ${this.listing(rosterView)}
        </aside>
        <section class="what" aria-label="conversation">${this.content(ws.content)}</section>
      </div>
    `;
  },
};
