/**
 * renderBench (Lit) unit spec — the bench board's render seam.
 *
 * Same discipline as renderChat.spec: DOM-free, flatten the template tree and
 * assert every model fact reaches the markup. The fixture is NOT invented —
 * every row is cut from 2026-08-08's REAL receipts (probe stream + captures),
 * which makes this spec double as the first recorded-stream regression
 * artifact for the universe seam ([[universes-are-positron-asset-payloads]]):
 * a future universe renders THIS SAME body and diffs against its own snapshot.
 *
 * The three facts that must never regress:
 *  1. REGRESSION leads visibly when a patch broke pass-to-pass tests
 *     (the hidden-collateral lesson, atlas-24066-n7).
 *  2. A run before its first generation says "no generations yet" — queued is
 *     never dressed as work (the 50-minute ambiguity, n8/n11 launch hour).
 *  3. A citizen-claimed run is marked on the same board as operator runs
 *     (the invisible-contention lesson, claim-a6d6d166).
 */

import { describe, it, expect } from 'vitest';
import type { BenchContentBody } from '@continuum/patterns';
import { renderBench } from './renderBench';

/** Flatten a Lit TemplateResult tree into its static strings + stringified
 *  interpolations (the renderChat.spec technique — no DOM, no SSR). */
function flatten(node: unknown, out: string[] = []): string[] {
  if (node == null || node === false) return out;
  if (Array.isArray(node)) {
    for (const child of node as readonly unknown[]) flatten(child, out);
    return out;
  }
  if (typeof node === 'object' && 'strings' in (node as object) && 'values' in (node as object)) {
    // Interleave statics with their interpolations so ORDER survives —
    // template semantics are strings[0] v[0] strings[1] v[1] … strings[n].
    const t = node as { strings: readonly string[]; values: readonly unknown[] };
    for (let i = 0; i < t.values.length; i++) {
      out.push(t.strings[i] ?? '');
      flatten(t.values[i], out);
    }
    out.push(t.strings[t.strings.length - 1] ?? '');
    return out;
  }
  out.push(String(node));
  return out;
}

/** 2026-08-08's real hour, as a body: Atlas's regression round terminal,
 *  Benchy's harmless-patch round terminal, the n8 relaunch queued behind the
 *  citizen's self-claimed flask run — under 2-serving/4-demanding pressure. */
const REAL_HOUR: BenchContentBody = {
  feedLive: true,
  lanePressure: { serving: 2, demanding: 4 },
  runs: [
    {
      runId: 'atlas-sympy-24066-n8',
      instance: 'sympy__sympy-24066',
      persona: 'Atlas',
      selfClaimed: false,
      attempt: 1,
      maxAttempts: 3,
      state: 'queued',
      generations: 0,
      lastGenAgeS: null,
      editActs: 0,
      patchBytes: null,
    },
    {
      runId: 'claim-a6d6d166-a2c5-4350-94eb-7b6f420dc945',
      instance: 'flask-4045',
      persona: 'Casper',
      selfClaimed: true,
      attempt: 3,
      maxAttempts: 3,
      state: 'failed',
      generations: 12,
      lastGenAgeS: 95,
      editActs: 2,
      patchBytes: 0,
      verdict: {
        resolved: false,
        f2pPassed: 0,
        f2pTotal: 2,
        p2pPassed: 30,
        p2pTotal: 30,
        regression: false,
        failedTests: ['tests/test_cli.py::TestRoutes::test_host'],
      },
    },
    {
      runId: 'atlas-sympy-24066-n7',
      instance: 'sympy__sympy-24066',
      persona: 'Atlas',
      selfClaimed: false,
      attempt: 3,
      maxAttempts: 3,
      state: 'failed',
      generations: 41,
      lastGenAgeS: 3600,
      editActs: 3,
      patchBytes: 2003,
      verdict: {
        resolved: false,
        f2pPassed: 0,
        f2pTotal: 1,
        p2pPassed: 0,
        p2pTotal: 30,
        regression: true,
        failedTests: ['test_Quantity_definition', 'test_issue_24062'],
      },
    },
  ],
};

describe('renderBench', () => {
  it('renders every fact of the real 2026-08-08 hour, with regression leading as an alarm', () => {
    const text = flatten(renderBench(REAL_HOUR)).join(' ');

    // NOTE: flattened Lit templates separate interpolations from static strings,
    // so adjacency is asserted whitespace-tolerantly (\s+), never as substrings.

    // fact 1 — the regression alarm: count of broken p2p, alarm class, names.
    expect(text).toContain('REGRESSION');
    expect(text).toMatch(/30\s+broken/); // p2pTotal 30 - p2pPassed 0
    expect(text).toContain('bench-alarm');
    expect(text).toContain('test_Quantity_definition');

    // fact 2 — queued honesty: the relaunched run shows its truth, not a pulse.
    expect(text).toContain('no generations yet');
    expect(text).toMatch(/bench-state-\s*queued/);

    // fact 3 — the self-claimed run is on the SAME board, marked.
    expect(text).toContain('self-claimed');
    expect(text).toContain('flask-4045');
    expect(text).toContain('Casper');

    // progress facts reach markup: gens+age (compacted), patch bytes, attempts, counts.
    expect(text).toMatch(/41\s+gens/);
    expect(text).toMatch(/1h0m\s+ago/); // 3600s compacts to h+m for board legibility
    expect(text).toContain('2003');
    expect(text).toMatch(/3\s*<i>\s*\/\s*<\/i>\s*3/); // attempt 3/3 chip
    expect(text).toMatch(/f2p\s+<b>\s*0\s*\/\s*2/);

    // the SCOREBOARD header: 0 resolved / 1 working (queued counts — an attempt
    // in flight) / 2 FAILED. Terminal failures are history with their own stat,
    // never dressed as a live stall; no runs are quiet → no alarm banner.
    expect(text).toContain('bench-score');
    expect(text).toMatch(/bench-stat-resolved[\s\S]*?0/);
    expect(text).toMatch(/bench-stat-failed[\s\S]*?2/);
    expect(text).not.toContain('bench-stall-banner');

    // the acts progress bar: the busiest run (41 gens) fills to 100%.
    expect(text).toContain('bench-bar-fill');
    expect(text).toMatch(/width:\s*100\s*%/);

    // lane pressure — the contention that took probe archaeology to see.
    expect(text).toMatch(/2\s+serving/);
    expect(text).toMatch(/4\s+demanding/);

    // no fabricated verdict on the queued run: exactly two verdict cells.
    expect(text.split('class="bench-verdict"').length - 1).toBe(2);
  });

  it('raises the stall ALARM banner only for live-but-silent runs, never for terminal failures', () => {
    // what this catches: 17 ancient failed runs reading as "17 stalled" (the
    // live-feed first-render defect, 2026-08-12) — failed is history, stalled alarms.
    const withStall = flatten(
      renderBench({
        feedLive: true,
        runs: [
          { ...REAL_HOUR.runs[2]!, runId: 'stall-1', state: 'stalled', verdict: undefined },
        ],
      }),
    ).join(' ');
    expect(withStall).toContain('bench-stall-banner');
    expect(withStall).toMatch(/1\s+run\s+gone quiet/);
  });

  it('renders the awaiting frame on an empty board and the snapshot banner off-feed', () => {
    const empty = flatten(renderBench({ runs: [], feedLive: false })).join(' ');
    expect(empty).toContain('No benchmark runs');
    expect(empty).toContain('frame is the promise');

    const snapshot = flatten(
      renderBench({ ...REAL_HOUR, feedLive: false }),
    ).join(' ');
    expect(snapshot).toContain('snapshot — no live feed attached');
  });
});
