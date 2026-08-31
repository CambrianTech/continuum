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
import type { BenchContentBody, BenchRoundVM, BenchRunVM, BenchVerdictVM } from '@continuum/patterns';

/** One in-flight ROUND row — the tracker's own lifecycle truth (#371), not a
 *  client-side count over run rows. settled/dispatched IS the round's progress. */
function roundRow(round: BenchRoundVM): TemplateResult {
  const pct =
    round.dispatched > 0 ? Math.round((round.settled / round.dispatched) * 100) : 0;
  return html`<div class="bench-round" data-stage=${round.stage}>
    <span class="bench-round-name" title=${round.roundId}>${round.benchmark}</span>
    <span class="bench-round-stage${round.stage === 'working' ? ' wave-active' : ''}">${round.stage}</span>
    <span class="bench-round-count" title="cards settled / dispatched">
      ${round.settled}/${round.dispatched}</span>
    <div class="bench-bar" role="progressbar" aria-label="round settle progress">
      <div class="bench-bar-fill" style="width:${pct}%"></div>
    </div>
    ${round.driver === 'citizen'
      ? html`<span class="bench-round-driver" title="worked in the room — turns feed the curriculum">citizens</span>`
      : html`<span class="bench-round-driver bench-round-detached" title="detached solve — produces no room turns">detached</span>`}
    ${round.stage === 'working'
      ? html`<button
          class="bench-round-ctl"
          title="hold this round — in-flight solves finish, no new card fires until resume"
          @click=${(e: Event) => emitRoundControl(e, 'pause', round.roundId)}
        >⏸</button>`
      : round.stage === 'paused'
        ? html`<button
            class="bench-round-ctl bench-round-ctl-resume"
            title="lift the hold — the driver fires the next card immediately"
            @click=${(e: Event) => emitRoundControl(e, 'resume', round.roundId)}
          >▶</button>`
        : nothing}
  </div>`;
}

/** Round pause/resume, as a composed event — the renderer stays pure; the
 *  host (which owns the transport) binds `bench-round-control` to
 *  `benchmark/pause` / `benchmark/resume`. Same seam discipline as
 *  historyHandler: rendering never talks to the wire directly. */
function emitRoundControl(e: Event, action: 'pause' | 'resume', roundId: string): void {
  e.stopPropagation();
  (e.currentTarget as HTMLElement).dispatchEvent(
    new CustomEvent('bench-round-control', {
      detail: { action, roundId },
      bubbles: true,
      composed: true,
    }),
  );
}

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

/** Open the run's activity room — the composed-event seam (same discipline as
 *  `emitRoundControl`): the renderer stays pure, the host binds navigation. */
function emitRunOpen(e: Event, roomId: string, roomName?: string): void {
  (e.currentTarget as HTMLElement).dispatchEvent(
    new CustomEvent('bench-run-open', {
      detail: { roomId, roomName },
      bubbles: true,
      composed: true,
    }),
  );
}

function runCard(run: BenchRunVM, maxGens: number): TemplateResult {
  // Acts bar normalized to the board's busiest run — relative shape, no
  // invented budget denominator (the wire carries no per-run act budget).
  const pct = maxGens > 0 ? Math.max(4, Math.round((run.generations / maxGens) * 100)) : 0;
  const door = run.roomId;
  return html`<div
    class="bench-card bench-state-${run.state}"
    ?data-door=${door !== undefined}
    title=${door !== undefined ? 'open this run\'s room — watch the work live' : nothing}
    tabindex=${door !== undefined ? '0' : nothing}
    @click=${door !== undefined ? (e: Event): void => emitRunOpen(e, door, run.roomName) : nothing}
    @keydown=${door !== undefined
      ? (e: KeyboardEvent): void => {
          if (e.key === 'Enter') emitRunOpen(e, door, run.roomName);
        }
      : nothing}
  >
    <div class="bench-card-head">
      <span class="bench-dot" title=${run.state}></span>
      <span
        class="bench-instance${run.state === 'working' || run.state === 'grading' ? ' wave-active' : ''}"
        title=${run.runId}
      >${run.instance}</span>
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
  // Rounds render even with zero run rows: a freshly staged round has no run
  // ledger yet, and the board saying "nothing here" while the tracker holds a
  // working round would be the launch-and-pray blindness this region kills.
  const rounds =
    body.rounds.length > 0
      ? html`<div class="bench-rounds" role="group" aria-label="in-flight rounds">
          ${body.rounds.map(roundRow)}
        </div>`
      : nothing;
  if (body.runs.length === 0) {
    return html`<div class="bench-board">
      ${rounds}
      <div class="bench-awaiting">
        <p>No benchmark runs on this board yet.</p>
        <p class="bench-awaiting-sub">Rows appear when a run starts — operator-launched or claimed by a citizen. The frame is the promise.</p>
      </div>
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
  // GROUPED NAVIGATION (Joel, 2026-08-31: "how do we navigate and group...
  // it's kind of confusing"): each round is a SECTION holding its own runs —
  // live work first, settled history folded away. Runs that predate round
  // linkage keep an ungrouped bucket at the tail; nothing is hidden.
  const LIVE_STATES = new Set(['working', 'grading', 'queued', 'stalled']);
  const orderInside = (a: BenchRunVM, b: BenchRunVM): number =>
    Number(LIVE_STATES.has(b.state)) - Number(LIVE_STATES.has(a.state));
  const runsOf = (rawId?: string): BenchRunVM[] =>
    body.runs.filter((r) => r.roundId !== undefined && r.roundId === rawId).sort(orderInside);
  const grouped = new Set(
    body.rounds.flatMap((rd) => runsOf(rd.rawId).map((r) => r.runId)),
  );
  const loose = body.runs.filter((r) => !grouped.has(r.runId)).sort(orderInside);
  const section = (runs: BenchRunVM[]): TemplateResult => {
    const live = runs.filter((r) => LIVE_STATES.has(r.state));
    const done = runs.filter((r) => !LIVE_STATES.has(r.state));
    return html`${live.map((r) => runCard(r, maxGens))}
    ${done.length > 0
      ? html`<details class="bench-history">
          <summary>${done.length} settled</summary>
          ${done.map((r) => runCard(r, maxGens))}
        </details>`
      : nothing}`;
  };
  return html`<div class="bench-board">
    ${body.feedLive ? nothing : html`<div class="bench-snapshot-banner">snapshot — no live feed attached</div>`}
    <div class="bench-score" role="group" aria-label="run scoreboard">
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
    ${body.rounds.map(
      (rd) => html`<section class="bench-round-group">
        ${roundRow(rd)}
        ${section(runsOf(rd.rawId))}
      </section>`,
    )}
    ${loose.length > 0
      ? html`<section class="bench-round-group bench-ungrouped">${section(loose)}</section>`
      : nothing}
  </div>`;
}
