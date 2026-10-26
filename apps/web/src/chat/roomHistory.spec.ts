import { describe, expect, it } from 'vitest';
import { HISTORY_PAGE_ROWS, RoomHistory } from './roomHistory';

/** A store of `n` rows, oldest first, answering the cursor the way chat/history does. */
function store(n: number) {
  const rows = Array.from({ length: n }, (_, i) => ({
    id: `m${String(i).padStart(4, '0')}`,
    senderId: 's',
    timestamp: new Date(Date.UTC(2026, 9, 26, 0, 0, i)).toISOString(),
    content: { text: `line ${i}` },
  }));
  const calls: (string | undefined)[] = [];
  const fetch = (before: string | undefined, limit: number): Promise<readonly unknown[]> => {
    calls.push(before);
    const end = before === undefined ? rows.length : rows.findIndex((r) => r.id === before);
    return Promise.resolve(rows.slice(Math.max(0, end - limit), end));
  };
  return { rows, calls, fetch };
}

describe('RoomHistory', () => {
  // what this catches: scroll-back that refetches the newest page forever. Each page
  // must ask with the oldest row held as its cursor, so successive pages walk back
  // through the room without overlap and stop on the short page at its start.
  it('pages back by the oldest held row until a short page', async () => {
    const s = store(HISTORY_PAGE_ROWS * 2 + 7);
    const h = new RoomHistory();
    const live = s.rows.slice(-10).map((r) => r.id);
    let pages = 0;
    while (h.canPage) {
      await h.pageOlder(live[0], s.fetch, [], new Set(live));
      pages += 1;
    }
    // 97 rows older than the live 10: a full page, then a short one.
    expect(pages).toBe(2);
    expect(s.calls).toEqual([live[0], s.rows[97 - HISTORY_PAGE_ROWS]?.id]);
    expect(h.rows.map((r) => r.id)).toEqual(s.rows.slice(0, -10).map((r) => r.id));
    expect(h.exhausted).toBe(true);
  });

  // what this catches: a page landing after the reader left the room. The page fills
  // the buffer that asked for it, and a second request while one is in flight is not
  // sent (the scroll handler fires every frame).
  it('allows one request in flight and fills the room that asked', async () => {
    const s = store(HISTORY_PAGE_ROWS * 3);
    const asked = new RoomHistory();
    const first = asked.pageOlder(undefined, s.fetch, [], new Set());
    expect(asked.canPage).toBe(false);
    expect(await asked.pageOlder(undefined, s.fetch, [], new Set())).toBe(0);
    await first;
    expect(s.calls).toHaveLength(1);
    expect(asked.rows).toHaveLength(HISTORY_PAGE_ROWS);
  });

  // what this catches: an unbounded buffer. Rows more than a page above the rendered
  // window are dropped, and dropping reopens paging so the reader can scroll back to them.
  it('drops rows far above the window and reopens paging', async () => {
    const s = store(HISTORY_PAGE_ROWS * 2);
    const h = new RoomHistory();
    await h.pageOlder(undefined, s.fetch, [], new Set());
    await h.pageOlder(undefined, s.fetch, [], new Set());
    await h.pageOlder(undefined, s.fetch, [], new Set());
    expect(h.exhausted).toBe(true);
    const dropped = h.trimAbove(HISTORY_PAGE_ROWS + 20);
    expect(dropped).toBe(20);
    expect(h.rows[0]?.id).toBe(s.rows[20]?.id);
    expect(h.exhausted).toBe(false);
    expect(h.trimAbove(HISTORY_PAGE_ROWS)).toBe(0);
  });

  // what this catches: a gap between history and the live window. Rows leaving the
  // live window join the buffer's newest end once, and never while it holds nothing.
  it('retires rows leaving the live window onto its newest end', async () => {
    const s = store(HISTORY_PAGE_ROWS);
    const h = new RoomHistory();
    const row = (id: string) => ({ id, senderId: 's', senderName: 's', kind: 'agent' as const, content: '', time: '', runtime: '' });
    h.retire([row('x')]);
    expect(h.rows).toHaveLength(0);
    await h.pageOlder(undefined, s.fetch, [], new Set());
    h.retire([row('x'), row('typing:s')]);
    h.retire([row('x')]);
    expect(h.rows.map((r) => r.id).slice(-2)).toEqual([s.rows.at(-1)?.id, 'x']);
  });
});
