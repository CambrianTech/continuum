/**
 * NodeSocketTransport — framing + provider back-channel, proven against a fake
 * duplex socket (no live core). Covers the two directions the eye-node needs:
 * a command CALL correlated by requestId, and the SERVE path where a core
 * `provideCall` is dispatched to a registered handler and answered with a
 * `provideResult`.
 */

import { describe, it, expect } from 'vitest';
import { NodeSocketTransport, type DuplexSocketLike } from './NodeSocketTransport';

/** Yield the microtask/timer queue so awaited connect + writes settle. */
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

/** A fake duplex socket: records outbound text frames, lets a test push
 *  length-prefixed inbound frames (the core→client wire shape) and fire the
 *  connect lifecycle. */
class FakeSocket implements DuplexSocketLike {
  readonly writes: string[] = [];
  private readonly listeners = new Map<string, (arg?: unknown) => void>();

  write(data: string, cb?: (err?: Error | null) => void): void {
    this.writes.push(data);
    cb?.(null);
  }
  end(): void {
    this.fire('close');
  }
  on(event: 'data', listener: (chunk: Uint8Array) => void): this;
  on(event: 'connect', listener: () => void): this;
  on(event: 'error', listener: (err: Error) => void): this;
  on(event: 'close', listener: () => void): this;
  on(event: string, listener: (...args: never[]) => void): this {
    this.listeners.set(event, listener as (arg?: unknown) => void);
    return this;
  }

  fire(event: string, arg?: unknown): void {
    this.listeners.get(event)?.(arg);
  }

  /** Simulate the core sending a length-prefixed JSON frame to this client. */
  pushFrame(obj: unknown): void {
    const bytes = new TextEncoder().encode(JSON.stringify(obj));
    const frame = new Uint8Array(4 + bytes.length);
    new DataView(frame.buffer).setUint32(0, bytes.length, false); // big-endian
    frame.set(bytes, 4);
    this.fire('data', frame);
  }
}

describe('NodeSocketTransport', () => {
  // what this catches: execute() frames `{command, ...params, requestId}` (the
  // exact shape the Rust reader spreads) and resolves the correlated response.
  it('CALLs a command and resolves the correlated reply', async () => {
    const fake = new FakeSocket();
    const t = new NodeSocketTransport(() => fake, 'test');

    const resultPromise = t.execute('ping', JSON.stringify({ echo: 'hi' }));
    fake.fire('connect');
    await tick();

    expect(fake.writes).toHaveLength(1);
    const sent = JSON.parse(fake.writes[0]);
    expect(sent.command).toBe('ping');
    expect(sent.echo).toBe('hi'); // params spread at top level
    expect(typeof sent.requestId).toBe('number');

    fake.pushFrame({ success: true, result: { pong: 1 }, requestId: sent.requestId });
    expect(JSON.parse(await resultPromise)).toEqual({ pong: 1 });
  });

  // what this catches: a failed response rejects loud with the core's message —
  // never a silent resolve.
  it('rejects loud when the core returns an error', async () => {
    const fake = new FakeSocket();
    const t = new NodeSocketTransport(() => fake);
    const p = t.execute('nope', '{}');
    fake.fire('connect');
    await tick();
    const sent = JSON.parse(fake.writes[0]);
    fake.pushFrame({ success: false, error: 'no such command', requestId: sent.requestId });
    await expect(p).rejects.toThrow('no such command');
  });

  // what this catches: the whole SERVE path — provide()+flush() registers, then a
  // core provideCall is dispatched to the handler and answered with a
  // provideResult carrying the handler's result. This IS the eye-node fulfilling
  // perception/observe.
  it('SERVEs a provided command: register → dispatch → provideResult', async () => {
    const fake = new FakeSocket();
    const t = new NodeSocketTransport(() => fake, 'eye-test');

    const seen: string[] = [];
    t.provide('perception/observe', {
      handle: async (paramsJson) => {
        seen.push(paramsJson);
        return JSON.stringify({ success: true, title: 'Hello' });
      },
    });

    const flushPromise = t.flush();
    fake.fire('connect');
    await tick();

    const reg = JSON.parse(fake.writes[0]);
    expect(reg.command).toBe('provider/register');
    expect(reg.commands).toEqual(['perception/observe']);
    expect(reg.label).toBe('eye-test');
    fake.pushFrame({ success: true, result: { registered: reg.commands }, requestId: reg.requestId });
    await flushPromise;

    // Core forwards a persona's observe.
    fake.writes.length = 0;
    fake.pushFrame({
      type: 'provideCall',
      callId: 7,
      command: 'perception/observe',
      params: { target: 'https://example.test' },
    });
    await tick();

    expect(seen[0]).toContain('https://example.test');
    const reply = JSON.parse(fake.writes[0]);
    expect(reply).toMatchObject({
      type: 'provideResult',
      callId: 7,
      success: true,
      result: { success: true, title: 'Hello' },
    });
  });

  // what this catches: a handler throw becomes a provideResult(success:false)
  // with the message — the persona's observe fails loud, never hangs or fabricates.
  it('answers a throwing handler with provideResult(success:false)', async () => {
    const fake = new FakeSocket();
    const t = new NodeSocketTransport(() => fake, 'eye-test');
    t.provide('perception/observe', {
      handle: async () => {
        throw new Error('browser crashed');
      },
    });
    const flushPromise = t.flush();
    fake.fire('connect');
    await tick();
    const reg = JSON.parse(fake.writes[0]);
    fake.pushFrame({ success: true, result: {}, requestId: reg.requestId });
    await flushPromise;

    fake.writes.length = 0;
    fake.pushFrame({ type: 'provideCall', callId: 9, command: 'perception/observe', params: {} });
    await tick();

    const reply = JSON.parse(fake.writes[0]);
    expect(reply).toMatchObject({ type: 'provideResult', callId: 9, success: false });
    expect(reply.error).toContain('browser crashed');
  });

  // what this catches: a provideCall for a command this client didn't register
  // fails loud back to the core (not a silent drop that would hang the persona).
  it('rejects a provideCall it has no handler for', async () => {
    const fake = new FakeSocket();
    const t = new NodeSocketTransport(() => fake);
    // Prime the connection via a benign execute so the socket exists.
    void t.execute('ping', '{}');
    fake.fire('connect');
    await tick();
    fake.writes.length = 0;

    fake.pushFrame({ type: 'provideCall', callId: 3, command: 'interface/screenshot', params: {} });
    await tick();

    const reply = JSON.parse(fake.writes[0]);
    expect(reply).toMatchObject({ type: 'provideResult', callId: 3, success: false });
    expect(reply.error).toContain('interface/screenshot');
  });
});
