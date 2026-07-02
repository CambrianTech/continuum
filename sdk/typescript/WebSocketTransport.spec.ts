/**
 * WebSocketTransport unit spec — the thin-client WS binding (task #29 Layer 0).
 *
 * Daemon-free: a FakeWebSocket drives the four lifecycle callbacks so the tests
 * pin the correlation/multiplexing/unwrap logic without a live core. (The full
 * cross-SDK behavioral contract lives in conformance.spec.ts against a
 * MockTransport; this file pins the ONE concrete binding's wire mechanics.)
 */

import { describe, it, expect } from 'vitest';
import { WebSocketTransport, type WebSocketLike, type WebSocketCtor } from './WebSocketTransport';
import type { WsClientMessage } from './generated/wire/transport/WsClientMessage';

/** A scriptable WebSocket: records sent frames, exposes the lifecycle triggers. */
class FakeWebSocket implements WebSocketLike {
  static last?: FakeWebSocket;
  sent: string[] = [];
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
    this.onclose?.({});
  }
  // test-side triggers
  open(): void {
    this.onopen?.({});
  }
  deliver(msg: unknown): void {
    this.onmessage?.({ data: JSON.stringify(msg) });
  }
}

/** Flush microtasks so post-`await ensureConnected()` sends land. */
const flush = () => new Promise((r) => setTimeout(r, 0));

const idOf = (frame: string): number => (JSON.parse(frame) as WsClientMessage).id;
const pathOf = (frame: string): string => (JSON.parse(frame) as WsClientMessage).request.path;

describe('WebSocketTransport', () => {
  // what this catches: an ok reply matched by correlation id must resolve with
  // the UNWRAPPED result JSON (AircCommandResponse.Ok.result), not the envelope —
  // Commands.execute JSON.parses this directly.
  it('unwraps an ok response to the plain result json', async () => {
    const t = new WebSocketTransport('ws://x', FakeWebSocket as unknown as WebSocketCtor);
    const p = t.execute('health/ping', '{}');
    FakeWebSocket.last!.open();
    await flush();
    const id = idOf(FakeWebSocket.last!.sent[0]);
    FakeWebSocket.last!.deliver({ type: 'response', id, response: { status: 'ok', result: { pong: true } } });
    expect(JSON.parse(await p)).toEqual({ pong: true });
  });

  // what this catches: N commands over ONE socket must be matched by id, even
  // when replies arrive out of order — the whole reason the envelope carries a
  // correlation id. A regression that resolved by arrival-order would swap these.
  it('correlates concurrent replies by id regardless of arrival order', async () => {
    const t = new WebSocketTransport('ws://x', FakeWebSocket as unknown as WebSocketCtor);
    const pOne = t.execute('a/one', '{}');
    const pTwo = t.execute('a/two', '{}');
    FakeWebSocket.last!.open();
    await flush();
    const ws = FakeWebSocket.last!;
    expect(ws.sent).toHaveLength(2);
    const byPath = new Map(ws.sent.map((f) => [pathOf(f), idOf(f)]));
    // reply to two FIRST, then one
    ws.deliver({ type: 'response', id: byPath.get('a/two'), response: { status: 'ok', result: 2 } });
    ws.deliver({ type: 'response', id: byPath.get('a/one'), response: { status: 'ok', result: 1 } });
    expect(JSON.parse(await pOne)).toBe(1);
    expect(JSON.parse(await pTwo)).toBe(2);
  });

  // what this catches: an error response must REJECT with the server's message —
  // never resolve with an error-shaped object that a caller would treat as success.
  it('rejects an error response with the server message', async () => {
    const t = new WebSocketTransport('ws://x', FakeWebSocket as unknown as WebSocketCtor);
    const p = t.execute('data/list', '{}');
    FakeWebSocket.last!.open();
    await flush();
    const id = idOf(FakeWebSocket.last!.sent[0]);
    FakeWebSocket.last!.deliver({ type: 'response', id, response: { status: 'error', message: 'policy denied' } });
    await expect(p).rejects.toThrow('policy denied');
  });

  // what this catches: a mid-flight connection close must REJECT pending commands
  // loudly, never leave a promise hung forever (fail-loud on terminal drop).
  it('rejects in-flight commands when the socket closes', async () => {
    const t = new WebSocketTransport('ws://x', FakeWebSocket as unknown as WebSocketCtor);
    const p = t.execute('a/slow', '{}');
    FakeWebSocket.last!.open();
    await flush();
    FakeWebSocket.last!.close();
    await expect(p).rejects.toThrow(/closed/);
  });

  // what this catches: the serve/publish/subscribe frames are NOT part of the WS
  // ingress yet — they must fail loud (named cause), never a silent no-op that a
  // caller mistakes for a working subscription.
  it('fails loud on unsupported serve/publish/subscribe', async () => {
    const t = new WebSocketTransport('ws://x', FakeWebSocket as unknown as WebSocketCtor);
    expect(() => t.provide('x/y', { handle: async () => '{}' })).toThrow(/not supported/);
    expect(() => t.subscribe('some:event', { onEvent: () => {} })).toThrow(/not supported/);
    await expect(t.emit('some:event', '{}')).rejects.toThrow(/not supported/);
  });

  // what this catches: an unauthenticated WS socket is Provisional — session()
  // must be empty, never a fabricated identity.
  it('reports an empty session (unauthenticated Provisional)', () => {
    const t = new WebSocketTransport('ws://x', FakeWebSocket as unknown as WebSocketCtor);
    expect(t.session()).toEqual({});
  });
});
