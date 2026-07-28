import { describe, expect, it } from 'vitest';
import { arenaBoards, arenaContentBody, type ArenaLedgerRow } from './arenaProjections';

const row = (over: Partial<ArenaLedgerRow>): ArenaLedgerRow => ({
  benchmark: 'humaneval-rs',
  model: 'm',
  arm: 'OURS',
  score: 1,
  total: 2,
  pass_rate: 0.5,
  captured: '2026-07-24',
  machine: 'test-box',
  ...over,
});

describe('arenaProjections', () => {
  // what this catches: the ranking contract — pass rate desc, bigger run wins
  // a rate tie, and EXCLUDED rows sort after every included row while staying
  // VISIBLE (the ledger is an audit surface; exclusion is a state, not
  // deletion). Regression here = a leaderboard that lies about order or hides
  // audited rows.
  it('ranks by pass rate, breaks ties by run size, keeps excluded rows last and visible', () => {
    const boards = arenaBoards([
      row({ model: 'small-run', score: 9, total: 10, pass_rate: 0.9 }),
      row({ model: 'excluded-top', score: 20, total: 20, pass_rate: 1.0, excluded: true }),
      row({ model: 'big-run', score: 18, total: 20, pass_rate: 0.9 }),
      row({ model: 'best', score: 5, total: 5, pass_rate: 1.0 }),
    ]);
    expect(boards).toHaveLength(1);
    const models = boards[0]?.rows.map((r) => r.model);
    expect(models).toEqual(['best', 'big-run', 'small-run', 'excluded-top']);
    expect(boards[0]?.rows[3]?.excluded).toBe(true);
  });

  // what this catches: board grouping + the honest feed flag — boards group by
  // benchmark in ledger order, rowCount is the audit denominator, feedLive is
  // exactly "a live state was attached", and live_run maps through (absent =
  // absent, never a fabricated idle run).
  it('groups by benchmark and maps the live run + feed flag honestly', () => {
    const body = arenaContentBody(
      {
        rows: [row({}), row({ benchmark: 'hard-rs' }), row({ benchmark: 'hard-rs', model: 'x' })],
        live_run: { benchmark: 'hard-rs', model: 'Qwen', done: 5, total: 8, current_task: 't6' },
      },
      true,
    );
    expect(body.boards.map((b) => b.benchmark)).toEqual(['humaneval-rs', 'hard-rs']);
    expect(body.rowCount).toBe(3);
    expect(body.feedLive).toBe(true);
    expect(body.liveRun).toEqual({
      benchmark: 'hard-rs',
      model: 'Qwen',
      done: 5,
      total: 8,
      currentTask: 't6',
    });
    const empty = arenaContentBody({ rows: [] }, false);
    expect(empty.liveRun).toBeUndefined();
    expect(empty.feedLive).toBe(false);
    expect(empty.boards).toEqual([]);
  });
});
