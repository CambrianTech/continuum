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

import { html, svg, type TemplateResult } from 'lit';
import {
  createWidgetRegistry,
  type WidgetRegistry,
  type GaugeView,
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

/** Per-series hues for the SYS gauge — the old sidebar's legend palette (CPU
 *  red · MEM green · GPU purple), keyed by label with a cyan fallback for any
 *  future series so an unknown label still draws. */
const GAUGE_HUES: Record<string, string> = {
  CPU: '#ff5c5c',
  MEM: '#3fb950',
  GPU: '#a78bfa',
};

/** One series → an SVG polyline over a fixed 0..=100 viewBox. Points are
 *  already normalized upstream; x spreads the window across the width so a
 *  short (fresh-boot) series draws from the left edge outward, honestly. */
function sparkline(points: readonly number[], hue: string, w: number, h: number): TemplateResult {
  if (points.length < 2) return svg``;
  const step = w / (points.length - 1);
  const pts = points.map((p, i) => `${(i * step).toFixed(1)},${(h - (p / 100) * h).toFixed(1)}`);
  return svg`<polyline points=${pts.join(' ')} fill="none" stroke=${hue} stroke-width="1.5" vector-effect="non-scaling-stroke" />`;
}

/** `'gauge'` — the SYS graph (brick 2): a multi-series sparkline over a faint
 *  grid with the legend row beneath (hue dot · LABEL · current reading), the
 *  old sidebar's CPU/MEM/GPU panel reborn from the core-carried window. */
webWidgetRegistry.register('gauge', (widget) => {
  const view = widget.body as GaugeView;
  const W = 240;
  const H = 56;
  return html`
    <section class="rail-widget" data-widget="gauge" data-id=${widget.id}>
      <div class="who-head">
        <span class="who-title">${widget.title}</span>
      </div>
      <div class="gauge">
        <svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" aria-label="resource history">
          ${[0.25, 0.5, 0.75].map(
            (f) => svg`<line x1="0" y1=${H * f} x2=${W} y2=${H * f} class="gauge-grid" />`,
          )}
          ${view.series.map((s) => sparkline(s.points, GAUGE_HUES[s.label] ?? '#7dd3fc', W, H))}
        </svg>
        <div class="gauge-legend">
          ${view.series.map(
            (s) => html`<span class="gauge-key">
              <span class="gauge-dot" style="background:${GAUGE_HUES[s.label] ?? '#7dd3fc'}"></span>
              <span class="gauge-label">${s.label}</span>
              <span class="gauge-val">${s.current}</span>
            </span>`,
          )}
        </div>
      </div>
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
