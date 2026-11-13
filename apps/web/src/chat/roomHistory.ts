/**
 * One room's scroll-back buffer: a contiguous span of rows older than the live
 * window, oldest first, bounded on both sides of the reader.
 *
 * Paging works the way Discord's does: the newest page on open, live rows appended
 * by the state stream, older pages fetched by a message cursor (`beforeMessageId` =
 * the oldest row held) as the reader scrolls up. Retention is a span around the
 * reader: rows a page beyond either edge of the rendered window are dropped and
 * paged back by cursor if the reader returns. Dropping the rows below a reader
 * parked far up DETACHES the span from the live window: live rows are then counted
 * ("N new messages") rather than kept, and the span pages down (`afterMessageId` =
 * the newest row held) until it meets the live window again.
 *
 * Every request carries the generation it was made in; leaving the room bumps the
 * generation, so a response that lands after the reader left (or left and came
 * back) is dropped instead of being spliced into a buffer it no longer fits.
 */
import { historyRowsFromPoll, type MessageRowVM, type RosterMemberVM } from '@continuum/chat-view';

/** Rows per page: the core's declared `chat/history` default page. */
export const HISTORY_PAGE_ROWS = 50;

/** A history request's cursor: at most one of the two (absent both = the newest page). */
export interface HistoryCursor {
  readonly beforeMessageId?: string;
  readonly afterMessageId?: string;
}

/** Fetch one page of raw stored rows, chronological, for a cursor. */
export type PageFetch = (cursor: HistoryCursor, limit: number) => Promise<readonly unknown[]>;

export class RoomHistory {
  private _rows: MessageRowVM[] = [];
  private _atStart = false;
  private _joined = true;
  private _unseen = 0;
  private _generation = 0;
  private _inFlight: number | undefined;

  /** Oldest first. Reassigned, never mutated, so a holder can compare by identity. */
  get rows(): readonly MessageRowVM[] {
    return this._rows;
  }

  /** A short older page came back: nothing older exists. */
  get atStart(): boolean {
    return this._atStart;
  }

  /** The span runs straight into the live window (no gap between them). */
  get joined(): boolean {
    return this._joined;
  }

  /** Live rows that arrived while the span was detached from the live window. */
  get unseen(): number {
    return this._unseen;
  }

  /** A page request of this generation is in flight; at most one per room. */
  get loading(): boolean {
    return this._inFlight !== undefined;
  }

  get canPageOlder(): boolean {
    return !this.loading && !this._atStart;
  }

  get canPageNewer(): boolean {
    return !this.loading && !this._joined && this._rows.length > 0;
  }

  /** Fetch one older page and prepend it. Returns the rows added (0 when the
   *  response was stale or empty). */
  async pageOlder(
    oldestLiveId: string | undefined,
    fetch: PageFetch,
    members: readonly RosterMemberVM[],
    onScreen: ReadonlySet<string>,
  ): Promise<number> {
    if (!this.canPageOlder) return 0;
    const before = this._rows[0]?.id ?? (this._joined ? oldestLiveId : undefined);
    const page = await this.request(fetch, before === undefined ? {} : { beforeMessageId: before });
    if (page === undefined) return 0;
    if (page.length < HISTORY_PAGE_ROWS) this._atStart = true;
    const held = new Set([...onScreen, ...this._rows.map((r) => r.id)]);
    const rows = historyRowsFromPoll(page, members, held);
    if (rows.length > 0) this._rows = [...rows, ...this._rows];
    return rows.length;
  }

  /** Fetch one newer page after the newest row held and append it; a page that
   *  reaches the live window (or comes back short: the present) rejoins it. */
  async pageNewer(
    liveIds: ReadonlySet<string>,
    fetch: PageFetch,
    members: readonly RosterMemberVM[],
  ): Promise<number> {
    const after = this._rows.at(-1)?.id;
    if (!this.canPageNewer || after === undefined) return 0;
    const page = await this.request(fetch, { afterMessageId: after });
    if (page === undefined) return 0;
    const held = new Set([...liveIds, ...this._rows.map((r) => r.id)]);
    const rows = historyRowsFromPoll(page, members, held);
    if (rows.length > 0) this._rows = [...this._rows, ...rows];
    const reachesLive = page.some((raw) => {
      const id = (raw as { id?: unknown } | null)?.id;
      return typeof id === 'string' && liveIds.has(id);
    });
    if (page.length < HISTORY_PAGE_ROWS || reachesLive) {
      this._joined = true;
      this._unseen = 0;
    }
    return rows.length;
  }

  /** The live window moved: `left` slid out of it, `arrived` rows came in. Joined,
   *  the rows that left join the span's newest end so no gap opens; detached, the
   *  arrivals are only counted. */
  retire(left: readonly MessageRowVM[], arrived: number): void {
    if (!this._joined) {
      this._unseen += arrived;
      return;
    }
    if (this._rows.length === 0) return;
    const held = new Set(this._rows.map((r) => r.id));
    const fresh = left.filter((r) => !held.has(r.id) && !r.id.startsWith('typing:'));
    if (fresh.length > 0) this._rows = [...this._rows, ...fresh];
  }

  /** Bound the span to the reader: keep one page beyond each edge of the rendered
   *  window (`start`/`end` are transcript indices; the span's rows lead the
   *  transcript) and drop the rest. Returns the rows dropped above the reader (the
   *  caller holds the reader's row still by their height). */
  trimAround(start: number, end: number): number {
    if (this.loading) return 0;
    const keepTo = end + HISTORY_PAGE_ROWS;
    if (keepTo < this._rows.length) {
      this._rows = this._rows.slice(0, keepTo);
      this._joined = false;
    }
    const drop = Math.min(this._rows.length, start - HISTORY_PAGE_ROWS);
    if (drop <= 0) return 0;
    this._rows = this._rows.slice(drop);
    this._atStart = false;
    return drop;
  }

  /** The reader left the room: abandon any request in flight. A detached span is
   *  dropped (the room reopens at the present); a joined one keeps its newest page. */
  leave(): void {
    this._generation += 1;
    this._inFlight = undefined;
    if (!this._joined) {
      this.jumpToPresent();
      return;
    }
    const drop = this._rows.length - HISTORY_PAGE_ROWS;
    if (drop > 0) {
      this._rows = this._rows.slice(drop);
      this._atStart = false;
    }
  }

  /** Back to the live window: the span is dropped and the count cleared. */
  jumpToPresent(): void {
    this._generation += 1;
    this._inFlight = undefined;
    this._rows = [];
    this._joined = true;
    this._atStart = false;
    this._unseen = 0;
  }

  /** One request of the current generation; `undefined` when the generation moved
   *  on while it was in flight (its response, or its failure, belongs to nobody). */
  private async request(fetch: PageFetch, cursor: HistoryCursor): Promise<readonly unknown[] | undefined> {
    const generation = this._generation;
    this._inFlight = generation;
    try {
      const page = await fetch(cursor, HISTORY_PAGE_ROWS);
      return generation === this._generation ? page : undefined;
    } catch (err) {
      if (generation !== this._generation) return undefined;
      throw err;
    } finally {
      if (this._inFlight === generation) this._inFlight = undefined;
    }
  }
}
