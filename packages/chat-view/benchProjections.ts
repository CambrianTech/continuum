/**
 * kind="bench" → bench-board projections (#329: a benchmark IS a live room).
 *
 * Folds the core-carried `BenchViewState` (the positron bench source's wire
 * payload — the ONE run-ledger projection) onto the neutral `BenchContentBody`
 * the board renderer draws, plus the right-rail `PanelWidget` face for the
 * academy room. Same adapt-don't-recompute discipline as `servingWidget`:
 * the core owns scanning, phase derivation, and grade folding; here we only
 * reshape wire vocabulary to widget vocabulary — honestly. A fact the wire
 * doesn't carry stays ABSENT (editActs, self-claimed marking), never a
 * fabricated zero.
 */

import type { BenchViewState, BenchRoundRow, BenchRunRow } from '@continuum/sdk-typescript';
import type {
  BenchContentBody,
  BenchRoundVM,
  BenchRunVM,
  BenchRunState,
  BenchVerdictVM,
} from '@continuum/patterns';
import type { PanelWidget } from '@continuum/patterns';

/** Parse the wire's render-ready "passed/total" ratio ("38/40") — the grader
 *  formats it; we only split. Malformed → null (absence over fabrication). */
function ratio(s: string | undefined): { passed: number; total: number } | null {
  if (!s) return null;
  const m = /^(\d+)\/(\d+)$/.exec(s);
  if (!m) return null;
  return { passed: Number(m[1]), total: Number(m[2]) };
}

/** Wire phase → board row state. `queued` is derived: an `active` row that has
 *  not completed a single act is an attempt that started but hasn't generated —
 *  the exact 50-minutes-of-ambiguity the state split exists to kill. */
function stateOf(row: BenchRunRow): BenchRunState {
  switch (row.phase) {
    case 'resolved':
      return 'resolved';
    case 'failed':
      return 'failed';
    case 'quiet':
      return 'stalled';
    case 'queued':
      // The core's cross-card verdict: silent, but the solver's hands are
      // busy on another run — waiting a turn, never an alarm.
      return 'queued';
    default:
      return (row.acts ?? 0) > 0 ? 'working' : 'queued';
  }
}

/** A verdict VM exists only when a grade actually landed (f2p present). */
function verdictOf(row: BenchRunRow): BenchVerdictVM | undefined {
  const f2p = ratio(row.fail_to_pass);
  if (!f2p) return undefined;
  const p2p = ratio(row.pass_to_pass) ?? { passed: 0, total: 0 };
  return {
    resolved: row.resolved === true,
    f2pPassed: f2p.passed,
    f2pTotal: f2p.total,
    p2pPassed: p2p.passed,
    p2pTotal: p2p.total,
    // The alarm: previously-passing tests broke under the patch.
    regression: p2p.total > 0 && p2p.passed < p2p.total,
    failedTests: row.failed_tests,
  };
}

/** A raw UUID (or uuid-suffixed run id) is an ID, not a NAME — the core resolves
 *  live personas to display names; anything still uuid-shaped here compacts to
 *  its 8-char short form (the same short-id vocabulary the rest of the system
 *  speaks, #161) so the board never spends a row-width on 36 hex chars. */
const UUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;
function compactId(s: string): string {
  return s.replace(UUID_RE, (u) => u.slice(0, 8));
}

function runVM(row: BenchRunRow): BenchRunVM {
  const acts = row.acts ?? 0;
  return {
    runId: row.run_id,
    ...(row.round_id !== undefined ? { roundId: row.round_id } : {}),
    ...(row.solve_room !== undefined ? { roomId: row.solve_room } : {}),
    ...(row.solve_room_name !== undefined ? { roomName: row.solve_room_name } : {}),
    // The board names WHAT when the ledger carries it; a non-SWE run is
    // honestly identified by its run id, never a guessed instance.
    instance: compactId(row.instance ?? row.run_id),
    persona: compactId(row.solver ?? 'unclaimed'),
    // The wire doesn't yet distinguish self-claimed from operator-launched;
    // unmarked until it does (mark only what is KNOWN).
    selfClaimed: false,
    attempt: row.attempt ?? 1,
    maxAttempts: row.max_attempts ?? 1,
    state: stateOf(row),
    generations: acts,
    // Artifact-age pulse; null before the first act = "no generations yet".
    lastGenAgeS: acts > 0 ? row.age_secs : null,
    patchBytes: row.patch_bytes ?? null,
    verdict: verdictOf(row),
    // editActs deliberately ABSENT — kind="bench" v1 doesn't carry the
    // edit/discovery split; the renderer hides the count.
  };
}

/** Wire round → scoreboard VM. Adapt, never recompute: settled/remaining are
 *  the round tracker's own state (#371), not client-side arithmetic. */
function roundVM(row: BenchRoundRow): BenchRoundVM {
  return {
    roundId: compactId(row.round_id),
    rawId: row.round_id,
    benchmark: row.benchmark,
    stage: row.stage,
    dispatched: row.dispatched,
    settled: row.settled,
    remaining: row.remaining,
    driver: row.driver,
  };
}

/** The bench board content body — `feedLive` is true only when the bench
 *  subscription has actually delivered (same contract as serving/arena). */
export function benchContentBody(view?: BenchViewState): BenchContentBody {
  return {
    runs: (view?.runs ?? []).map(runVM),
    // Older wires predate `rounds` — fold absent to empty, the honest frame.
    rounds: (view?.rounds ?? []).map(roundVM),
    feedLive: view !== undefined,
  };
}

/** The academy right-rail BENCH widget — joins the rail once the feed
 *  delivers AND carries at least one non-terminal-stale row worth watching;
 *  `undefined` before that (honest absence, the rail never shows an empty
 *  frame for a node that isn't benchmarking). */
export function benchWidget(view?: BenchViewState): PanelWidget<BenchContentBody> | undefined {
  if (!view || view.runs.length === 0) return undefined;
  return {
    id: 'bench',
    kind: 'bench',
    title: 'Benchmarks',
    body: benchContentBody(view),
    scope: 'global',
  };
}
