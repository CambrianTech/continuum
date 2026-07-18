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

import { html, nothing, type TemplateResult } from 'lit';
import type {
  RenderTarget,
  WorkspaceView,
  ListingView,
  ContentView,
  ContextPanelView,
  PanelWidget,
} from '@continuum/patterns';
import { renderListing } from './parts';
import { webContentRegistry } from '../content/registry';
import { webWidgetRegistry } from './widgets';

/** The first `kind:'listing'` widget in the rail — the roster — used only for the
 *  header's "active / total" count. The rail itself draws every widget generically. */
function rosterOf(ws: WorkspaceView): ListingView | undefined {
  const w = ws.left.find((widget) => widget.kind === 'listing');
  return w ? (w.body as ListingView) : undefined;
}

export const webTarget: RenderTarget<TemplateResult> = {
  /** A `Listing`. The roster draws as rich member cards (the neutral cell now carries
   *  glyph/name/badges/status/meters); every other listing uses the generic cell. */
  listing(view: ListingView): TemplateResult {
    return renderListing(view);
  },

  /** The center, dispatched by room purpose through the web Content registry. */
  content(view: ContentView): TemplateResult {
    return webContentRegistry.render(view);
  },

  contextPanel(view: ContextPanelView): TemplateResult {
    return html`${view.listings.map((l) => this.listing(l))}`;
  },

  /** One left-rail widget, dispatched by kind through the web Widget registry. */
  widget(view: PanelWidget): TemplateResult {
    return webWidgetRegistry.render(view);
  },

  /** The three-panel who/what/where — reproduced from the WorkspaceView alone. The left
   *  rail is now a GLOBAL WIDGET STACK: each `PanelWidget` draws as a titled rail section
   *  (Metrics · Rooms · Users & Agents · …), dispatched by kind. */
  workspace(ws: WorkspaceView): TemplateResult {
    const room = ws.nav.cells[0];
    const roster = rosterOf(ws);
    const memberCount = roster?.cells.length ?? 0;
    const activeCount = roster?.cells.filter((c) => c.status === 'active').length ?? 0;
    return html`
      <header class="room">
        <div class="room-name">${room?.title ?? ''}</div>
        <div class="room-meta">
          <span class="count" title="active / total">${activeCount}/${memberCount} here</span>
          <span class="live" title="live · ${room?.id ?? ''}"><span class="live-dot"></span>live</span>
        </div>
      </header>
      <div class="panels">
        <aside class="who" aria-label="global widgets">
          ${ws.left.length > 0 ? ws.left.map((w) => this.widget(w)) : nothing}
        </aside>
        <section class="what" aria-label="conversation">${this.content(ws.content)}</section>
      </div>
    `;
  },
};
