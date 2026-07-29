/**
 * StateConnection unit spec — the state-subscription client (task #29,
 * WIDGET-AS-STATE-KIND slice 3).
 *
 * Daemon-free: a FakeWebSocket drives the lifecycle callbacks so the tests pin
 * the subscribe-framing + per-kind state routing without a live core. (Sibling
 * of WebSocketTransport.spec.ts, which pins the command socket's mechanics;
 * this pins the state socket's.)
 */

import { describe, it, expect, vi } from 'vitest';
import { StateConnection } from './StateConnection';
import type { WebSocketLike } from './WebSocketTransport';
import type { ClientMessage } from './generated/positron/ClientMessage';
import type { ServerMessage } from './generated/positron/ServerMessage';
import type { StateEnvelope } from './generated/positron/StateEnvelope';

/** A scriptable WebSocket: records sent frames, exposes the lifecycle triggers. */
class FakeWebSocket implements WebSocketLike {
  static last?: FakeWebSocket;
  sent: string[] = [];
  closed = false;
  onopen: ((ev: unknown) => void) | null = null;
  onclose: ((ev: unknown) => void) | null = null;
  onerror: ((ev: unknown) => void) | null = null;
  onmessage: ((ev: { data: unknown }) => void) | null = null;

  constructor(public url: string) {
    FakeWebSocket.last = this;
  }
  send(data: string): void {
    this.sent.push(data);
  }
  close(): void {
    this.closed = true;
    this.onclose?.({});
  }
  // test-side triggers
  open(): void {
    this.onopen?.({});
  }
  deliver(msg: unknown): void {
    this.onmessage?.({ data: JSON.stringify(msg) });
  }
  fail(reason = 'refused'): void {
    this.onerror?.(reason);
  }
}

/** A `{type:'state'} & StateEnvelope` server frame for a kind. */
const stateFrame = (
  kind: string,
  revision: number | undefined,
  payload: unknown,
): Extract<ServerMessage, { type: 'state' }> => ({
  type: 'state',
  kind,
  revision,
  layer: 'ephemeral',
  payload,
});

const subOf = (frame: string): Extract<ClientMessage, { type: 'subscribe' }> =>
  JSON.parse(frame) as Extract<ClientMessage, { type: 'subscribe' }>;

/** The last constructed socket, fail-loud if the transport never made one — the
 *  test's precondition, surfaced instead of a bare non-null assertion. */
const lastSocket = (): FakeWebSocket => {
  const s = FakeWebSocket.last;
  if (!s) throw new Error('FakeWebSocket: no socket constructed yet');
  return s;
};

describe('StateConnection', () => {
  // what this catches: on connect the client MUST send exactly one Subscribe
  // frame naming every registered kind + the requested layers — the one frame
  // that starts the snapshot-then-live stream. A regression that forgot to send
  // it, or sent a command-shaped frame, would leave the widget blank forever.
  it('sends one subscribe frame with all registered kinds on connect', async () => {
    const conn = new StateConnection('ws://x', FakeWebSocket);
    conn.on('chat', () => { /* sink: this test asserts framing/routing/teardown, not payload delivery */ });
    conn.on('wall', () => { /* sink: this test asserts framing/routing/teardown, not payload delivery */ });
    const connected = conn.connect({ layers: ['ephemeral', 'session'] });
    lastSocket().open();
    await connected;

    expect(lastSocket().sent).toHaveLength(1);
    const frame = subOf(lastSocket().sent[0]);
    expect(frame.type).toBe('subscribe');
    expect(new Set(frame.kinds)).toEqual(new Set(['chat', 'wall']));
    expect(frame.layers).toEqual(['ephemeral', 'session']);
    expect(frame.last_seen).toEqual([]);
  });

  // what this catches: a state frame must route to the sink registered for its
  // kind and NOT to another kind's sink — the core multiplexes every widget's
  // state over one socket, so mis-routing by kind would cross-wire widgets.
  it('routes each state frame to its kind sink and no other', async () => {
    const conn = new StateConnection('ws://x', FakeWebSocket);
    const chat = vi.fn();
    const wall = vi.fn();
    conn.on('chat', chat);
    conn.on('wall', wall);
    const connected = conn.connect();
    lastSocket().open();
    await connected;

    lastSocket().deliver(stateFrame('chat', 1, { messages: ['hi'] }));
    expect(chat).toHaveBeenCalledTimes(1);
    expect(wall).not.toHaveBeenCalled();
    const env = chat.mock.calls[0][0] as StateEnvelope;
    expect(env).toEqual({ kind: 'chat', revision: 1, layer: 'ephemeral', payload: { messages: ['hi'] } });
  });

  // what this catches: the sink receives the FULL envelope (kind + revision +
  // layer + payload), not a bare payload — the app needs kind/revision to merge
  // into a positron ViewState. A regression that delivered only `payload` would
  // strip the framing the LitHost merge depends on.
  it('delivers the full envelope, not just the payload', async () => {
    const conn = new StateConnection('ws://x', FakeWebSocket);
    const sink = vi.fn();
    conn.on('chat', sink);
    const connected = conn.connect();
    lastSocket().open();
    await connected;

    lastSocket().deliver(stateFrame('chat', 7, { room: 'general' }));
    const env = sink.mock.calls[0][0] as StateEnvelope;
    expect(env.kind).toBe('chat');
    expect(env.revision).toBe(7);
    expect(env.layer).toBe('ephemeral');
    expect(env.payload).toEqual({ room: 'general' });
  });

  // what this catches: a revision seen on a state frame must be replayed as
  // last_seen on RE-subscribe (reconnect / add-kind) so the core can skip a
  // redundant snapshot. A regression that never tracked revisions would force a
  // full re-send of already-held state every reconnect.
  it('replays the latest seen revision as last_seen on re-subscribe', async () => {
    const conn = new StateConnection('ws://x', FakeWebSocket);
    conn.on('chat', () => { /* sink: this test asserts framing/routing/teardown, not payload delivery */ });
    const connected = conn.connect();
    lastSocket().open();
    await connected;
    lastSocket().deliver(stateFrame('chat', 42, {}));

    // Registering a new kind while live re-sends Subscribe with the superset.
    conn.on('wall', () => { /* sink: this test asserts framing/routing/teardown, not payload delivery */ });
    const frames = lastSocket().sent.map(subOf);
    const reSub = frames[frames.length - 1];
    expect(new Set(reSub.kinds)).toEqual(new Set(['chat', 'wall']));
    expect(reSub.last_seen).toEqual([{ kind: 'chat', revision: 42 }]);
  });

  // what this catches: after off(), the sink must stop receiving — a widget torn
  // down must not keep getting driven (leak / write-to-dead-DOM). Pins the
  // ownership check so a stale sink can't linger.
  it('stops delivering to a sink after off()', async () => {
    const conn = new StateConnection('ws://x', FakeWebSocket);
    const sink = vi.fn();
    const sub = conn.on('chat', sink);
    const connected = conn.connect();
    lastSocket().open();
    await connected;

    lastSocket().deliver(stateFrame('chat', 1, {}));
    sub.off();
    lastSocket().deliver(stateFrame('chat', 2, {}));
    expect(sink).toHaveBeenCalledTimes(1);
  });

  // what this catches: connect() with no registered kinds is a caller bug that
  // must fail loud — a silent empty subscription would look connected but stream
  // nothing, the exact bug the fail-loud doctrine exists to surface.
  it('fails loud on connect with no registered kinds', async () => {
    const conn = new StateConnection('ws://x', FakeWebSocket);
    const connected = conn.connect();
    lastSocket().open();
    await expect(connected).rejects.toThrow(/no registered kinds/);
  });

  // what this catches: an empty layers option would subscribe to zero cadence
  // layers (receive nothing) — a foot-gun that must fail loud, not silently
  // produce a dead feed.
  it('fails loud on connect with an empty layers list', async () => {
    const conn = new StateConnection('ws://x', FakeWebSocket);
    conn.on('chat', () => { /* sink: this test asserts framing/routing/teardown, not payload delivery */ });
    await expect(conn.connect({ layers: [] })).rejects.toThrow(/layers=\[\]/);
  });

  // what this catches: a dropped socket must surface to the app (stale-feed
  // signal), never be swallowed — a silent drop leaves the UI frozen with no
  // way for the app to know it went stale.
  it('surfaces a socket close to the onClose callback', async () => {
    const conn = new StateConnection('ws://x', FakeWebSocket);
    const onClose = vi.fn();
    conn.onClose(onClose);
    conn.on('chat', () => { /* sink: this test asserts framing/routing/teardown, not payload delivery */ });
    const connected = conn.connect();
    lastSocket().open();
    await connected;

    lastSocket().close();
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onClose.mock.calls[0][0]).toMatch(/closed/);
  });

  // what this catches: with reconnect OFF (one-shot consumers: probes, tests),
  // a failed connection must REJECT connect(), never hang or resolve — a caller
  // awaiting a dead core must see the failure.
  it('rejects connect when the socket errors before open (reconnect: false)', async () => {
    const conn = new StateConnection('ws://x', FakeWebSocket, { reconnect: false });
    conn.on('chat', () => { /* sink: this test asserts framing/routing/teardown, not payload delivery */ });
    const connected = conn.connect();
    lastSocket().fail('ECONNREFUSED');
    await expect(connected).rejects.toThrow(/failed/);
  });

  // what this catches: the DEFAULT contract is self-healing (positron-inherent
  // resilience — glass-boxed 2026-07-29: a core reboot orphaned every open tab
  // and four months of HUD looked "lost"). A failed first connect must RESOLVE,
  // surface a loud `reconnecting` status, and keep retrying — never strand the
  // renderer on a rejected boot promise.
  it('resolves connect on socket error by default and surfaces reconnecting status', async () => {
    const conn = new StateConnection('ws://x', FakeWebSocket);
    const statuses: string[] = [];
    conn.onStatus((s) => statuses.push(s));
    conn.on('chat', () => { /* status-only test */ });
    const connected = conn.connect();
    lastSocket().fail('ECONNREFUSED');
    await connected; // resolves — cached/last-known UI stays up, ladder runs
    expect(statuses).toContain('reconnecting');
    conn.close(); // stop the ladder so the test leaves no timer behind
    expect(statuses[statuses.length - 1]).toBe('closed');
  });

  // what this catches: the Twitter model — with a storage adapter, cached
  // envelopes must paint BEFORE the socket ever opens (instant last-known UI,
  // even against a dead core), under a `cached` status.
  it('hydrates cached envelopes to sinks before the socket opens', async () => {
    const { MemoryStateStorage } = await import('./StateStorage');
    const storage = new MemoryStateStorage();
    await storage.save('scopeA', { kind: 'chat', revision: 7, layer: 'ephemeral', payload: { cached: true } });
    const conn = new StateConnection('ws://x', FakeWebSocket, { storage, scope: 'scopeA' });
    const seen: StateEnvelope[] = [];
    const statuses: string[] = [];
    conn.onStatus((s) => statuses.push(s));
    conn.on('chat', (e) => seen.push(e));
    FakeWebSocket.last = undefined; // storage-await defers ctor; don't grab a stale socket
    const connected = conn.connect();
    // Cached delivery happens during connect(), BEFORE the socket ever opens.
    await vi.waitFor(() => expect(seen.length).toBeGreaterThanOrEqual(1));
    expect(seen[0].payload).toEqual({ cached: true });
    expect(statuses).toContain('cached');
    await vi.waitFor(() => lastSocket());
    lastSocket().open();
    await connected;
    conn.close();
  });

  // what this catches: write-through — every live envelope must land in the
  // adapter so the NEXT boot hydrates the newest snapshot per kind; a feed that
  // renders but forgets would make the cache silently stale.
  it('writes each live envelope through to storage', async () => {
    const { MemoryStateStorage } = await import('./StateStorage');
    const storage = new MemoryStateStorage();
    const conn = new StateConnection('ws://x', FakeWebSocket, { storage, scope: 'scopeB' });
    conn.on('chat', () => { /* write-through test */ });
    FakeWebSocket.last = undefined; // storage-await defers ctor; don't grab a stale socket
    const connected = conn.connect();
    await vi.waitFor(() => lastSocket());
    lastSocket().open();
    await connected;
    lastSocket().deliver(stateFrame('chat', 3, { live: true }));
    await Promise.resolve(); // let the fire-and-forget save settle (Memory adapter is sync-fast)
    const rows = await storage.load('scopeB');
    expect(rows).toHaveLength(1);
    expect(rows[0].envelope.payload).toEqual({ live: true });
    conn.close();
  });

  // what this catches: a dropped socket must SELF-HEAL — reconnect constructs a
  // fresh socket and re-sends Subscribe (with last_seen replay) so a routine
  // core reboot never permanently orphans an open renderer. Status stays loud
  // (`reconnecting` → `live`) the whole way.
  it('reconnects and resubscribes after a socket drop', async () => {
    vi.useFakeTimers();
    try {
      const conn = new StateConnection('ws://x', FakeWebSocket);
      const statuses: string[] = [];
      conn.onStatus((s) => statuses.push(s));
      conn.on('chat', () => { /* reconnect test */ });
      const connected = conn.connect();
      const first = lastSocket();
      first.open();
      await connected;
      first.deliver(stateFrame('chat', 1, {})); // live
      expect(statuses).toContain('live');

      first.close(); // simulate core reboot
      expect(statuses[statuses.length - 1]).toBe('reconnecting');

      await vi.advanceTimersByTimeAsync(1100); // ladder step 1 (1s)
      const second = lastSocket();
      expect(second).not.toBe(first);
      second.open();
      await vi.advanceTimersByTimeAsync(0);
      expect(second.sent.length).toBeGreaterThanOrEqual(1);
      const sub = subOf(second.sent[0]);
      expect(sub.kinds).toContain('chat');
      expect(sub.last_seen).toEqual([{ kind: 'chat', revision: 1 }]);
      second.deliver(stateFrame('chat', 2, {}));
      expect(statuses[statuses.length - 1]).toBe('live');
      conn.close();
    } finally {
      vi.useRealTimers();
    }
  });
});
