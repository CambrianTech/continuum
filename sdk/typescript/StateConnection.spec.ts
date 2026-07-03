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

  // what this catches: a failed connection must REJECT connect(), never hang or
  // resolve — a caller awaiting a dead core must see the failure.
  it('rejects connect when the socket errors before open', async () => {
    const conn = new StateConnection('ws://x', FakeWebSocket);
    conn.on('chat', () => { /* sink: this test asserts framing/routing/teardown, not payload delivery */ });
    const connected = conn.connect();
    lastSocket().fail('ECONNREFUSED');
    await expect(connected).rejects.toThrow(/failed/);
  });
});
