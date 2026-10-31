import { describe, expect, it } from 'vitest';
import { HISTORY_PAGE_ROWS, RoomHistory, type HistoryCursor } from './roomHistory';

const id = (i: number): string => `m${String(i).padStart(6, '0')}`;
const index = (rowId: string): number => Number(rowId.slice(1));

/** A room's store of `n` rows, answering cursors the way chat/history does. `grow`
 *  appends live rows, as the room does while the reader reads. */
function store(n: number) {
  let total = n;
  const calls: HistoryCursor[] = [];
  const row = (i: number) => ({
    id: id(i),
    senderId: 's',
    timestamp: new Date(Date.UTC(2026, 9, 26) + i * 1000).toISOString(),
    content: { text: `line ${i}` },
  });
  const fetch = (cursor: HistoryCursor, limit: number): Promise<readonly unknown[]> => {
    calls.push(cursor);
    let from: number;
    let to: number;
    if (cursor.afterMessageId !== undefined) {
      from = index(cursor.afterMessageId) + 1;
      to = Math.min(total, from + limit);
    } else {
      to = cursor.beforeMessageId !== undefined ? index(cursor.beforeMessageId) : total;
      from = Math.max(0, to - limit);
    }
    return Promise.resolve(Array.from({ length: Math.max(0, to - from) }, (_, k) => row(from + k)));
  };
  /** The live window: the newest 100 rows. */
  const live = () => Array.from({ length: Math.min(100, total) }, (_, k) => total - Math.min(100, total) + k).map(id);
  const grow = () => {
    total += 1;
  };
  return { fetch, calls, live, grow };
}

const vmRow = (rowId: string) => ({ id: rowId, senderId: 's', senderName: 's', kind: 'agent' as const, content: '', time: '', runtime: '' });

/** The rendered window a reader at the top (or bottom) of the span sees: three
 *  viewports of rows, as transcriptWindow renders them. */
const WINDOW_ROWS = 36;
const BOUND = WINDOW_ROWS + 2 * HISTORY_PAGE_ROWS;

describe('RoomHistory', () => {
  // what this catches: scroll-back that refetches the newest page forever. Each page
  // must ask with the oldest row held as its cursor, so successive pages walk back
  // through the room without overlap and stop on the short page at its start.
  it('pages back by the oldest held row until a short page', async () => {
    const s = store(HISTORY_PAGE_ROWS * 2 + 7);
    const h = new RoomHistory();
    const live = s.live();
    let pages = 0;
    while (h.canPageOlder) {
      await h.pageOlder(live[0], s.fetch, [], new Set(live));
      pages += 1;
    }
    // 7 rows older than the live 100: one short page.
    expect(pages).toBe(1);
    expect(s.calls).toEqual([{ beforeMessageId: live[0] }]);
    expect(h.rows.map((r) => r.id)).toEqual(Array.from({ length: 7 }, (_, i) => id(i)));
    expect(h.atStart).toBe(true);
  });

  // what this catches: memory growing with how far back the reader scrolls. Over the
  // whole of a 40,000-row room (hundreds of pages up, then all the way back down),
  // the span never holds more than the window plus a page each side; it detaches
  // from the live window on the way up and rejoins it exactly at the present, with
  // every page adjacent to the last (no gap, no overlap).
  it('stays bounded over a prolonged backscroll and the way back down', async () => {
    const s = store(40_000);
    const h = new RoomHistory();
    const live = s.live();
    let max = 0;
    let up = 0;
    while (h.canPageOlder) {
      await h.pageOlder(live[0], s.fetch, [], new Set(live));
      h.trimAround(0, WINDOW_ROWS);
      max = Math.max(max, h.rows.length);
      up += 1;
    }
    expect(up).toBeGreaterThan(700);
    expect(max).toBeLessThanOrEqual(BOUND);
    expect(h.rows[0]?.id).toBe(id(0));
    expect(h.joined).toBe(false);

    let down = 0;
    while (!h.joined) {
      const last = h.rows.at(-1)?.id ?? '';
      await h.pageNewer(new Set(live), s.fetch, []);
      const at = h.rows.findIndex((r) => r.id === last);
      expect(h.rows[at + 1]?.id ?? id(index(last) + 1)).toBe(id(index(last) + 1));
      h.trimAround(h.rows.length - WINDOW_ROWS, h.rows.length);
      max = Math.max(max, h.rows.length);
      down += 1;
    }
    expect(down).toBeGreaterThan(700);
    expect(max).toBeLessThanOrEqual(BOUND);
    expect(index(h.rows.at(-1)?.id ?? '')).toBe(index(live[0] ?? '') - 1);
  });

  // what this catches: a reader parked on an old row while the room keeps talking.
  // Detached, live arrivals are counted, never retained; jumping to the present
  // drops the span and the count.
  it('counts live arrivals while parked far up instead of retaining them', async () => {
    const s = store(10_000);
    const h = new RoomHistory();
    for (let i = 0; i < 5; i += 1) {
      const live = s.live();
      await h.pageOlder(live[0], s.fetch, [], new Set(live));
    }
    h.trimAround(0, WINDOW_ROWS);
    expect(h.joined).toBe(false);
    const held = h.rows.length;
    for (let i = 0; i < 5_000; i += 1) {
      const before = s.live();
      s.grow();
      const now = s.live();
      h.retire(before.filter((r) => !now.includes(r)).map(vmRow), 1);
    }
    expect(h.rows.length).toBe(held);
    expect(h.unseen).toBe(5_000);
    h.jumpToPresent();
    expect([h.rows.length, h.unseen, h.joined]).toEqual([0, 0, true]);
  });

  // what this catches: a page landing after the reader left the room (A, B, A). The
  // response belongs to a visit that is over: it must not touch the buffer, and the
  // new visit can page at once rather than wait for it.
  it('drops a response from an earlier visit', async () => {
    const s = store(HISTORY_PAGE_ROWS * 4);
    const h = new RoomHistory();
    let release: (rows: readonly unknown[]) => void = () => undefined;
    const slow = (): Promise<readonly unknown[]> =>
      new Promise((resolve) => {
        release = resolve;
      });
    const first = h.pageOlder(undefined, slow, [], new Set());
    expect(h.canPageOlder).toBe(false);
    h.leave();
    expect(h.canPageOlder).toBe(true);
    const fresh = h.pageOlder(undefined, s.fetch, [], new Set());
    release([{ id: 'stale', senderId: 's', timestamp: '2026-10-26T00:00:00Z', content: { text: 'stale' } }]);
    expect(await first).toBe(0);
    expect(await fresh).toBe(HISTORY_PAGE_ROWS);
    expect(h.rows.some((r) => r.id === 'stale')).toBe(false);
  });

  // what this catches: a gap between history and the live window. Rows leaving the
  // live window join the span's newest end once, and never while it holds nothing.
  it('retires rows leaving the live window onto its newest end', async () => {
    const s = store(HISTORY_PAGE_ROWS * 3);
    const h = new RoomHistory();
    h.retire([vmRow('x')], 1);
    expect(h.rows).toHaveLength(0);
    await h.pageOlder(undefined, s.fetch, [], new Set());
    h.retire([vmRow('x'), vmRow('typing:s')], 1);
    h.retire([vmRow('x')], 1);
    expect(h.rows.map((r) => r.id).slice(-2)).toEqual([id(HISTORY_PAGE_ROWS * 3 - 1), 'x']);
  });
});
