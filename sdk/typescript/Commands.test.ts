/**
 * THE SDK CONFORMANCE CONTRACT.
 *
 * This file is the single, authoritative behavioral spec EVERY Continuum SDK must
 * satisfy — the web/desktop TS SDK here, and the native twins (Rust cargo,
 * Swift XCTest, Kotlin JUnit over the uniffi binding) which mirror these exact
 * checks name-for-name. If a behavior isn't pinned here, it isn't part of the
 * contract; if it is, every twin proves it. Pinned in TS first because the loop is
 * instant (`vitest --watch`) against a daemon-free MockTransport.
 *
 * The contract has FIVE sections, each a conformance dimension:
 *   1. Addressing      — name + Target → the airc:// URI / event topic (pure, total)
 *   2. Commands        — execute (call) + provide (serve): serialize-once fidelity
 *   3. Events          — subscribe (listen) + emit (publish): source, filter, sequence, teardown
 *   4. Handle          — addressable resource: ops + event stream + close route to one URI
 *   5. Timing          — the SDK is a thin skin; the hot path is ~free (optimization-first)
 *
 * Mirroring guide for the native twins: each `it(...)` is one contract clause.
 * Keep the clause text identical across languages so a reviewer can diff suites.
 *
 * Run: npm test -w @continuum/sdk-typescript   (vitest)
 */

import { describe, it, expect } from 'vitest';
import { Continuum } from './Continuum';
import { buildCommandUri, buildEventTopic } from './transport';
import type {
  Transport,
  RawEventHandlers,
  RawCommandHandler,
  Subscription,
  Registration,
} from './transport';

/**
 * Daemon-free transport double — records exactly what crossed the byte boundary
 * (the JSON strings), so the contract asserts on the wire, not on internals. The
 * native twins implement the same double over their facade's seam.
 */
class MockTransport implements Transport {
  executed: Array<{ command: string; paramsJson: string }> = [];
  provided = new Map<string, RawCommandHandler>();
  subscribed: Array<{ topic: string; filterJson?: string }> = [];
  unsubscribed = 0;
  emitted: Array<{ eventClass: string; payloadJson: string }> = [];
  lastHandlers?: RawEventHandlers;
  private nextResult = '{}';

  willReturn(json: string) {
    this.nextResult = json;
  }
  async execute(command: string, paramsJson: string): Promise<string> {
    this.executed.push({ command, paramsJson });
    return this.nextResult;
  }
  provide(command: string, handler: RawCommandHandler): Registration {
    this.provided.set(command, handler);
    return { remove: () => this.provided.delete(command) };
  }
  subscribe(topic: string, handlers: RawEventHandlers, filterJson?: string): Subscription {
    this.subscribed.push({ topic, filterJson });
    this.lastHandlers = handlers;
    return { unsubscribe: () => { this.unsubscribed += 1; } };
  }
  async emit(eventClass: string, payloadJson: string): Promise<void> {
    this.emitted.push({ eventClass, payloadJson });
  }
}

// ─── 1. Addressing ────────────────────────────────────────────────────────────
//
// buildCommandUri / buildEventTopic are PURE and TOTAL — the entire cross-grid
// addressing surface reduces to these two functions. They project onto the core's
// CommandUri / RouteDecision (core/src/routing/). Pin every Target shape: a twin
// that builds a different string routes to the wrong place silently.

describe('1. addressing — name + Target → airc:// URI', () => {
  it('bare name = Local (no target, back-compatible)', () => {
    expect(buildCommandUri('data/list')).toBe('data/list');
  });
  it('peer only = that citizen, default node + embodiment', () => {
    expect(buildCommandUri('x/y', { peer: 'p1' })).toBe('airc://p1/x/y');
  });
  it('peer + env = a specific embodiment (WHICH)', () => {
    expect(buildCommandUri('interface/screenshot', { peer: 'p1', env: 'web' }))
      .toBe('airc://p1:web/interface/screenshot');
  });
  it('peer + node (no env) = a specific machine', () => {
    expect(buildCommandUri('x/y', { peer: 'p1', node: 'n2' })).toBe('airc://p1@n2/x/y');
  });
  it('peer@node:env carries WHO / WHERE / WHICH', () => {
    expect(buildCommandUri('x/y', { peer: 'p1', node: 'n2', env: 'vr' }))
      .toBe('airc://p1@n2:vr/x/y');
  });
  it("peer + env '*' = every embodiment (broadcast to the citizen)", () => {
    expect(buildCommandUri('x/y', { peer: 'p1', env: '*' })).toBe('airc://p1:*/x/y');
  });
  it('room (no env) = fan-out to room subscribers', () => {
    expect(buildCommandUri('x', { room: 'r1' })).toBe('airc://room:r1/x');
  });
  it('room + env = fan-out to one embodiment of the room', () => {
    expect(buildCommandUri('x', { room: 'r1', env: 'web' })).toBe('airc://room:r1:web/x');
  });

  it('event topic: bare class = local stream', () => {
    expect(buildEventTopic('data:users:created')).toBe('data:users:created');
  });
  it("event topic: a peer's event stream across the grid", () => {
    expect(buildEventTopic('grid:peer:joined', { peer: 'p1' }))
      .toBe('airc://p1/events/grid:peer:joined');
  });
  it("event topic: a room's event stream", () => {
    expect(buildEventTopic('data:users:created', { room: 'r1' }))
      .toBe('airc://room:r1/events/data:users:created');
  });
});

// ─── 2. Commands (execute + provide) ────────────────────────────────────────────

describe('2. Commands — execute (call) + provide (serve)', () => {
  it('execute: serialize-once out, parse-once back, typed result', async () => {
    const t = new MockTransport();
    t.willReturn('{"ok":true,"roundTripMs":3}');
    const c = Continuum.connect(t);
    const r = await c.commands.execute('ping', { message: 'hi' });
    expect(t.executed[0]).toEqual({ command: 'ping', paramsJson: '{"message":"hi"}' });
    expect(r).toEqual({ ok: true, roundTripMs: 3 });
  });

  it('execute: nested params serialize with full fidelity (no flatten/reorder)', async () => {
    const t = new MockTransport();
    t.willReturn('{"items":[],"total":0}');
    const c = Continuum.connect(t);
    await c.commands.execute('data/list', {
      collection: 'users',
      orderBy: [{ field: 'lastActiveAt', direction: 'desc' }],
      filter: { active: true },
    });
    expect(JSON.parse(t.executed[0].paramsJson)).toEqual({
      collection: 'users',
      orderBy: [{ field: 'lastActiveAt', direction: 'desc' }],
      filter: { active: true },
    });
  });

  it('execute: routes through the cross-environment URI', async () => {
    const t = new MockTransport();
    const c = Continuum.connect(t);
    await c.commands.execute('interface/screenshot', { querySelector: 'body' }, { peer: 'p1', env: 'web' });
    expect(t.executed[0].command).toBe('airc://p1:web/interface/screenshot');
  });

  it('provide: registers the platform adapter; routed call runs it + serializes back', async () => {
    const t = new MockTransport();
    const c = Continuum.connect(t);
    c.commands.provide('interface/screenshot', async (p) => ({
      dataUrl: `shot:${p.querySelector}`,
      width: 1,
      height: 1,
    }));
    const handler = t.provided.get('interface/screenshot')!;
    const out = await handler.handle('{"querySelector":"body"}');
    expect(JSON.parse(out)).toEqual({ dataUrl: 'shot:body', width: 1, height: 1 });
  });

  it('provide: Registration.remove() deregisters the adapter', () => {
    const t = new MockTransport();
    const c = Continuum.connect(t);
    const reg = c.commands.provide('interface/screenshot', async () => ({ dataUrl: '', width: 0, height: 0 }));
    expect(t.provided.has('interface/screenshot')).toBe(true);
    reg.remove();
    expect(t.provided.has('interface/screenshot')).toBe(false);
  });
});

// ─── 3. Events (subscribe + emit) ───────────────────────────────────────────────

describe('3. Events — subscribe (listen) + emit (publish)', () => {
  it('subscribe: local class = bare topic, no filter', () => {
    const t = new MockTransport();
    const c = Continuum.connect(t);
    c.events.subscribe('data:users:created', { onEvent: () => {} });
    expect(t.subscribed[0]).toEqual({ topic: 'data:users:created', filterJson: undefined });
  });

  it('subscribe: source addressing + server-side filter + typed sequence delivery', () => {
    const t = new MockTransport();
    const c = Continuum.connect(t);
    const seen: Array<[unknown, number]> = [];
    c.events.subscribe(
      'grid:peer:joined',
      { onEvent: (e, meta) => seen.push([e, meta.sequence]) },
      { source: { peer: 'p1' }, filter: { runtime: 'persona' } },
    );
    expect(t.subscribed[0].topic).toBe('airc://p1/events/grid:peer:joined');
    expect(t.subscribed[0].filterJson).toBe('{"runtime":"persona"}');
    // drive a delivery through the recorded raw handler: parse-once + sequence
    t.lastHandlers!.onEvent('{"peerId":"p9","runtime":"persona"}', 7);
    expect(seen).toEqual([[{ peerId: 'p9', runtime: 'persona' }, 7]]);
  });

  it('subscribe: onError / onClosed propagate from the transport', () => {
    const t = new MockTransport();
    const c = Continuum.connect(t);
    let err: string | undefined;
    let closed = false;
    c.events.subscribe('data:users:created', {
      onEvent: () => {},
      onError: (m) => { err = m; },
      onClosed: () => { closed = true; },
    });
    t.lastHandlers!.onError?.('route lost');
    t.lastHandlers!.onClosed?.();
    expect(err).toBe('route lost');
    expect(closed).toBe(true);
  });

  it('subscribe: Subscription.unsubscribe() tears down', () => {
    const t = new MockTransport();
    const c = Continuum.connect(t);
    const sub = c.events.subscribe('data:users:created', { onEvent: () => {} });
    sub.unsubscribe();
    expect(t.unsubscribed).toBe(1);
  });

  it('emit: publishes the typed payload, serialize-once', async () => {
    const t = new MockTransport();
    const c = Continuum.connect(t);
    await c.events.emit('data:users:created', { id: 'u1' });
    expect(t.emitted[0]).toEqual({ eventClass: 'data:users:created', payloadJson: '{"id":"u1"}' });
  });
});

// ─── 4. Handle (addressable resource) ───────────────────────────────────────────
//
// Commands + events + routing collapse into ONE addressing scheme: an open-style
// command returns a URI, and every later op + the event stream + close route to
// `<uri>/...`. This is the WebRTC-connection / file-session / inference-session
// shape — and it works unchanged whether the resource is local or N hops away.

describe('4. Handle — ops + event stream + close all route to one URI', () => {
  it('open returns a handle carrying its URI; ops + events + close route to it', async () => {
    const t = new MockTransport();
    t.willReturn('{"uri":"airc://p1/live/conn-42"}');
    const c = Continuum.connect(t);
    const conn = await c.open('ping', {}); // ping stands in for an open-style cmd
    expect(conn.uri).toBe('airc://p1/live/conn-42');

    t.willReturn('{"accepted":true}');
    const ack = await conn.execute<{ sdp: string }, { accepted: boolean }>('offer', { sdp: 's' });
    expect(t.executed.at(-1)!.command).toBe('airc://p1/live/conn-42/offer');
    expect(ack).toEqual({ accepted: true });

    conn.on('signal', { onEvent: () => {} });
    expect(t.subscribed.at(-1)!.topic).toBe('airc://p1/live/conn-42/events/signal');

    await conn.close();
    expect(t.executed.at(-1)!.command).toBe('airc://p1/live/conn-42/close');
  });
});

// ─── 5. Timing (optimization-first: the SDK is a thin skin) ──────────────────────

describe('5. timing — the hot path is ~free', () => {
  it('URI build is ~free (100k builds well under 50ms)', () => {
    const start = performance.now();
    for (let i = 0; i < 100_000; i++) buildCommandUri('a/b', { peer: 'p', node: 'n', env: 'web' });
    expect(performance.now() - start).toBeLessThan(50);
  });
  it('event-topic build is ~free (100k builds well under 50ms)', () => {
    const start = performance.now();
    for (let i = 0; i < 100_000; i++) buildEventTopic('grid:peer:joined', { peer: 'p1' });
    expect(performance.now() - start).toBeLessThan(50);
  });
});
