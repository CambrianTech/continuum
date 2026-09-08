import { describe, it, expect } from 'vitest';
import { mergeCalls, PlaybackView, type CallHeader } from './playback';
const call = (id: string, status: CallHeader['status'] = 'submitted', at = 1): CallHeader => ({
  request_id: id, session_id: 'session', cycle_id: 7, room_id: 'actual-room', cause: 'stimulus',
  request: { generation: 'retained-segment', offset: 0, bytes: 1 }, terminal: null,
  cause_root: 'original-task', captured_at_ms: at, started_at_ms: at, status, model: 'fixture', cursor: `${id}:${at}`,
});
const page = (entries: CallHeader[], older: string | null = null) => ({ page: {
  entries, older, newer: entries.at(-1)?.cursor ?? null, issues: [],
}, detail: null });

describe('recorded mind playback', () => {
  it('coalesces interleaved lifecycle events without letting older history erase a terminal', () => {
    const submitted = call('a'); const completed = call('a', 'completed', 3);
    const b = call('b', 'submitted', 2);
    const rows = mergeCalls([completed], [submitted, b]);
    expect(rows).toHaveLength(2);
    expect(rows.find((row) => row.request_id === 'a')?.status).toBe('completed');
    expect(mergeCalls(rows, [completed])).toHaveLength(2);
  });

  it('reads lean headers, fetches selected payload once, and never dispatches effects or inference', async () => {
    const requests: Record<string, unknown>[] = [];
    const header = call('a');
    const view = new PlaybackView(async (params) => {
      requests.push(params);
      if (params.selected) return { page: null, detail: { header, submitted: {
        request: { systemPrompt: 'actual identity', messages: [{ role: 'user', content: 'original task' }],
          tools: [{ name: 'code/run' }] },
      }, terminal: null, issues: [] } };
      return params.cursor ? page([]) : page([header]);
    }, () => {});
    view.focus('persona'); await view.open();
    expect(requests).toHaveLength(2);
    expect(requests[0]).toEqual({ persona_id: 'persona', limit: 30 });
    expect(requests[1]).toEqual({ persona_id: 'persona', selected: header.cursor });
    await view.refresh();
    expect(requests).toHaveLength(3);
    expect(requests[2]).toEqual({ persona_id: 'persona', limit: 30, cursor: header.cursor, newer: true });
    expect(view.detail?.submitted).toHaveProperty('request');
    view.pause(); await view.refresh();
    expect(requests).toHaveLength(3);
  });

  it('discards a response from a persona that is no longer focused', async () => {
    let complete: ((value: ReturnType<typeof page>) => void) | undefined;
    const view = new PlaybackView(() => new Promise((resolve) => { complete = resolve; }), () => {});
    view.focus('first'); const pending = view.open();
    view.focus('second'); complete?.(page([call('first-call')]));
    await pending;
    expect(view.calls).toEqual([]); expect(view.persona).toBe('second');
    expect(view.detail).toBeUndefined();
  });

  it('rejects stale same-persona data after leaving and returning', async () => {
    let complete: ((value: ReturnType<typeof page>) => void) | undefined;
    const view = new PlaybackView(() => new Promise((resolve) => { complete = resolve; }), () => {});
    view.focus('first'); const pending = view.open();
    view.focus('second'); view.focus('first');
    complete?.(page([call('old-session')])); await pending;
    expect(view.calls).toEqual([]); expect(view.opened).toBe(false);
  });

  it('Resume invalidates an in-flight older page and requests a new live tail', async () => {
    let olderComplete: ((value: ReturnType<typeof page>) => void) | undefined;
    let initial = true;
    const view = new PlaybackView(async (params) => {
      if (params.selected) return { page: null, detail: null };
      if (params.cursor && params.newer === false) return new Promise((resolve) => { olderComplete = resolve; });
      const result = page([call(initial ? 'old-tail' : 'new-tail')], 'older'); initial = false; return result;
    }, () => {});
    view.focus('persona'); await view.open(); view.pause();
    const older = view.page(true); await view.resume();
    olderComplete?.(page([call('stale-older')])); await older;
    expect(view.live).toBe(true);
    expect(view.calls.map((row) => row.request_id)).toEqual(['new-tail']);
  });

  it('Resume invalidates a pending selected payload', async () => {
    let selectedComplete: ((value: { page: null; detail: { header: CallHeader; submitted: null; terminal: null; issues: string[] } }) => void) | undefined;
    const stale = call('stale');
    const view = new PlaybackView(async (params) => {
      if (params.selected === stale.cursor) return new Promise((resolve) => { selectedComplete = resolve; });
      return params.selected ? { page: null, detail: null } : page([call('current')]);
    }, () => {});
    view.focus('persona'); await view.open(); view.pause();
    const selected = view.select(stale); await view.resume();
    selectedComplete?.({ page: null, detail: { header: stale, submitted: null, terminal: null, issues: [] } });
    await selected;
    expect(view.detail?.header.request_id).not.toBe('stale');
    expect(view.calls[0]?.request_id).toBe('current');
  });

  it('retains a bounded window and reads previous calls without adding a timer', async () => {
    let batch = 0;
    const view = new PlaybackView(async (params) => params.selected ? { page: null, detail: null }
      : page(Array.from({ length: 30 }, (_, index) => call(`call-${batch * 30 + index}`, 'completed', batch * 30 + index)), 'older'), () => {});
    view.focus('persona'); await view.open();
    for (batch = 1; batch < 6; batch += 1) await view.refresh();
    expect(view.calls).toHaveLength(120);
    expect(view.older).toBeDefined();
    expect(view.issues.some((issue) => issue.includes('120'))).toBe(true);
  });
});
