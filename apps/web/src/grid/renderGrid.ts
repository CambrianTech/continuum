/**
 * `renderGrid` — the web renderer for the GRID view (`purpose="grid"`).
 *
 * The NODES strip's full activity, center-stage: one rich panel per node —
 * node banner (name, LOCAL mark, serving model + ready pulse when live),
 * the RESOURCES window (CPU/MEM/GPU), and the SERVING control loop — the
 * whole grid legible at a distance. Registered for `GRID_PURPOSE` in the
 * ONE content registry. Route health and resident citizens join the panel
 * as their feeds land (#257 attestation, delivery-truth ledger); grid
 * peers appear as rows the moment cross-grid feeds carry them (#283).
 *
 * Honesty: a panel renders exactly the sections its node's feeds deliver;
 * an empty grid renders the awaiting frame.
 */

import { html, nothing, type TemplateResult } from 'lit';
import type { GridContentBody, GridNodeVM } from '@continuum/patterns';
import { renderGaugeBody } from '../render/parts';
import { renderServingBody } from '../render/ServingPanel';

function banner(n: GridNodeVM): TemplateResult {
  const h = n.serving?.header;
  return html`<div class="srv-banner">
    <span class="srv-node-name grid-node-name" ?data-local=${n.local} title=${n.local ? 'this node' : 'grid peer'}>
      ${n.node}${n.local ? html`<span class="srv-local-chip">local</span>` : nothing}
    </span>
    ${h?.model
      ? html`<span class="srv-model" title=${h.model}>${h.model}</span>
          <span class="srv-pulse" data-ready=${h.ready ? 'true' : 'false'}></span>`
      : nothing}
  </div>`;
}

function nodePanel(n: GridNodeVM): TemplateResult {
  return html`<section class="srv-node" data-node=${n.node}>
    ${banner(n)}
    ${n.resources
      ? html`<div class="grid-section">
          <span class="srv-section-label">resources</span>
          ${renderGaugeBody(n.resources)}
        </div>`
      : nothing}
    ${n.serving
      ? html`<div class="grid-section">
          <span class="srv-section-label">serving</span>
          ${renderServingBody(n.serving)}
        </div>`
      : nothing}
    ${!n.resources && !n.serving
      ? html`<div class="gauge-awaiting" title="no feeds from this node yet">awaiting node feeds…</div>`
      : nothing}
  </section>`;
}

/** The grid view — every node's panel, local first. */
export function renderGrid(body: GridContentBody): TemplateResult {
  return html`<div class="srv-console">
    ${body.feedLive
      ? nothing
      : html`<div class="srv-snapshot" title="no live node streams attached">snapshot — not live</div>`}
    ${body.nodes.length > 0
      ? body.nodes.map(nodePanel)
      : html`<div class="srv-awaiting">
          <div class="srv-awaiting-title">no node feeds</div>
          <div class="srv-awaiting-line">
            panels appear as nodes publish their state — the frame is the promise
          </div>
        </div>`}
  </div>`;
}
