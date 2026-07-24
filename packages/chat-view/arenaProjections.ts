/**
 * Arena projections — REAL eval-ledger rows → the neutral `ArenaContentBody`.
 *
 * Pure functions, target-free: the same projection feeds web, tui, and RAG.
 * The input row shape mirrors benchmarks/RESULTS.jsonl verbatim (snake_case
 * wire keys) — the core-side arena feed emits those rows unchanged, so this is
 * the ONE place ledger rows become a ranked face ([[the compression principle]]).
 *
 * Ranking honesty ([[benchmark-numbers-carry-gpu-provenance]]):
 *   - rank by pass rate within a benchmark, ties broken by score (bigger run
 *     wins the tie — 18/20 outranks 9/10 at the same rate);
 *   - `excluded` rows sort AFTER every included row and stay visible — the
 *     ledger is an audit surface; exclusion is a visible state, not deletion.
 */

import type { ArenaBoardVM, ArenaContentBody, ArenaLiveRunVM, ArenaResultRowVM } from '@continuum/patterns';

/** One RESULTS.jsonl row as it rides the wire (snake_case ledger keys). */
export interface ArenaLedgerRow {
  readonly benchmark: string;
  readonly model: string;
  readonly arm: string;
  readonly score: number;
  readonly total: number;
  readonly pass_rate: number;
  readonly captured: string;
  readonly machine: string;
  readonly note?: string | null;
  readonly excluded?: boolean;
}

/** The live-feed envelope state for an arena room (`kind: "arena"`). Mirrors
 *  the Nav/SystemMetrics state pattern: the core's arena source folds ledger +
 *  run events into this snapshot; a projection never reads files. */
export interface ArenaViewState {
  readonly rows: readonly ArenaLedgerRow[];
  readonly live_run?: {
    readonly benchmark: string;
    readonly model: string;
    readonly done: number;
    readonly total: number;
    readonly current_task?: string;
  };
}

/** The envelope kind the arena feed rides on. */
export const ARENA_KIND = 'arena';

function toRowVM(row: ArenaLedgerRow): ArenaResultRowVM {
  return {
    benchmark: row.benchmark,
    model: row.model,
    arm: row.arm,
    score: row.score,
    total: row.total,
    passRate: row.pass_rate,
    captured: row.captured,
    machine: row.machine,
    ...(row.note ? { note: row.note } : {}),
    excluded: row.excluded === true,
  };
}

/** Rank comparator: included before excluded, then pass rate desc, then the
 *  bigger run wins the tie, then model name for stability. */
function rankRows(a: ArenaResultRowVM, b: ArenaResultRowVM): number {
  if (a.excluded !== b.excluded) return a.excluded ? 1 : -1;
  if (b.passRate !== a.passRate) return b.passRate - a.passRate;
  if (b.score !== a.score) return b.score - a.score;
  return a.model.localeCompare(b.model);
}

/** Group + rank ledger rows into per-benchmark boards. Board order = first
 *  appearance in the ledger (stable — the ledger's own narrative order). */
export function arenaBoards(rows: readonly ArenaLedgerRow[]): ArenaBoardVM[] {
  const byBenchmark = new Map<string, ArenaResultRowVM[]>();
  for (const row of rows) {
    const list = byBenchmark.get(row.benchmark);
    if (list) list.push(toRowVM(row));
    else byBenchmark.set(row.benchmark, [toRowVM(row)]);
  }
  return Array.from(byBenchmark.entries(), ([benchmark, boardRows]) => ({
    benchmark,
    rows: boardRows.sort(rankRows),
  }));
}

/** The full arena face from the live-feed state. `feedLive` marks whether a
 *  core envelope stream is attached (vs a static ledger snapshot fixture). */
export function arenaContentBody(state: ArenaViewState, feedLive: boolean): ArenaContentBody {
  const liveRun: ArenaLiveRunVM | undefined = state.live_run
    ? {
        benchmark: state.live_run.benchmark,
        model: state.live_run.model,
        done: state.live_run.done,
        total: state.live_run.total,
        ...(state.live_run.current_task ? { currentTask: state.live_run.current_task } : {}),
      }
    : undefined;
  return {
    boards: arenaBoards(state.rows),
    ...(liveRun ? { liveRun } : {}),
    feedLive,
    rowCount: state.rows.length,
  };
}
