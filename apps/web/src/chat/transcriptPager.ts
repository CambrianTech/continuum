/**
 * The transcript's scroll owner for `<chat-widget>`: history paging, the rendered
 * window, and the reader's intent (follow the live edge, or read back).
 *
 * Joel, 2026-10-26: "The browser goes out of control... do you understand how
 * discord and others do this?" A room opens on its newest page; live rows append;
 * scrolling up pages older history by message cursor; only rows near the viewport
 * are in the DOM; rows far above the reader are dropped from memory and paged back
 * if the reader returns. Before this, scroll-back re-read the newest page on every
 * scroll-up, every row ever seen stayed in the DOM, and programmatic scrolls
 * (prepend compensation, pin-to-latest) re-triggered fetches.
 */
import type { ReactiveController, ReactiveControllerHost } from 'lit';
import type { ChatViewModel, MessageRowVM, RosterMemberVM } from '@continuum/chat-view';
import { RoomHistory, type HistoryCursor } from './roomHistory';
import { RowHeights, windowFor, type TranscriptWindow, type Viewport } from './transcriptWindow';

/** The history fetch the host injects (`chat/history { beforeMessageId |
 *  afterMessageId, limit }`). Resolves to one page of RAW stored entities adjacent to
 *  the cursor (the newest page with no cursor), chronological; a page shorter than
 *  `limit` means the room's start (or the present) was reached. */
export type HistoryHandler = (
  roomId: string,
  cursor: HistoryCursor,
  limit: number,
) => Promise<readonly unknown[]>;
export type { HistoryCursor } from './roomHistory';

/** "At the live edge" tolerance: sub-pixel rounding and a trailing margin must not
 *  read as the reader having scrolled up. */
const LIVE_EDGE_SLACK_PX = 40;

/** What the host knows about this render, passed to `afterRender`. */
export interface RenderFacts {
  /** The transcript is the face on screen (not a persona home, live call, or other
   *  content): only then does the pager page, window, measure or trim. */
  readonly transcript: boolean;
  /** New content may pin the view to the latest row (the reader follows it). */
  readonly pinToLatest: boolean;
}

type PagerHost = ReactiveControllerHost & { readonly renderRoot: ParentNode };

export class TranscriptPager implements ReactiveController {
  private readonly rooms = new Map<string, RoomHistory>();
  private roomId: string | undefined;
  /** Incremented on every room enter: a page that lands in a later visit (A, B, A
   *  again) neither moves the reader nor changes the buffer. */
  private visit = 0;
  private readonly heights = new RowHeights();
  private viewport: Viewport = { scrollTop: 0, clientHeight: 0 };
  private ids: readonly string[] = [];
  private window: TranscriptWindow | undefined;
  private live: readonly MessageRowVM[] = [];
  private members: readonly RosterMemberVM[] = [];
  private scrollHost: Element | undefined;
  /** The reader follows the live edge; false once they scroll away from it. */
  private following = true;
  /** Set around the pager's own scrolls, so their scroll events never read as the
   *  reader's intent or trigger a fetch. */
  private programmatic = false;
  /** Rows were added or dropped above the viewport: restore the reader's row after
   *  the render by the scroll height that changed. */
  private keep: { readonly top: number; readonly height: number } | undefined;
  /** "Jump to present" was clicked: pin to the latest row on the next render that
   *  draws the transcript, once. That render changes neither state nor typing, so
   *  the host's `pinToLatest` would not ask for it. */
  private jumpPending = false;

  constructor(
    private readonly host: PagerHost,
    private readonly fetchHandler: () => HistoryHandler | undefined,
    private readonly onError: (message: string) => void,
  ) {
    host.addController(this);
  }

  hostDisconnected(): void {
    this.scrollHost?.removeEventListener('scroll', this.onScroll);
    this.scrollHost = undefined;
  }

  /** The room on screen changed. The room left keeps only its newest page (or, if
   *  its reader was parked far up, nothing: it reopens at the present); the room
   *  entered starts at its live edge, in a new visit. */
  enterRoom(roomId: string): void {
    if (this.roomId === roomId) return;
    if (this.roomId !== undefined) this.rooms.get(this.roomId)?.leave();
    this.roomId = roomId;
    this.visit += 1;
    this.keep = undefined;
    this.following = true;
    this.window = undefined;
    this.live = [];
  }

  /** The reader's span no longer reaches the live window (they are parked far up). */
  get detached(): boolean {
    return !(this.current()?.joined ?? true);
  }

  /** Live rows that arrived while detached ("N new messages"). */
  get unseen(): number {
    return this.current()?.unseen ?? 0;
  }

  /** Drop the span and follow the live window again. */
  jumpToPresent(): void {
    this.current()?.jumpToPresent();
    this.following = true;
    this.keep = undefined;
    this.jumpPending = true;
    this.host.requestUpdate();
  }

  /** The new live rows arrived: rows that left the live window join the history
   *  (or, detached, the arrivals are counted). */
  retire(liveNow: readonly MessageRowVM[]): void {
    if (this.live.length === 0) return;
    const now = liveNow.filter((r) => !r.id.startsWith('typing:'));
    const ids = new Set(now.map((r) => r.id));
    const before = new Set(this.live.map((r) => r.id));
    this.current()?.retire(
      this.live.filter((r) => !ids.has(r.id)),
      now.filter((r) => !before.has(r.id)).length,
    );
  }

  /** The view model to render: history rows above the live ones, windowed to the
   *  rows near the viewport, with spacers for the rest. `vm` holds the live rows. */
  compose(vm: ChatViewModel, transcript: boolean): ChatViewModel {
    this.live = vm.messages.filter((m) => !m.id.startsWith('typing:'));
    this.members = vm.members;
    if (!transcript) return vm;
    const liveIds = new Set(vm.messages.map((m) => m.id));
    const history = this.current();
    const older = (history?.rows ?? []).filter((r) => !liveIds.has(r.id));
    // Detached, the live rows sit beyond a gap the reader has not paged through:
    // they are counted, not drawn.
    const joined = history?.joined ?? true;
    const below = joined ? vm.transcript : [];
    const full: ChatViewModel =
      older.length === 0 && joined
        ? vm
        : {
            ...vm,
            messages: [...older, ...vm.messages],
            transcript: [...older.map((r) => ({ row: 'message' as const, ...r })), ...below],
            isEmpty: older.length === 0 && below.length === 0,
          };
    this.ids = full.transcript.map((r) => r.id);
    this.heights.retain(new Set(this.ids));
    const w = windowFor(this.ids, this.heights, this.viewport, this.following);
    this.window = w;
    if (w.start === 0 && w.end === this.ids.length) return full;
    return {
      ...full,
      transcript: full.transcript.slice(w.start, w.end),
      transcriptSpacers: { abovePx: w.abovePx, belowPx: w.belowPx },
    };
  }

  /** After the host rendered: attach to the scroll container, measure the rows the
   *  window drew, keep the reader's row still, follow the live edge, bound the
   *  history, and fill a viewport the transcript does not yet fill. */
  afterRender(facts: RenderFacts): void {
    const el = this.host.renderRoot.querySelector('.what');
    if (el !== this.scrollHost) {
      this.scrollHost?.removeEventListener('scroll', this.onScroll);
      el?.addEventListener('scroll', this.onScroll, { passive: true });
      this.scrollHost = el ?? undefined;
    }
    if (!el || !facts.transcript) return;

    const drift = this.measure(el);
    if (this.keep !== undefined) {
      this.scrollBy(el, el.scrollHeight - this.keep.height + this.keep.top - el.scrollTop);
      this.keep = undefined;
    } else if (!this.following && drift !== 0) {
      this.scrollBy(el, drift);
    }
    if ((facts.pinToLatest && this.following) || this.jumpPending) {
      this.jumpPending = false;
      this.scrollBy(el, el.scrollHeight - el.scrollTop);
    }
    this.viewport = { scrollTop: el.scrollTop, clientHeight: el.clientHeight };

    this.trim(el);
    if (this.windowMoved()) this.host.requestUpdate();
    // A transcript shorter than its viewport has no scrollbar to scroll with: page
    // until it fills the viewport or the room's start (or the present) is reached.
    if (el.scrollHeight <= el.clientHeight) {
      if (this.detached) void this.pageNewer();
      else void this.pageOlder(el);
    }
  }

  private readonly onScroll = (e: Event): void => {
    const el = e.currentTarget as Element;
    this.viewport = { scrollTop: el.scrollTop, clientHeight: el.clientHeight };
    if (this.windowMoved()) this.host.requestUpdate();
    if (this.programmatic) return;
    const atBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - LIVE_EDGE_SLACK_PX;
    this.following = atBottom && !this.detached;
    // Within one viewport of either end of the span: the next page that way is
    // about to be needed.
    if (el.scrollTop < el.clientHeight) void this.pageOlder(el);
    else if (this.detached && el.scrollTop + 2 * el.clientHeight >= el.scrollHeight) void this.pageNewer();
  };

  private current(): RoomHistory | undefined {
    if (this.roomId === undefined) return undefined;
    let h = this.rooms.get(this.roomId);
    if (h === undefined) {
      h = new RoomHistory();
      this.rooms.set(this.roomId, h);
    }
    return h;
  }

  private async pageOlder(el: Element): Promise<void> {
    const handler = this.fetchHandler();
    const room = this.roomId;
    const visit = this.visit;
    const history = this.current();
    if (!handler || room === undefined || !history?.canPageOlder) return;
    try {
      const added = await history.pageOlder(
        this.live[0]?.id,
        (cursor, limit) => handler(room, cursor, limit),
        this.members,
        new Set(this.live.map((r) => r.id)),
      );
      // A page from an earlier visit never moves the reader of this one.
      if (added === 0 || visit !== this.visit) return;
      this.holdReaderRow(el, added * this.heights.mean);
      this.host.requestUpdate();
    } catch (err) {
      if (visit !== this.visit) return;
      this.onError(`History load failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  /** Page down toward the present (rows append below the reader: nothing to hold). */
  private async pageNewer(): Promise<void> {
    const handler = this.fetchHandler();
    const room = this.roomId;
    const visit = this.visit;
    const history = this.current();
    if (!handler || room === undefined || !history?.canPageNewer) return;
    try {
      const added = await history.pageNewer(
        new Set(this.live.map((r) => r.id)),
        (cursor, limit) => handler(room, cursor, limit),
        this.members,
      );
      if (visit !== this.visit) return;
      if (added > 0 || history.joined) this.host.requestUpdate();
    } catch (err) {
      if (visit !== this.visit) return;
      this.onError(`History load failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  /** Bound the span to the reader (see `RoomHistory.trimAround`). */
  private trim(el: Element): void {
    const history = this.current();
    if (!this.window || !history) return;
    const ids = this.ids;
    const rows = history.rows.length;
    const dropped = history.trimAround(this.window.start, this.window.end);
    if (dropped === 0 && history.rows.length === rows) return;
    if (dropped > 0) {
      const px = ids.slice(0, dropped).reduce((sum, id) => sum + this.heights.of(id), 0);
      this.holdReaderRow(el, -px);
    }
    this.host.requestUpdate();
  }

  /** Content of `px` height is about to be added (or removed, negative) above the
   *  viewport: shift the window's notion of position now, and restore the reader's
   *  row from the real scroll height after the render. */
  private holdReaderRow(el: Element, px: number): void {
    this.keep = { top: el.scrollTop, height: el.scrollHeight };
    this.viewport = { ...this.viewport, scrollTop: this.viewport.scrollTop + px };
  }

  /** Record the drawn rows' heights. Returns how much rows above the viewport grew
   *  past the height their spacer stood in for (the reader's row moved that much). */
  private measure(el: Element): number {
    const w = this.window;
    const list = el.querySelector('ul.messages');
    if (!w || !list) return 0;
    const top = el.getBoundingClientRect().top;
    let drift = 0;
    let i = w.start;
    for (const li of Array.from(list.children)) {
      if (li.classList.contains('transcript-spacer')) continue;
      const id = this.ids[i];
      i += 1;
      if (id === undefined) break;
      const rect = li.getBoundingClientRect();
      if (rect.bottom <= top) drift += rect.height - this.heights.of(id);
      this.heights.record(id, rect.height);
    }
    return drift;
  }

  private windowMoved(): boolean {
    if (!this.window) return false;
    const next = windowFor(this.ids, this.heights, this.viewport, this.following);
    return next.start !== this.window.start || next.end !== this.window.end;
  }

  private scrollBy(el: Element, px: number): void {
    if (px === 0) return;
    this.programmatic = true;
    el.scrollTop += px;
    // Scroll events from this change are dispatched before the next frame's
    // callbacks, so they all see the flag.
    requestAnimationFrame(() => {
      this.programmatic = false;
    });
  }
}
