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

import { html, nothing, type TemplateResult } from 'lit';
import {
  createWidgetRegistry,
  type WidgetRegistry,
  type ContinuonView,
  type GaugeView,
  type ListingView,
  type MetricsView,
  type ServingPanelView,
  type SystemPanelView,
} from '@continuum/patterns';
import { renderGaugeBody, renderListing, renderMetricsRow } from './parts';
import './RoomsPanel'; // registers <rooms-panel> (the dense rooms rail section)
import './SysPanel'; // registers <sys-panel> (the two-faced SYS|AI system panel)
import './ServingPanel'; // registers <serving-panel> (the serving glass box, #141)

/** The web left-rail registry. Import for its side-effectful registrations; `webTarget`
 *  dispatches `widget()` through it. */
export const webWidgetRegistry: WidgetRegistry<TemplateResult> = createWidgetRegistry();

/** `'listing'` — the roster / rooms list as a rail section: an uppercase header with a
 *  live count, then the `Listing` rows (rich member cards for the roster). Reuses the
 *  existing `.who-head` header styling so the roster reads identically to before the
 *  rail became a widget stack. */
webWidgetRegistry.register('listing', (widget) => {
  const view = widget.body as ListingView;
  // The rooms listing draws through the dense `<rooms-panel>` (filter facets +
  // purpose descriptions + the start-conversation affordance) — same neutral
  // ListingView, a richer web idiom for THIS listing. Every other listing keeps
  // the generic titled section.
  if (view.id === 'rooms') {
    return html`<rooms-panel .view=${view} .heading=${widget.title}></rooms-panel>`;
  }
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

/** `'continuon'` — the rail's identity header (the old sidebar's continuum mark):
 *  a breathing status orb + the wordmark + tagline, a version badge, and the tiny
 *  live-activity ticker (last turns, digested by the projection). The mark reads
 *  ALIVE from data — `alive` gates the breath, the ticker carries the real feed. */
webWidgetRegistry.register('continuon', (widget) => {
  const view = widget.body as ContinuonView;
  return html`
    <section class="rail-widget" data-widget="continuon" data-id=${widget.id}>
      <div class="continuon">
        <span class="continuon-orb" data-alive=${view.alive ? 'yes' : 'no'}></span>
        <div class="continuon-id">
          <div class="continuon-row">
            <span class="continuon-wordmark">${view.wordmark}</span>
            ${view.version
              ? html`<span class="continuon-version" title="client build">${view.version}</span>`
              : nothing}
          </div>
          ${view.tagline ? html`<div class="continuon-tagline">${view.tagline}</div>` : nothing}
        </div>
        ${view.ticker.length > 0
          ? html`<div class="continuon-ticker" title="latest activity">
              ${view.ticker.map((line) => html`<div class="continuon-tick">${line}</div>`)}
            </div>`
          : nothing}
      </div>
    </section>
  `;
});

/** `'gauge'` — the SYS graph (brick 2): a multi-series sparkline over a faint
 *  grid with the legend row beneath (hue dot · LABEL · current reading), the
 *  old sidebar's CPU/MEM/GPU panel reborn from the core-carried window. Body
 *  markup shared with `<sys-panel>`'s SYS face via `renderGaugeBody`. */
webWidgetRegistry.register('gauge', (widget) => {
  const view = widget.body as GaugeView;
  return html`
    <section class="rail-widget" data-widget="gauge" data-id=${widget.id}>
      <div class="who-head">
        <span class="who-title">${widget.title}</span>
      </div>
      ${renderGaugeBody(view)}
    </section>
  `;
});

/** `'metrics'` — the "AI Performance" rail widget: an uppercase header over a compact
 *  stat row (value over label, tone-coloured), the live team-cognition readout. Body
 *  markup shared with `<sys-panel>`'s AI face via `renderMetricsRow`. */
webWidgetRegistry.register('metrics', (widget) => {
  const view = widget.body as MetricsView;
  return html`
    <section class="rail-widget" data-widget="metrics" data-id=${widget.id}>
      <div class="who-head">
        <span class="who-title">${widget.title}</span>
      </div>
      ${renderMetricsRow(view)}
    </section>
  `;
});

/** `'status'` — the NODES strip (the factory sidebar's "1/1 nodes online"):
 *  an online summary in the header, then one compact row per node (pulse dot ·
 *  name · role chip). Body is the neutral `Listing`; only attested nodes reach
 *  it, so the count is real by construction. */
webWidgetRegistry.register('status', (widget) => {
  const view = widget.body as ListingView;
  const online = view.cells.filter((c) => c.status === 'active').length;
  return html`
    <section class="rail-widget" data-widget="status" data-id=${widget.id}>
      <div class="who-head">
        <span class="who-title">${widget.title}</span>
        <span class="nodes-online"
          ><span class="node-dot" data-on=${online > 0 ? '' : nothing}></span>${online}/${view.cells
            .length}
          online</span
        >
      </div>
      <ul class="nodes">
        ${view.cells.map(
          (c) => html`<li class="node-row" data-status=${c.status ?? 'none'}>
            <span class="node-dot" data-on=${c.status === 'active' ? '' : nothing}></span>
            <span class="node-name">${c.title}</span>
            ${c.subtitle ? html`<span class="node-role">${c.subtitle}</span>` : nothing}
          </li>`,
        )}
      </ul>
    </section>
  `;
});

/** `'system'` — the two-faced SYS|AI panel: gauge + team stats behind a real
 *  toggle, drawn by `<sys-panel>` (which face shows is the reader's lens —
 *  element state, never projection state). */
webWidgetRegistry.register('system', (widget) => {
  const view = widget.body as SystemPanelView;
  return html`<sys-panel .body=${view} .heading=${widget.title}></sys-panel>`;
});

/** `'serving'` — the serving glass box (#141 slice 1): header + pager
 *  sparklines + bandit arms + event cards, drawn by `<serving-panel>`. */
webWidgetRegistry.register('serving', (widget) => {
  const view = widget.body as ServingPanelView;
  return html`<serving-panel .body=${view} .heading=${widget.title}></serving-panel>`;
});
