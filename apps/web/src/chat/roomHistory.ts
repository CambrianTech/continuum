/**
 * One room's scroll-back buffer: the rows older than the live window, oldest first.
 *
 * Paging works the way Discord's does: the newest page on open, live rows appended
 * by the state stream, older pages fetched by a message cursor (`beforeMessageId` =
 * the oldest row held) as the reader scrolls up, and a bounded buffer: rows far
 * above the reader are dropped and paged back in if the reader returns to them.
 *
 * The buffer is per room and owns its own latches, so a page that lands after the
 * reader switched rooms fills the room that asked for it, never the one on screen.
 */
import { historyRowsFromPoll, type MessageRowVM, type RosterMemberVM } from '@continuum/chat-view';

/** Rows per scroll-back page: the core's declared `chat/history` default page. */
export const HISTORY_PAGE_ROWS = 50;

export class RoomHistory {
  private _rows: MessageRowVM[] = [];
  private _exhausted = false;
  private _loading = false;

  /** Oldest first. Reassigned, never mutated, so a holder can compare by identity. */
  get rows(): readonly MessageRowVM[] {
    return this._rows;
  }

  /** The server returned a short page: nothing older exists. */
  get exhausted(): boolean {
    return this._exhausted;
  }

  /** A page request is in flight; at most one per room. */
  get loading(): boolean {
    return this._loading;
  }

  /** Whether a page should be requested now. */
  get canPage(): boolean {
    return !this._loading && !this._exhausted;
  }

  /** The cursor for the next older page: the oldest row held, else the oldest live
   *  row; `undefined` (an empty room on screen) asks for the newest page. */
  anchor(oldestLiveId: string | undefined): string | undefined {
    return this._rows[0]?.id ?? oldestLiveId;
  }

  /** Fetch one older page and prepend it. `fetch` is given the cursor and the page
   *  size; a page shorter than asked latches `exhausted`. Returns the rows added. */
  async pageOlder(
    oldestLiveId: string | undefined,
    fetch: (beforeMessageId: string | undefined, limit: number) => Promise<readonly unknown[]>,
    members: readonly RosterMemberVM[],
    onScreen: ReadonlySet<string>,
  ): Promise<number> {
    if (!this.canPage) return 0;
    this._loading = true;
    try {
      const page = await fetch(this.anchor(oldestLiveId), HISTORY_PAGE_ROWS);
      if (page.length < HISTORY_PAGE_ROWS) this._exhausted = true;
      const held = new Set([...onScreen, ...this._rows.map((r) => r.id)]);
      const rows = historyRowsFromPoll(page, members, held);
      if (rows.length > 0) this._rows = [...rows, ...this._rows];
      return rows.length;
    } finally {
      this._loading = false;
    }
  }

  /** Rows that slid out of the live window join the buffer's newest end, so no gap
   *  opens between scrolled-back history and the live rows. Only while the buffer
   *  holds history: with none, the live window is the whole transcript on screen. */
  retire(leftLive: readonly MessageRowVM[]): void {
    if (this._rows.length === 0) return;
    const held = new Set(this._rows.map((r) => r.id));
    const fresh = leftLive.filter((r) => !held.has(r.id) && !r.id.startsWith('typing:'));
    if (fresh.length > 0) this._rows = [...this._rows, ...fresh];
  }

  /** Bound the buffer by the reader's position: keep one page above the first
   *  rendered row (`windowStart`, a transcript index; history rows lead the
   *  transcript) so a reader nudging upward never waits on a refetch, and drop
   *  what lies further up. Returns how many rows were dropped. */
  trimAbove(windowStart: number): number {
    const drop = Math.min(this._rows.length, windowStart - HISTORY_PAGE_ROWS);
    return drop > 0 ? this.dropOldest(this._rows.length - drop) : 0;
  }

  /** Drop the oldest rows beyond `keep`. Dropped rows are paged back in by the
   *  cursor if the reader scrolls to them, so `exhausted` reopens. Returns how many
   *  rows were dropped (the caller shifts its window by that much). */
  dropOldest(keep: number): number {
    const drop = this._rows.length - Math.max(0, keep);
    if (drop <= 0) return 0;
    this._rows = this._rows.slice(drop);
    this._exhausted = false;
    return drop;
  }
}
