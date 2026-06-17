/**
 * SDK conformance spec (TDD). The behaviors EVERY SDK must satisfy — pinned here in
 * TS against a daemon-free MockTransport so the loop is instant (`vitest --watch`),
 * and mirrored per language (Rust cargo, swift/kotlin) so the native twins prove
 * the SAME contract. Includes hot-path TIMING budgets (optimization-first): the SDK
 * is a thin skin, so URI build + serialize-once + dispatch must be ~free.
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

/** Daemon-free transport double — records what crossed the boundary. */
class MockTransport implements Transport {
  executed: Array<{ command: string; paramsJson: string }> = [];
  provided = new Map<string, RawCommandHandler>();
  subscribed: Array<{ topic: string; filterJson?: string }> = [];
  emitted: Array<{ eventClass: string; payloadJson: string }> = [];
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
    // expose the handler for the test to drive deliveries
    (this as unknown as { lastHandlers: RawEventHandlers }).lastHandlers = handlers;
    return { unsubscribe: () => {} };
  }
  async emit(eventClass: string, payloadJson: string): Promise<void> {
    this.emitted.push({ eventClass, payloadJson });
  }
}

describe('addressing (projects onto the redone CommandUri)', () => {
  it('bare name = Local (back-compatible)', () => {
    expect(buildCommandUri('data/list')).toBe('data/list');
  });
  it('peer + env = cross-environment dispatch', () => {
    expect(buildCommandUri('interface/screenshot', { peer: 'p1', env: 'web' }))
      .toBe('airc://p1:web/interface/screenshot');
  });
  it('peer@node:env carries WHO/WHERE/WHICH', () => {
    expect(buildCommandUri('x/y', { peer: 'p1', node: 'n2', env: 'vr' }))
      .toBe('airc://p1@n2:vr/x/y');
  });
  it('room fan-out', () => {
    expect(buildCommandUri('x', { room: 'r1', env: 'web' })).toBe('airc://room:r1:web/x');
  });
  it('event topic: local class vs a peer’s event stream', () => {
    expect(buildEventTopic('data:users:created')).toBe('data:users:created');
    expect(buildEventTopic('grid:peer:joined', { peer: 'p1' }))
      .toBe('airc://p1/events/grid:peer:joined');
  });
});

describe('Commands primitive (execute + provide)', () => {
  it('execute: serialize-once out, parse-once back, typed result', async () => {
    const t = new MockTransport();
    t.willReturn('{"ok":true,"roundTripMs":3}');
    const c = Continuum.connect(t);
    const r = await c.commands.execute('ping', { message: 'hi' });
    expect(t.executed[0]).toEqual({ command: 'ping', paramsJson: '{"message":"hi"}' });
    expect(r).toEqual({ ok: true, roundTripMs: 3 });
  });

  it('execute routes through the cross-environment URI', async () => {
    const t = new MockTransport();
    const c = Continuum.connect(t);
    await c.commands.execute('interface/screenshot', { querySelector: 'body' }, { peer: 'p1', env: 'web' });
    expect(t.executed[0].command).toBe('airc://p1:web/interface/screenshot');
  });

  it('provide: registers the platform adapter; routed call runs it + serializes back', async () => {
    const t = new MockTransport();
    const c = Continuum.connect(t);
    c.commands.provide('interface/screenshot', async (p: { querySelector?: string }) => ({
      dataUrl: `shot:${p.querySelector}`,
      width: 1,
      height: 1,
    }));
    const handler = t.provided.get('interface/screenshot')!;
    const out = await handler.handle('{"querySelector":"body"}');
    expect(JSON.parse(out)).toEqual({ dataUrl: 'shot:body', width: 1, height: 1 });
  });
});

describe('Events primitive (subscribe + emit)', () => {
  it('subscribe: source addressing + server-side filter + sequence delivery', () => {
    const t = new MockTransport();
    const c = Continuum.connect(t);
    const seen: Array<[unknown, number]> = [];
    c.events.subscribe(
      'grid:peer:joined' as never,
      { onEvent: (e, meta) => seen.push([e, meta.sequence]) } as never,
      { source: { peer: 'p1' }, filter: { runtime: 'persona' } as never },
    );
    expect(t.subscribed[0].topic).toBe('airc://p1/events/grid:peer:joined');
    expect(t.subscribed[0].filterJson).toBe('{"runtime":"persona"}');
    // drive a delivery through the recorded raw handler
    (t as unknown as { lastHandlers: RawEventHandlers }).lastHandlers.onEvent('{"runtime":"persona"}', 7);
    expect(seen).toEqual([[{ runtime: 'persona' }, 7]]);
  });

  it('emit: publishes the typed payload', async () => {
    const t = new MockTransport();
    const c = Continuum.connect(t);
    await c.events.emit('data:users:created' as never, { id: 'u1' } as never);
    expect(t.emitted[0]).toEqual({ eventClass: 'data:users:created', payloadJson: '{"id":"u1"}' });
  });
});

describe('Handle (addressable resource — long-running / streaming / multi-hop)', () => {
  it('open returns a handle carrying its URI; ops + events route to it', async () => {
    const t = new MockTransport();
    t.willReturn('{"uri":"airc://p1/live/conn-42"}');
    const c = Continuum.connect(t);
    const conn = await c.open('ping' as never, {} as never); // ping stands in for an open-style cmd
    expect(conn.uri).toBe('airc://p1/live/conn-42');

    t.willReturn('{}');
    await conn.execute('offer', { sdp: 's' });
    expect(t.executed.at(-1)!.command).toBe('airc://p1/live/conn-42/offer');

    conn.on('signal', { onEvent: () => {} });
    expect(t.subscribed.at(-1)!.topic).toBe('airc://p1/live/conn-42/events/signal');

    await conn.close();
    expect(t.executed.at(-1)!.command).toBe('airc://p1/live/conn-42/close');
  });
});

describe('timing budgets (optimization-first: the SDK is a thin skin)', () => {
  it('URI build is ~free (100k builds well under 50ms)', () => {
    const start = performance.now();
    for (let i = 0; i < 100_000; i++) buildCommandUri('a/b', { peer: 'p', env: 'web' });
    expect(performance.now() - start).toBeLessThan(50);
  });
});
