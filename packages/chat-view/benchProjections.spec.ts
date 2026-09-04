import { describe, expect, it } from 'vitest';
import type { BenchViewState, BenchRunRow } from '@continuum/sdk-typescript';
import { benchContentBody, benchWidget } from './benchProjections';
import { BENCH_PURPOSE } from '@continuum/patterns';
import type { ChatViewModel } from './chatViewModel';
import { ACADEMY_PURPOSE, chatWorkspace, type AcademyContentBody } from './patternProjections';

const row = (over: Partial<BenchRunRow>): BenchRunRow => ({
  run_id: 'claim-abc',
  phase: 'active',
  stalled: false,
  age_secs: 60,
  failed_tests: [],
  ...over,
});

const view = (runs: BenchRunRow[]): BenchViewState => ({ runs, rounds: [], sample_interval_ms: 5000 });

describe('benchContentBody', () => {
  // what this catches: the wire→VM fold's honesty rules — instance/solver
  // absent fall back to identifiable truth (run id / 'unclaimed'), a graded
  // row carries a verdict with the p2p REGRESSION alarm derived, and a row
  // with zero acts reads as queued with the "no generations yet" pulse —
  // never dressed as work.
  it('folds a working graded row with a regression alarm', () => {
    const body = benchContentBody(
      view([
        row({
          instance: 'sympy__sympy-21055',
          solver: 'anon-uuid',
          attempt: 2,
          max_attempts: 3,
          acts: 10,
          patch_bytes: 1295,
          fail_to_pass: '0/1',
          pass_to_pass: '31/34',
          failed_tests: ['test_refine'],
        }),
      ]),
    );
    expect(body.feedLive).toBe(true);
    const run = body.runs[0]!;
    expect(run.instance).toBe('sympy__sympy-21055');
    expect(run.persona).toBe('anon-uuid');
    expect(run.state).toBe('working');
    expect(run.generations).toBe(10);
    expect(run.lastGenAgeS).toBe(60);
    expect(run.patchBytes).toBe(1295);
    expect(run.verdict).toMatchObject({
      resolved: false,
      f2pPassed: 0,
      f2pTotal: 1,
      regression: true, // 31/34 — the destroyed-the-tree alarm, never a count
      failedTests: ['test_refine'],
    });
    expect(run.editActs).toBeUndefined();
  });

  it('reads a zero-act active row as queued, a quiet row as stalled', () => {
    const body = benchContentBody(view([row({}), row({ run_id: 'r2', phase: 'quiet', acts: 3 })]));
    expect(body.runs[0]!.state).toBe('queued');
    expect(body.runs[0]!.lastGenAgeS).toBeNull();
    expect(body.runs[1]!.state).toBe('stalled');
  });

  it('compacts uuid-shaped ids to short form — a board row never spends 36 hex chars', () => {
    // what this catches: the live-feed first-render defect (2026-08-12) — the
    // ledger's persona_id uuid and claim-<uuid> run ids rendered raw. The core
    // resolves LIVE personas to names; anything still uuid-shaped compacts to
    // the system's 8-char short-id vocabulary (#161). Real names pass through.
    const body = benchContentBody(
      view([
        row({
          run_id: 'claim-c995488a-bb84-4c31-9f10-6a2b3c4d5e6f',
          solver: '90e758b2-3cf3-45c1-b100-de7c4ab5a549',
        }),
      ]),
    );
    expect(body.runs[0]!.instance).toBe('claim-c995488a');
    expect(body.runs[0]!.persona).toBe('90e758b2');
  });

  it('undelivered feed is the honest snapshot frame', () => {
    const body = benchContentBody(undefined);
    expect(body.feedLive).toBe(false);
    expect(body.runs).toHaveLength(0);
  });
});

describe('benchWidget', () => {
  // what this catches: the rail contract — no feed / no runs → NO widget
  // (the rail never shows an empty frame for a node that isn't
  // benchmarking); a delivering feed with rows joins the rail.
  it('joins the rail only when rows exist', () => {
    expect(benchWidget(undefined)).toBeUndefined();
    expect(benchWidget(view([]))).toBeUndefined();
    const w = benchWidget(view([row({ instance: 'x' })]));
    expect(w?.kind).toBe('bench');
    expect(w?.body.runs).toHaveLength(1);
  });
});

describe('a run room is a conversation with its round on top', () => {
  const roomVm = (roomName: string): ChatViewModel => ({
    roomName,
    roomId: 'room-run',
    purpose: 'benchmark/hard-rs',
    transcript: [],
    memberCount: 2,
    activeCount: 2,
    members: [],
    messages: [
      { id: 'm1', senderId: 'joel', senderName: 'Joel', kind: 'human', content: 'which card do you hold?', time: '08:06', runtime: '' },
    ],
    isEmpty: false,
  });
  const rounds = (runRoom: string): BenchViewState => ({
    runs: [],
    sample_interval_ms: 5000,
    rounds: [
      { run_room: runRoom, round_id: 'r1', benchmark: 'swe-bench-verified', stage: 'Working', dispatched: 12, settled: 0, remaining: 12, driver: 'citizens', cards: [], verdict: '' },
      { run_room: 'bench-other', round_id: 'r2', benchmark: 'swe-bench-verified', stage: 'Paused', dispatched: 4, settled: 1, remaining: 3, driver: 'citizens', cards: [], verdict: '' },
    ],
  });

  // what this catches: the round widget REPLACING the transcript in a run room
  // (live 2026-09-04: an operator's line into the room, and the "heard by N"
  // receipt on it, had nowhere to render). The run room takes the landing
  // shape — its OWN round on top, the chat underneath.
  it('renders its own round above the transcript', () => {
    const ws = chatWorkspace(roomVm('bench-swe-bench-verified-1'), { bench: rounds('bench-swe-bench-verified-1') });
    expect(ws.content.purpose).toBe(ACADEMY_PURPOSE);
    const body = ws.content.body as AcademyContentBody;
    expect(body.chat.messages.map((m) => m.content)).toEqual(['which card do you hold?']);
    expect(body.bench.rounds).toHaveLength(1);
    expect(body.chatOpen).toBe(true);
    expect(body.title).toBe('verified · working · 0/12 in hands');
  });

  // what this catches: a bench-family room that is NOT a round's run room (an
  // index) must keep the board-only face.
  it('a bench index that is no round keeps the board alone', () => {
    const ws = chatWorkspace(roomVm('bench-index'), { bench: rounds('bench-swe-bench-verified-1') });
    expect(ws.content.purpose).toBe(BENCH_PURPOSE);
  });
});
