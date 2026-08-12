/**
 * `renderBench` — the web renderer for the Academy's live BENCHMARK BOARD
 * (`purpose="bench"`, #374/#329: a benchmark IS a live room).
 *
 * CONSOLE-grade, not a list (Joel, 2026-08-12: the first pill-card pass was
 * "lame" — the rail must read like the machine-room it is):
 *
 * - A SCOREBOARD header: resolved / working / stalled as big glanceable
 *   stats — the round's state in one look.
 * - One card per run: state dot (pulsing while working), instance headline,
 *   a REAL progress bar (acts, normalized to the board's max — the serving
 *   sparkline's normalization precedent: shape without unit math), patch
 *   bytes as the accent chip, verdict strip with the REGRESSION alarm.
 *
 * Honesty rules unchanged (same contract as serving/arena): no runs → the
 * awaiting frame; `feedLive` false → the "snapshot" banner; a run before its
 * first generation renders "no generations yet" — queued is shown as queued,
 * never dressed as work ([[honest presence]]).
 */

import { html, nothing, type TemplateResult } from 'lit';
import type { BenchContentBody, BenchRunVM, BenchVerdictVM } from '@continuum/patterns';

/** Compact seconds → "12s" / "3m" / "2h" — board legibility, not precision. */
function age(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  return `${Math.floor(seconds / 3600)}h${Math.floor((seconds % 3600) / 60)}m`;
}

function verdictCell(v: BenchVerdictVM): TemplateResult {
  return html`<div class="bench-verdict">
    ${v.resolved ? html`<span class="bench-resolved">✓ RESOLVED</span>` : nothing}
    ${v.regression
      ? html`<span class="bench-alarm" title="the patch broke previously-passing tests">
          ▲ REGRESSION ${v.p2pTotal - v.p2pPassed} broken</span>`
      : nothing}
    <span class="bench-counts" title="fail-to-pass / pass-to-pass">
      f2p <b>${v.f2pPassed}/${v.f2pTotal}</b> · p2p <b>${v.p2pPassed}/${v.p2pTotal}</b></span>
    ${v.failedTests.length > 0
      ? html`<span class="bench-failed" title=${v.failedTests.join(', ')}>
          ${v.failedTests[0]}${v.failedTests.length > 1 ? ` +${v.failedTests.length - 1}` : ''}</span>`
      : nothing}
  </div>`;
}

function runCard(run: BenchRunVM, maxGens: number): TemplateResult {
  // Acts bar normalized to the board's busiest run — relative shape, no
  // invented budget denominator (the wire carries no per-run act budget).
  const pct = maxGens > 0 ? Math.max(4, Math.round((run.generations / maxGens) * 100)) : 0;
  return html`<div class="bench-card bench-state-${run.state}">
    <div class="bench-card-head">
      <span class="bench-dot" title=${run.state}></span>
      <span class="bench-instance" title=${run.runId}>${run.instance}</span>
      <span class="bench-attempt" title="attempt N of M">${run.attempt}<i>/</i>${run.maxAttempts}</span>
    </div>
    <div class="bench-card-meta">
      <span class="bench-persona">${run.persona}</span>
      ${run.selfClaimed ? html`<span class="bench-selfclaimed">self-claimed</span>` : nothing}
      ${run.lastGenAgeS === null
        ? html`<span class="bench-nogen">no generations yet</span>`
        : html`<span class="bench-pulse">${run.generations} gens · ${age(run.lastGenAgeS)} ago</span>`}
      ${run.patchBytes !== null
        ? html`<span class="bench-patch" title="workspace diff size — a patch is forming">${run.patchBytes}B</span>`
        : nothing}
    </div>
    ${run.generations > 0
      ? html`<div class="bench-bar" role="progressbar" aria-label="acts relative to busiest run">
          <div class="bench-bar-fill" style="width:${pct}%"></div>
        </div>`
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
  const resolved = body.runs.filter((r) => r.state === 'resolved').length;
  const working = body.runs.filter(
    (r) => r.state === 'working' || r.state === 'grading' || r.state === 'queued',
  ).length;
  // Terminal failures are HISTORY, not an alarm — they get their own stat.
  // `stalled` (live-but-silent) is the true alarm and renders as a banner
  // only when it exists; 17 ancient failed runs must never read as "17
  // currently stalled" (the live-feed first-render lesson, 2026-08-12).
  const failed = body.runs.filter((r) => r.state === 'failed').length;
  const stalled = body.runs.filter((r) => r.state === 'stalled').length;
  const maxGens = Math.max(...body.runs.map((r) => r.generations));
  return html`<div class="bench-board">
    ${body.feedLive ? nothing : html`<div class="bench-snapshot-banner">snapshot — no live feed attached</div>`}
    <div class="bench-score" role="group" aria-label="round scoreboard">
      <div class="bench-stat bench-stat-resolved">
        <span class="bench-stat-n">${resolved}</span><span class="bench-stat-l">resolved</span>
      </div>
      <div class="bench-stat bench-stat-working">
        <span class="bench-stat-n">${working}</span><span class="bench-stat-l">working</span>
      </div>
      <div class="bench-stat bench-stat-failed">
        <span class="bench-stat-n">${failed}</span><span class="bench-stat-l">failed</span>
      </div>
    </div>
    ${stalled > 0
      ? html`<div class="bench-stall-banner" role="alert">
          ⚠ ${stalled} run${stalled === 1 ? '' : 's'} gone quiet — live but no artifact activity</div>`
      : nothing}
    ${body.lanePressure
      ? html`<div class="bench-lanes" title="serving lanes vs lanes of demand — contention at a glance">
          lanes ${body.lanePressure.serving} serving · ${body.lanePressure.demanding} demanding</div>`
      : nothing}
    ${body.runs.map((r) => runCard(r, maxGens))}
  </div>`;
}
