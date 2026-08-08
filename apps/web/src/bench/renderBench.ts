/**
 * `renderBench` — the web renderer for the Academy's live BENCHMARK BOARD
 * (`purpose="bench"`, #374/#329: a benchmark IS a live room).
 *
 * One row per run — operator-launched and citizen-claimed on the SAME board
 * (the 2026-08-08 contention hour, made one glance). Every cell reports
 * PROGRESS, never bare liveness: generations + last-generation age, edit
 * acts, patch bytes, attempt, and the graded verdict — with a pass-to-pass
 * REGRESSION rendered as the alarm it is (the hidden-collateral lesson:
 * buried in a count it taught "not fixed yet" when the truth was "destroyed
 * the tree").
 *
 * Honesty rules (same contract as serving/arena): no runs → the awaiting
 * frame; `feedLive` false → the "snapshot" banner; a run before its first
 * generation renders the literal "no generations yet" — a QUEUED state is
 * shown as queued, never dressed as work ([[honest presence]]: never look
 * alive when you are not).
 */

import { html, nothing, type TemplateResult } from 'lit';
import type { BenchContentBody, BenchRunVM, BenchVerdictVM } from '@continuum/patterns';

/** Compact seconds → "12s" / "3m40s" — board legibility, not precision. */
function age(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  return `${Math.floor(seconds / 60)}m${seconds % 60}s`;
}

function verdictCell(v: BenchVerdictVM): TemplateResult {
  const cls = v.resolved ? 'bench-verdict-pass' : v.regression ? 'bench-verdict-regression' : 'bench-verdict-fail';
  return html`<span class="bench-verdict ${cls}">
    ${v.resolved ? html`<span class="bench-resolved">RESOLVED</span>` : nothing}
    ${v.regression
      ? html`<span class="bench-alarm" title="the patch broke previously-passing tests">
          REGRESSION ${v.p2pTotal - v.p2pPassed} broken</span>`
      : nothing}
    <span class="bench-counts" title="fail-to-pass / pass-to-pass">
      f2p ${v.f2pPassed}/${v.f2pTotal} · p2p ${v.p2pPassed}/${v.p2pTotal}</span>
    ${v.failedTests.length > 0
      ? html`<span class="bench-failed" title=${v.failedTests.join(', ')}>
          ${v.failedTests[0]}${v.failedTests.length > 1 ? ` +${v.failedTests.length - 1}` : ''}</span>`
      : nothing}
  </span>`;
}

function runRow(run: BenchRunVM): TemplateResult {
  const pulse =
    run.lastGenAgeS === null
      ? html`<span class="bench-nogen" title="attempt started; first generation not completed">no generations yet</span>`
      : html`<span class="bench-pulse" title="generations this attempt · age of latest">
          ${run.generations} gens · ${age(run.lastGenAgeS)} ago</span>`;
  return html`<div class="bench-row bench-state-${run.state}">
    <span class="bench-who">
      <span class="bench-persona">${run.persona}</span>
      ${run.selfClaimed ? html`<span class="bench-selfclaimed" title="claimed off the work board by the citizen herself">self-claimed</span>` : nothing}
    </span>
    <span class="bench-instance" title=${run.runId}>${run.instance}</span>
    <span class="bench-attempt">attempt ${run.attempt}/${run.maxAttempts}</span>
    <span class="bench-state">${run.state}</span>
    ${pulse}
    <span class="bench-acts" title="edit/write acts — a patch forming">${run.editActs} edits</span>
    ${run.patchBytes !== null
      ? html`<span class="bench-patch">${run.patchBytes}B patch</span>`
      : nothing}
    ${run.verdict ? verdictCell(run.verdict) : nothing}
  </div>`;
}

/** The bench board content — pure fragments of the projected body. */
export function renderBench(body: BenchContentBody): TemplateResult {
  if (body.runs.length === 0) {
    return html`<div class="bench-awaiting">
      <p>No benchmark runs on this board yet.</p>
      <p class="bench-awaiting-sub">Rows appear when a run starts — operator-launched or claimed by a citizen. The frame is the promise.</p>
    </div>`;
  }
  return html`<div class="bench-board">
    ${body.feedLive ? nothing : html`<div class="bench-snapshot-banner">snapshot — no live feed attached</div>`}
    ${body.lanePressure
      ? html`<div class="bench-lanes" title="serving lanes vs lanes of demand — contention at a glance">
          lanes ${body.lanePressure.serving} serving · ${body.lanePressure.demanding} demanding</div>`
      : nothing}
    ${body.runs.map(runRow)}
  </div>`;
}
