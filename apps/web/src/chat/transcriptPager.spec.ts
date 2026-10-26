import { describe, expect, it } from 'vitest';
import type { ReactiveController, ReactiveControllerHost } from 'lit';
import type { ChatViewModel, MessageRowVM } from '@continuum/chat-view';
import { TranscriptPager, type HistoryCursor } from './transcriptPager';

/** The scroll container, as far as the pager reads it. */
class Scroller {
  scrollTop = 0;
  scrollHeight = 3000;
  clientHeight = 600;
  onScroll: ((e: Event) => void) | undefined;
  addEventListener(_type: string, listener: (e: Event) => void): void {
    this.onScroll = listener;
  }
  removeEventListener(): void {
    this.onScroll = undefined;
  }
  getBoundingClientRect(): { top: number } {
    return { top: 0 };
  }
  querySelector(): null {
    return null;
  }
  scrollTo(top: number): void {
    this.scrollTop = top;
    this.onScroll?.({ currentTarget: this } as unknown as Event);
  }
}

class Host implements ReactiveControllerHost {
  updates = 0;
  readonly updateComplete = Promise.resolve(true);
  constructor(readonly scroller: Scroller) {}
  readonly renderRoot = { querySelector: (sel: string): Scroller | null => (sel === '.what' ? this.scroller : null) };
  addController(_c: ReactiveController): void {
    /* the pager is driven directly */
  }
  removeController(_c: ReactiveController): void {
    /* the pager is driven directly */
  }
  requestUpdate(): void {
    this.updates += 1;
  }
}

const row = (id: string): MessageRowVM => ({ id, senderId: 's', senderName: 's', kind: 'agent', content: id, time: '', runtime: '' });
const room = (roomId: string, ids: readonly string[]): ChatViewModel => ({
  roomName: roomId,
  roomId,
  purpose: 'chat',
  memberCount: 0,
  activeCount: 0,
  members: [],
  messages: ids.map(row),
  transcript: ids.map((id) => ({ row: 'message' as const, ...row(id) })),
  isEmpty: ids.length === 0,
});

describe('TranscriptPager', () => {
  // what this catches: A -> B -> A with a page in flight. The page was asked for in
  // the first visit to A; when it lands in the second it must neither move the
  // reader (a scroll compensation measured against the old visit's content) nor
  // enter the buffer, nor ask for a render.
  it('drops a page and its scroll compensation from an earlier visit', async () => {
    // Node has no frames: run the pager's end-of-frame release at once.
    globalThis.requestAnimationFrame = (cb: FrameRequestCallback): number => {
      cb(0);
      return 0;
    };
    const scroller = new Scroller();
    const host = new Host(scroller);
    let release: (rows: readonly unknown[]) => void = () => undefined;
    const asked: HistoryCursor[] = [];
    const pager = new TranscriptPager(
      host as unknown as ReactiveControllerHost & { renderRoot: ParentNode },
      () => (_room, cursor) => {
        asked.push(cursor);
        return new Promise((resolve) => {
          release = resolve;
        });
      },
      (message) => {
        throw new Error(message);
      },
    );
    const live = ['a1', 'a2', 'a3'];
    pager.enterRoom('A');
    pager.compose(room('A', live), true);
    pager.afterRender({ transcript: true, pinToLatest: false });
    scroller.scrollTo(100); // within a viewport of the top: page older
    expect(asked).toEqual([{ beforeMessageId: 'a1' }]);

    pager.enterRoom('B');
    pager.enterRoom('A');
    pager.compose(room('A', live), true);
    const updates = host.updates;
    release([{ id: 'old', senderId: 's', timestamp: '2026-10-26T00:00:00Z', content: { text: 'old' } }]);
    await Promise.resolve();
    await Promise.resolve();
    expect(host.updates).toBe(updates);

    scroller.scrollHeight = 6000; // whatever the new visit's content measures
    scroller.scrollTop = 100;
    const vm = pager.compose(room('A', live), true);
    pager.afterRender({ transcript: true, pinToLatest: false });
    expect(scroller.scrollTop).toBe(100);
    expect(vm.transcript.map((r) => r.id)).toEqual(live);
  });

  // what this catches: "Jump to present" that does not move. The click's render
  // changes neither state nor typing, so nothing else asks to pin: the jump itself
  // must scroll to the latest row on the next render, once, and a later render with
  // nothing new must leave the reader where they scrolled.
  it('pins to the latest row once after Jump to present', () => {
    globalThis.requestAnimationFrame = (cb: FrameRequestCallback): number => {
      cb(0);
      return 0;
    };
    const scroller = new Scroller();
    const host = new Host(scroller);
    const pager = new TranscriptPager(
      host as unknown as ReactiveControllerHost & { renderRoot: ParentNode },
      () => undefined,
      (message) => {
        throw new Error(message);
      },
    );
    const vm = room('A', ['a1', 'a2', 'a3']);
    pager.enterRoom('A');
    pager.compose(vm, true);
    pager.afterRender({ transcript: true, pinToLatest: false });
    scroller.scrollTop = 0;

    pager.jumpToPresent();
    pager.compose(vm, true);
    pager.afterRender({ transcript: true, pinToLatest: false });
    expect(scroller.scrollTop).toBe(scroller.scrollHeight);

    scroller.scrollTop = 500;
    pager.compose(vm, true);
    pager.afterRender({ transcript: true, pinToLatest: false });
    expect(scroller.scrollTop).toBe(500);
  });
});
