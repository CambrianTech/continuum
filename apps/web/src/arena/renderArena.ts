/**
 * `renderArena` — the web renderer for the benchmark ARENA (`purpose="arena"`).
 *
 * Benchmarks are the show: per-benchmark leaderboards from REAL ledger rows
 * (rank, model+arm, score bar, pass rate, machine provenance, capture date) and
 * the in-flight run strip while an eval is live. Registered for `ARENA_PURPOSE`
 * in the ONE content registry — the same dispatch as chat/foundry/persona/live.
 * Pure fragments: field→element maps of the projected `ArenaContentBody`.
 *
 * Honesty rules: excluded rows render STRUCK (visible audit state, not
 * deletion); provenance and capture date are always shown; no feed → the
 * "ledger snapshot" banner; no rows → the awaiting frame (anti-disappearance:
 * the frame is the promise, data fills it). Colors by theme tokens only.
 */

import { html, nothing, type TemplateResult } from 'lit';
import type { ArenaBoardVM, ArenaContentBody, ArenaLiveRunVM, ArenaResultRowVM } from '@continuum/patterns';

/** One leaderboard row — rank, competitor, score bar, provenance. */
function boardRow(r: ArenaResultRowVM, rank: number): TemplateResult {
  const pct = Math.round(r.passRate * 100);
  return html`<tr class="arena-row" data-excluded=${r.excluded ? '' : nothing} title=${r.note ?? nothing}>
    <td class="a-rank">${r.excluded ? '—' : rank}</td>
    <td class="a-model">
      <span class="a-model-name">${r.model}</span>
      <span class="a-arm" data-arm=${r.arm}>${r.arm}</span>
    </td>
    <td class="a-score">
      <span class="a-score-bar"><span class="a-score-fill" style="width:${pct}%"></span></span>
      <span class="a-score-num">${r.score}/${r.total}</span>
      <span class="a-score-pct">${pct}%</span>
    </td>
    <td class="a-prov" title="hardware provenance">${r.machine}</td>
    <td class="a-date">${r.captured}</td>
  </tr>`;
}

/** One benchmark's leaderboard card. */
function boardCard(board: ArenaBoardVM): TemplateResult {
  return html`<section class="arena-board">
    <div class="a-board-head">
      <span class="a-board-name">${board.benchmark}</span>
      <span class="a-board-count">${board.rows.length} entr${board.rows.length === 1 ? 'y' : 'ies'}</span>
    </div>
    <table class="arena-table">
      <thead>
        <tr>
          <th>#</th>
          <th>competitor</th>
          <th>score</th>
          <th>machine</th>
          <th>captured</th>
        </tr>
      </thead>
      <tbody>
        ${board.rows.map((r, i) => boardRow(r, i + 1))}
      </tbody>
    </table>
  </section>`;
}

/** The in-flight run strip — only rendered while an eval is actually live. */
function liveRunStrip(run: ArenaLiveRunVM): TemplateResult {
  const pct = run.total > 0 ? Math.round((run.done / run.total) * 100) : 0;
  return html`<div class="arena-live-run">
    <span class="a-live-dot"></span>
    <span class="a-live-label">RUNNING</span>
    <span class="a-live-what">${run.model} · ${run.benchmark}</span>
    <span class="a-live-progress">
      <span class="a-live-fill" style="width:${pct}%"></span>
    </span>
    <span class="a-live-count">${run.done}/${run.total}</span>
    ${run.currentTask ? html`<span class="a-live-task">${run.currentTask}</span>` : nothing}
  </div>`;
}

/** The arena face. Every section keeps its frame — awaiting states, never a
 *  vanished surface. */
export function renderArena(body: ArenaContentBody): TemplateResult {
  return html`<div class="arena-home">
    <div class="arena-head">
      <span class="arena-title">ARENA</span>
      ${body.feedLive
        ? html`<span class="a-feed-chip" data-on>live feed</span>`
        : html`<span class="a-feed-chip" title="rendered from the results ledger; the live eval feed attaches when a run streams">ledger snapshot</span>`}
      <span class="a-rowcount">${body.rowCount} ledger rows</span>
    </div>
    ${body.liveRun ? liveRunStrip(body.liveRun) : nothing}
    ${body.boards.length === 0
      ? html`<div class="a-awaiting">Awaiting eval results — the first scored run fills this board.</div>`
      : body.boards.map(boardCard)}
  </div>`;
}
