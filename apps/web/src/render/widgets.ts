/**
 * `webWidgetRegistry` — the web target's left-rail widget dispatch table.
 *
 * The left rail is a GLOBAL WIDGET STACK: each `PanelWidget` draws as a titled rail
 * section, dispatched by `kind` exactly as room content is dispatched by `purpose`
 * (`webContentRegistry`). A `'listing'` widget (the roster, a rooms list) draws the
 * `Listing` primitive; richer kinds — `'metrics'` (resources + spend), `'rooms'`
 * (All/DMs filter), `'status'`, `'continuon'` — register here as they land (task #184),
 * never by growing `webTarget` ([[app-shell-layout-left-global-right-per-activity]]).
 *
 * Each renderer owns its ENTIRE section (header included) because widget headers differ
 * — a metrics widget carries a time selector, a rooms widget its filter tabs, the roster
 * its member count. One registry, one section-per-widget contract, styled from the shared
 * cyberpunk tokens.
 */

import { html, type TemplateResult } from 'lit';
import {
  createWidgetRegistry,
  type WidgetRegistry,
  type ListingView,
  type MetricsView,
} from '@continuum/patterns';
import { renderListing } from './parts';

/** The web left-rail registry. Import for its side-effectful registrations; `webTarget`
 *  dispatches `widget()` through it. */
export const webWidgetRegistry: WidgetRegistry<TemplateResult> = createWidgetRegistry();

/** `'listing'` — the roster / rooms list as a rail section: an uppercase header with a
 *  live count, then the `Listing` rows (rich member cards for the roster). Reuses the
 *  existing `.who-head` header styling so the roster reads identically to before the
 *  rail became a widget stack. */
webWidgetRegistry.register('listing', (widget) => {
  const view = widget.body as ListingView;
  return html`
    <section class="rail-widget" data-widget="listing" data-id=${widget.id}>
      <div class="who-head">
        <span class="who-title">${widget.title}</span>
        <span class="who-count">${view.cells.length}</span>
      </div>
      ${renderListing(view)}
    </section>
  `;
});

/** `'metrics'` — the "AI Performance" rail widget: an uppercase header over a compact
 *  stat row (value over label, tone-coloured), the live team-cognition readout. A
 *  sparkline draws when the body carries a `spark` series (a future real-metrics feed);
 *  today the honest slice is the stat row. */
webWidgetRegistry.register('metrics', (widget) => {
  const view = widget.body as MetricsView;
  return html`
    <section class="rail-widget" data-widget="metrics" data-id=${widget.id}>
      <div class="who-head">
        <span class="who-title">${widget.title}</span>
      </div>
      <div class="metrics-row">
        ${view.stats.map(
          (s) => html`<span class="metric" data-tone=${s.tone ?? 'muted'}>
            <span class="metric-val">${s.value}</span>
            <span class="metric-label">${s.label}</span>
          </span>`,
        )}
      </div>
    </section>
  `;
});
