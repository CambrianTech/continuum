/**
 * `renderServing` — the web renderer for the SERVING console (`purpose="serving"`).
 *
 * The machine room, center-stage (console doctrine, Joel 2026-08-01: the
 * graphical FULL VIEW lives here — futuristic-console legibility, state
 * readable at a distance — never crammed into rails). One panel per grid
 * node, local first: a banner (node · model · ready pulse · lanes×ctx) with
 * the HEADLINE tok/s numeral large on the right, the full-width control-loop
 * instrument (hit/tok-s/fetch), the bandit's arm bank, and the pager event
 * feed. Registered for `SERVING_PURPOSE` in the ONE content registry — the
 * ops sibling of chat/foundry/arena/live. Pure fragments of the projected
 * `ServingContentBody`.
 *
 * Honesty rules: no nodes → the awaiting frame (the frame is the promise);
 * `feedLive` false → the "snapshot" banner; a node without pager capture
 * renders its banner + an honest no-telemetry line, never a fabricated gauge.
 */

import { html, nothing, type TemplateResult } from 'lit';
import type { ServingContentBody, ServingNodeVM, ServingPanelView } from '@continuum/patterns';
import { renderGaugeBody } from '../render/parts';

/** The banner's headline stat — the current tok/s reading, extracted from the
 *  projection's own series (source-formatted; no client unit math). */
function headline(view: ServingPanelView): TemplateResult | typeof nothing {
  const tok = view.gauge?.series.find((s) => s.label.toLowerCase().includes('tok'));
  if (!tok) return nothing;
  return html`<span class="srv-headline" title="current decode rate">
    <span class="srv-headline-num">${tok.current}</span>
    <span class="srv-headline-unit">tok/s</span>
  </span>`;
}

function banner(n: ServingNodeVM): TemplateResult {
  const h = n.view.header;
  return html`<div class="srv-banner">
    <span class="srv-node-name" ?data-local=${n.local} title=${n.local ? 'this node' : 'grid peer'}>
      ${n.node}${n.local ? html`<span class="srv-local-chip">local</span>` : nothing}
    </span>
    ${h?.model
      ? html`<span class="srv-model" title=${h.model}>${h.model}</span>
          <span class="srv-pulse" data-ready=${h.ready ? 'true' : 'false'}></span>
          <span class="srv-lanes">${h.ready ? 'ready' : 'warming'} · ${h.lanes}×${h.contextWindow}</span>`
      : h?.degradedReason
        ? html`<span class="srv-degraded" title=${h.degradedReason}>⚠ ${h.degradedReason}</span>`
        : html`<span class="srv-lanes">no model serving</span>`}
    ${headline(n.view)}
  </div>`;
}

function armBank(view: ServingPanelView): TemplateResult | typeof nothing {
  if (view.arms.length === 0) return nothing;
  return html`<div class="srv-bank" title="bandit decay arms — reward belief per arm">
    <span class="srv-section-label">decay arms</span>
    <div class="serving-arms srv-bank-arms">
      ${view.arms.map(
        (a) => html`<span
          class="serving-arm"
          ?data-chosen=${a.chosen}
          title="decay ${a.label} · reward ${a.reward.toFixed(3)}${a.chosen ? ' · serving' : ''}"
        >
          <span class="arm-label">${a.label}</span>
          <span class="arm-reward">${a.reward.toFixed(2)}</span>
          <span class="arm-bar" style="width:${Math.round(Math.min(1, Math.max(0, a.reward)) * 100)}%"></span>
        </span>`,
      )}
    </div>
  </div>`;
}

function eventFeed(view: ServingPanelView): TemplateResult | typeof nothing {
  if (view.events.length === 0) return nothing;
  const newestFirst = [...view.events].reverse();
  return html`<div class="srv-feed">
    <span class="srv-section-label">control loop</span>
    <ul class="serving-events srv-feed-events">
      ${newestFirst.map(
        (e) => html`<li class="serving-event" data-kind=${e.kind}>
          <span class="event-token">t${e.atToken}</span>
          <span class="event-detail">${e.detail}</span>
        </li>`,
      )}
    </ul>
  </div>`;
}

function nodePanel(n: ServingNodeVM): TemplateResult {
  return html`<section class="srv-node" data-node=${n.node}>
    ${banner(n)}
    <div class="srv-instrument">
      ${n.view.gauge
        ? renderGaugeBody(n.view.gauge)
        : html`<div class="gauge-awaiting" title="no pager capture feed on this serve">
            no pager telemetry on this serve
          </div>`}
    </div>
    ${armBank(n.view)} ${eventFeed(n.view)}
  </section>`;
}

/** The serving console — the full center view. */
export function renderServing(body: ServingContentBody): TemplateResult {
  return html`<div class="srv-console">
    ${body.feedLive
      ? nothing
      : html`<div class="srv-snapshot" title="no live serving stream attached">snapshot — not live</div>`}
    ${body.nodes.length > 0
      ? body.nodes.map(nodePanel)
      : html`<div class="srv-awaiting">
          <div class="srv-awaiting-title">no serving feed</div>
          <div class="srv-awaiting-line">
            panels appear when a node's serving daemon publishes — the frame is the promise
          </div>
        </div>`}
  </div>`;
}
