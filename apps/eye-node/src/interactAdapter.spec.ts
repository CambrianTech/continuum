import { describe, expect, it } from 'vitest';

import { IDLE_MS, InteractSessions, MAX_SESSIONS, toDomAction, type OpenWeb } from './interactAdapter';

// A stand-in session: records the actions it was driven with, and whether it was closed. The
// registry's contract (persist, bound, expire, never lose the page on a failed step) is what
// is under test, not Playwright, which domSurface.spec.ts already drives for real.
function fakeOpen(log: { opened: string[]; closed: number; acted: unknown[][] }, failOn?: string): OpenWeb {
  return async (url) => {
    log.opened.push(url);
    const percept = { width: 2, height: 2, rgba: new Uint8Array(16) };
    const observation = { percept, structure: { url, title: 'Tracker', tree: { role: 'document', name: '', children: [] } } };
    return {
      observe: async () => observation,
      interact: async (actions: unknown[]) => {
        if (failOn && JSON.stringify(actions).includes(failOn)) throw new Error(`no element matches ${failOn}`);
        log.acted.push(actions);
        return { observation, delta: { pixelsChanged: 1, totalPixels: 4, ratio: 0.25 } };
      },
      close: async () => {
        log.closed += 1;
      },
    } as never;
  };
}

describe('perception/interact sessions', () => {
  // what this catches: a session that does not persist (every call reopening the page, so a
  // multi-step flow is impossible), actions not reaching the driver, or the delta not coming back.
  it('opens once, continues on the same page, and returns the delta', async () => {
    const log = { opened: [] as string[], closed: 0, acted: [] as unknown[][] };
    const sessions = new InteractSessions(fakeOpen(log));
    const first = await sessions.interact({ target: 'http://localhost:31004/', actions: [] });
    expect(first.success).toBe(true);
    expect(first.session).toBeTruthy();
    const next = await sessions.interact({
      session: first.session,
      actions: [{ kind: 'click', selector: '#approve' }, { kind: 'goto', url: 'http://localhost:31004/gates' }],
    });
    expect(next.success).toBe(true);
    expect(next.delta?.ratio).toBe(0.25);
    expect(log.opened).toEqual(['http://localhost:31004/']);
    expect(log.acted[0]).toEqual([
      { kind: 'click', selector: '#approve' },
      { kind: 'goto', url: 'http://localhost:31004/gates' },
    ]);
    await sessions.closeAll();
  });

  // what this catches: an unknown or missing session silently opening something, a step that
  // fails costing her the page, and an unbounded number of browsers.
  it('refuses unknown sessions, keeps the page after a failed step, and bounds the count', async () => {
    const log = { opened: [] as string[], closed: 0, acted: [] as unknown[][] };
    const sessions = new InteractSessions(fakeOpen(log, '#missing'));
    expect((await sessions.interact({ session: 'nope', actions: [] })).success).toBe(false);
    expect((await sessions.interact({ actions: [] })).error).toMatch(/target/);

    const s = (await sessions.interact({ target: 'http://x/', actions: [] })).session!;
    const failed = await sessions.interact({ session: s, actions: [{ kind: 'click', selector: '#missing' }] });
    expect(failed.success).toBe(false);
    expect(failed.session).toBe(s);
    expect((await sessions.interact({ session: s, actions: [] })).success).toBe(true);

    for (let i = sessions.size; i < MAX_SESSIONS; i++) await sessions.interact({ target: `http://x/${i}`, actions: [] });
    const refused = await sessions.interact({ target: 'http://x/over', actions: [] });
    expect(refused.success).toBe(false);
    expect(refused.error).toMatch(/session-close/);
    expect(sessions.size).toBe(MAX_SESSIONS);
    await sessions.closeAll();
    expect(log.closed).toBe(MAX_SESSIONS);
  });

  // what this catches: an abandoned session holding a browser forever.
  it('closes a session idle past the limit', async () => {
    const log = { opened: [] as string[], closed: 0, acted: [] as unknown[][] };
    let now = 1_000;
    const sessions = new InteractSessions(fakeOpen(log), () => now);
    const s = (await sessions.interact({ target: 'http://x/', actions: [] })).session!;
    now += IDLE_MS;
    await sessions.sweep();
    expect(sessions.size).toBe(0);
    expect(log.closed).toBe(1);
    expect((await sessions.interact({ session: s, actions: [] })).error).toMatch(/expired|no live session/);
    await sessions.closeAll();
  });

  // what this catches (Fable on #4551): a handle pasted into a room letting another citizen
  // drive (or close) her logged-in session, and a file:// page rendering local files into her
  // observation.
  it('binds a session to its opener and opens only web pages', async () => {
    const log = { opened: [] as string[], closed: 0, acted: [] as unknown[][] };
    const sessions = new InteractSessions(fakeOpen(log));
    const kimi = { _callerPeerId: 'kimi' };
    const s = (await sessions.interact({ ...kimi, target: 'https://jobs.example/', actions: [] } as never)).session!;
    const stolen = await sessions.interact({ _callerPeerId: 'iris', session: s, actions: [] } as never);
    expect(stolen.success).toBe(false);
    expect(stolen.error).toMatch(/another citizen/);
    expect((await sessions.close({ _callerPeerId: 'iris', session: s } as never)).success).toBe(false);
    expect((await sessions.interact({ ...kimi, session: s, actions: [] } as never)).success).toBe(true);

    expect((await sessions.interact({ ...kimi, target: 'file:///Users/k/.continuum/config.env', actions: [] } as never)).error).toMatch(/http\(s\)/);
    const viaGoto = await sessions.interact({ ...kimi, session: s, actions: [{ kind: 'goto', url: 'file:///etc/hosts' }] } as never);
    expect(viaGoto.success).toBe(false);
    expect(log.acted).toEqual([]);
    expect((await sessions.close({ ...kimi, session: s } as never)).closed).toBe(true);
    await sessions.closeAll();
  });

  // what this catches: a wire kind mapped onto the wrong driver verb.
  it('maps each wire action onto the driver verb of the same kind', () => {
    expect(toDomAction({ kind: 'type', selector: 'input', text: 'x' })).toEqual({ kind: 'type', selector: 'input', text: 'x' });
    expect(toDomAction({ kind: 'press', key: 'Enter' })).toEqual({ kind: 'press', key: 'Enter' });
  });
});
