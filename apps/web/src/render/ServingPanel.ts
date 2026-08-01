/**
 * `<serving-panel>` — the serving glass box rail section (#141 slice 1): the
 * node's live inference serving, and when the MoE expert pager streams, the
 * control loop itself — hit/tok-s/fetch sparklines, the bandit's arm beliefs,
 * and pager event cards. The beat-WASTE campaign on screen.
 *
 * Light DOM so the element inherits `<chat-widget>`'s shadow stylesheet, same
 * as `<sys-panel>`. Every section renders only when its feed has delivered —
 * VISIBLE absence (an awaiting line), never a vanished panel and never a
 * fabricated gauge (the anti-disappearance rule).
 */

import { LitElement, html, nothing, type TemplateResult } from 'lit';
import type { ServingPanelView } from '@continuum/patterns';
import { renderGaugeBody } from './parts';

/** The serving body's full inner render — header line + sparklines + arm
 *  chips + event cards. Shared by `<serving-panel>` (standalone / future
 *  console) and `<sys-panel>`'s SRV face (the rail's one tabbed telemetry
 *  control), so the two can never drift. */
export function renderServingBody(body: ServingPanelView): TemplateResult {
  return html`${renderServingHeader(body)}
  ${body.gauge
    ? renderGaugeBody(body.gauge)
    : html`<div class="gauge-awaiting" title="no pager capture feed on this serve">
        no pager telemetry
      </div>`}
  ${renderServingArms(body)} ${renderServingEvents(body)}`;
}

function renderServingHeader(body: ServingPanelView): TemplateResult {
  const h = body.header;
  if (!h) {
    return html`<div class="serving-line" title="the serving daemon has not published yet">
      awaiting serving feed…
    </div>`;
  }
  if (h.degradedReason) {
    return html`<div class="serving-line serving-degraded" title=${h.degradedReason}>
      ⚠ degraded — ${h.degradedReason}
    </div>`;
  }
  if (!h.model) {
    return html`<div class="serving-line">no model serving</div>`;
  }
  return html`<div class="serving-line" data-ready=${h.ready ? 'true' : 'false'}>
    <span class="serving-model" title=${h.model}>${h.model}</span>
    <span class="serving-meta"> ${h.ready ? 'ready' : 'warming'} · ${h.lanes}×${h.contextWindow} </span>
  </div>`;
}

function renderServingArms(body: ServingPanelView): TemplateResult | typeof nothing {
  const arms = body.arms;
  if (arms.length === 0) return nothing;
  return html`<div class="serving-arms" title="bandit decay arms — reward belief per arm">
    ${arms.map(
      (a) => html`<span
        class="serving-arm"
        ?data-chosen=${a.chosen}
        title="decay ${a.label} · reward ${a.reward.toFixed(3)}${a.chosen ? ' · serving' : ''}"
      >
        <span class="arm-label">${a.label}</span>
        <span class="arm-bar" style="width:${Math.round(Math.min(1, Math.max(0, a.reward)) * 100)}%"></span>
      </span>`,
    )}
  </div>`;
}

function renderServingEvents(body: ServingPanelView): TemplateResult | typeof nothing {
  const events = body.events;
  if (events.length === 0) return nothing;
  // Newest first for the glanceable card stack.
  const newestFirst = [...events].reverse();
  return html`<ul class="serving-events">
    ${newestFirst.map(
      (e) => html`<li class="serving-event" data-kind=${e.kind}>
        <span class="event-token">t${e.atToken}</span>
        <span class="event-detail">${e.detail}</span>
      </li>`,
    )}
  </ul>`;
}

export class ServingPanel extends LitElement {
  static override properties = {
    body: { attribute: false },
    heading: { attribute: false },
  };

  /** The projected glass-box body. */
  body?: ServingPanelView;

  /** Section heading (the PanelWidget title). */
  heading = 'Serving';

  protected override createRenderRoot(): HTMLElement {
    return this;
  }

  override render(): TemplateResult {
    const body = this.body;
    if (!body) return html``;
    return html`
      <section class="rail-widget" data-widget="serving" data-id="serving">
        <div class="who-head">
          <span class="who-title">${this.heading}</span>
        </div>
        ${renderServingBody(body)}
      </section>
    `;
  }
}

customElements.define('serving-panel', ServingPanel);

declare global {
  interface HTMLElementTagNameMap {
    'serving-panel': ServingPanel;
  }
}
