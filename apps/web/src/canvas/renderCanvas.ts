/**
 * `renderCanvas` — the web renderer for the design-bench run room's CANVAS
 * region (`purpose="canvas"`, DESIGN-BENCH-VISUAL-CRAFT.md §5: the persona's
 * RENDERED page, live, re-observed on her writes — the walk-in sees the
 * design evolve).
 *
 * The stage is the page ITSELF when a renderable source exists — inline HTML
 * in a sandboxed iframe (`sandbox="allow-scripts"`, NO same-origin: the
 * artifact is citizen-authored content and must never script this shell),
 * or the served URL in the same sandbox — and the last-observed screenshot
 * when only pixels are available. A compact header carries the observation
 * facts: WHO observed, WHEN, at what viewport, with the craft scorecard chip
 * (gates passed/total) when the oracle has graded.
 *
 * Honesty rules (same contract as bench/arena/serving): no artifact → the
 * awaiting frame; `feedLive` false → the "snapshot" banner; unmeasured pages
 * carry no score chip; no observed-at stamp is drawn when the wire carries
 * none — never a fabricated pulse.
 */

import { html, nothing, type TemplateResult } from 'lit';
import type { CanvasCheckVM, CanvasContentBody, CanvasScoreVM } from '@continuum/patterns';
import { agoText } from '@continuum/chat-view';

/** One scorecard check row — tier badge, verdict dot, assertion, measured
 *  detail (the receipt a citizen iterates against). */
function checkRow(c: CanvasCheckVM): TemplateResult {
  return html`<li class="canvas-check" data-passed=${c.passed ? 'yes' : 'no'}>
    <span class="canvas-check-dot" title=${c.passed ? 'passed' : 'failed'}></span>
    <span class="canvas-check-tier" title=${c.tier === 'v1' ? 'structure gate (UiCheck)' : 'measured-craft gate (StyleCheck)'}>${c.tier}</span>
    <span class="canvas-check-name">${c.name}</span>
    ${c.detail ? html`<span class="canvas-check-detail">${c.detail}</span>` : nothing}
  </li>`;
}

/** The scorecard panel under the stage — gate checks (failures lead, the
 *  projection sorts) + the V3 judge objective when a panel has scored. */
function scorePanel(score: CanvasScoreVM): TemplateResult {
  return html`<div class="canvas-score" role="group" aria-label="craft scorecard">
    <ul class="canvas-checks">${score.checks.map(checkRow)}</ul>
    ${score.judge !== undefined
      ? html`<div class="canvas-judge" title="judge-panel objective — a weight, never a gate">
          judge ${(score.judge * 100).toFixed(0)}%</div>`
      : nothing}
  </div>`;
}

/** The score CHIP in the header — gates at a glance; all-green earns the
 *  success tone, any failing gate the alarm tone. */
function scoreChip(score: CanvasScoreVM): TemplateResult {
  const clean = score.passed === score.total;
  return html`<span
    class="canvas-chip canvas-chip-score"
    data-clean=${clean ? 'yes' : 'no'}
    title="craft gates passed / total (V1 structure · V2 measured craft)"
    >${score.passed}/${score.total} gates</span
  >`;
}

/** The canvas region content — pure fragments of the projected body. */
export function renderCanvas(body: CanvasContentBody): TemplateResult {
  if (!body.artifact) {
    return html`<div class="canvas-region">
      ${body.feedLive ? nothing : html`<div class="canvas-snapshot-banner">snapshot — no live feed attached</div>`}
      <div class="canvas-awaiting">
        <p>No page observed yet.</p>
        <p class="canvas-awaiting-sub">
          The stage lights when a citizen writes and the room re-observes her render — the frame is the promise.
        </p>
      </div>
    </div>`;
  }
  const observed = body.observedAtMs !== undefined ? agoText(body.observedAtMs, Date.now()) : undefined;
  const a = body.artifact;
  // Stage preference: live render (inline HTML, then served URL) over pixels.
  // The sandbox has NO allow-same-origin: citizen-authored pages run their own
  // scripts (the fluid-sim class of artifact needs them) but stay fully
  // cross-origin-isolated from this shell.
  const stage = a.html !== undefined
    ? html`<iframe
        class="canvas-stage-frame"
        sandbox="allow-scripts"
        title=${a.title}
        srcdoc=${a.html}
      ></iframe>`
    : a.url !== undefined
      ? html`<iframe class="canvas-stage-frame" sandbox="allow-scripts" title=${a.title} src=${a.url}></iframe>`
      : html`<img class="canvas-stage-shot" src=${a.screenshot ?? ''} alt="last observed screenshot — ${a.title}" />`;
  return html`<div class="canvas-region">
    ${body.feedLive ? nothing : html`<div class="canvas-snapshot-banner">snapshot — no live feed attached</div>`}
    <div class="canvas-head">
      <span class="canvas-title" title=${a.title}>${a.title}</span>
      ${a.html === undefined && a.url === undefined
        ? html`<span class="canvas-chip" title="no renderable source on the wire — showing the last observed pixels">pixels only</span>`
        : nothing}
      ${body.score ? scoreChip(body.score) : nothing}
      <span class="canvas-head-facts">
        ${body.persona ? html`<span class="canvas-persona" title="the observing citizen">${body.persona}</span>` : nothing}
        ${body.viewport
          ? html`<span class="canvas-chip" title="observation viewport">${body.viewport.width}×${body.viewport.height}</span>`
          : nothing}
        ${body.revision !== undefined
          ? html`<span class="canvas-chip" title="observations of this artifact — the iterate loop's pulse">obs #${body.revision}</span>`
          : nothing}
        ${observed ? html`<span class="canvas-observed" title="last observed">${observed}</span>` : nothing}
      </span>
    </div>
    <div class="canvas-stage">${stage}</div>
    ${body.score ? scorePanel(body.score) : nothing}
  </div>`;
}
