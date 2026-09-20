/**
 * Multi-Persona Response Timing — chat/persona E2E regression test
 *
 * Codifies the bar that Mac+Windows smoke runs in #1057→#1060 surfaced:
 * post #1062 backpressure work, the storm IS fixed (CPU stays flat) BUT
 * fairness is broken — first-claim-wins, only ONE persona responds when
 * N candidates are eligible. This test makes that failure mode explicit
 * so the eventual fix has an executable green-vs-red signal.
 *
 * What it does
 * ------------
 * 1. Send ONE chat message into a room with N≥3 active personas.
 * 2. Poll chat/export every 500ms with the probe's shortId as anchor.
 * 3. Record when each persona's reply (replyToId === probe shortId) lands.
 * 4. Assert:
 *    - First persona reply within FIRST_RESPONSE_BUDGET_MS (10s per #1062)
 *    - All eligible personas reply within ALL_RESPONSE_BUDGET_MS (30s)
 *    - At least MIN_FAIR_RESPONSE_COUNT of N personas reply (fairness)
 *
 * Loud-fail buckets per #1063 / #1067 typed-bucket pattern:
 *   probe_not_persisted             — chat/send returned ok but DB has no row
 *   no_personas_replied             — no persona replied at all (storm-fix
 *                                     over-corrected into total silence)
 *   first_response_budget_exceeded  — first reply arrived after 10s
 *   all_response_budget_exceeded    — full reply set didn't settle in 30s
 *   fairness_violated               — only K of N replied where K < min
 *
 * Standing-rule alignment (#1070 / #1072):
 * - Single attempt, no retry on failure
 * - Loud-fail with typed bucket — operator greps result, doesn't dig
 *   through logs
 * - No silent fallback — the test reports what actually happened on the
 *   user-facing surface (chat_messages → chat/export)
 *
 * Uses ./jtag CLI via execFile to stay decoupled from in-process JTAGClient
 * TS surface drift; matches the chat-probe pattern operators already use.
 *
 * Run:
 *   npx tsx src/tests/integration/multi-persona-response-timing.test.ts
 */

import { execFile as execFileCb } from 'child_process';
import { promisify } from 'util';
import * as path from 'path';

const execFile = promisify(execFileCb);

// =============================================================================
// Failure bucket taxonomy
// =============================================================================

export type TimingFailureBucket =
  | 'probe_not_persisted'
  | 'no_personas_replied'
  | 'first_response_budget_exceeded'
  | 'all_response_budget_exceeded'
  | 'fairness_violated';

export interface TimingFailure {
  bucket: TimingFailureBucket;
  reason: string;
  observed?: {
    expected_personas: number;
    replied_personas: number;
    first_response_ms?: number;
    full_response_ms?: number;
    persona_response_ms: Record<string, number>;
  };
}

export interface TimingSuccess {
  probe_short_id: string;
  expected_personas: number;
  replied_personas: number;
  first_response_ms: number;
  full_response_ms: number;
  persona_response_ms: Record<string, number>;
}

export type TimingResult =
  | { ok: true; success: TimingSuccess }
  | { ok: false; failure: TimingFailure };

// =============================================================================
// Budgets — alpha SLOs from #1062 RecipeTurnBatchPlan defaults
// =============================================================================

const FIRST_RESPONSE_BUDGET_MS = 10_000;
const ALL_RESPONSE_BUDGET_MS = 30_000;
const POLL_INTERVAL_MS = 500;
const MIN_FAIR_RESPONSE_COUNT = 2;
const TARGET_ROOM = 'general';
const JTAG_BIN = path.resolve(__dirname, '../../../jtag');

// =============================================================================
// Smoke runner
// =============================================================================

interface JtagResult { stdout: string; stderr: string }

async function jtag(command: string, params: Record<string, string | number | boolean>): Promise<unknown> {
  const args = [command];
  for (const [k, v] of Object.entries(params)) args.push(`--${k}=${v}`);
  const { stdout }: JtagResult = await execFile(JTAG_BIN, args, { maxBuffer: 16 * 1024 * 1024 });
  // ./jtag prints status lines + final JSON object. Find the trailing JSON.
  const jsonStart = stdout.lastIndexOf('{');
  if (jsonStart === -1) throw new Error(`./jtag ${command} produced no JSON: ${stdout.slice(0, 500)}`);
  return JSON.parse(stdout.slice(jsonStart));
}

export async function runMultiPersonaResponseTimingSmoke(): Promise<TimingResult> {
  // STEP 1 — count expected personas via data/list.
  const personaList = await jtag('data/list', { collection: 'users' }) as { items?: Array<{ type?: string }> };
  const expectedPersonas = (personaList?.items ?? []).filter((u) => u?.type === 'persona').length;
  if (expectedPersonas < MIN_FAIR_RESPONSE_COUNT) {
    return failBucket('no_personas_replied', `room has only ${expectedPersonas} seeded personas; need >= ${MIN_FAIR_RESPONSE_COUNT}`);
  }

  // STEP 2 — send ONE chat message.
  const probeMarker = `multi-persona-timing-${Date.now()}`;
  const sendResult = await jtag('collaboration/chat/send', { room: TARGET_ROOM, message: probeMarker }) as { shortId?: string };
  const probeShortId = sendResult?.shortId;
  if (!probeShortId) {
    return failBucket('probe_not_persisted', 'collaboration/chat/send returned no shortId');
  }

  // STEP 3 — verify probe persisted.
  const verify = await jtag('collaboration/chat/export', { room: TARGET_ROOM, limit: 5 }) as { markdown?: string };
  if (!verify?.markdown?.includes(probeMarker)) {
    return failBucket('probe_not_persisted', `probe shortId=${probeShortId} not visible in chat/export within first poll`);
  }

  // STEP 4 — poll chat_messages for replies whose replyToId === probeShortId.
  const startWait = Date.now();
  const personaResponseMs: Record<string, number> = {};
  let firstResponseMs: number | undefined;

  while (Date.now() - startWait < ALL_RESPONSE_BUDGET_MS) {
    const recent = await jtag('data/list', { collection: 'chat_messages', filter: JSON.stringify({ replyToId: probeShortId }), orderBy: JSON.stringify([{ field: 'createdAt', direction: 'asc' }]), limit: 50 }) as { items?: Array<{ senderId?: string; senderName?: string; replyToId?: string }> };
    const replies = (recent?.items ?? []).filter((m) => m?.replyToId === probeShortId);
    const elapsedMs = Date.now() - startWait;

    for (const reply of replies) {
      const personaKey = reply.senderName || reply.senderId;
      if (!personaKey || personaResponseMs[personaKey] !== undefined) continue;
      personaResponseMs[personaKey] = elapsedMs;
      if (firstResponseMs === undefined) {
        firstResponseMs = elapsedMs;
        if (firstResponseMs > FIRST_RESPONSE_BUDGET_MS) {
          return failBucket(
            'first_response_budget_exceeded',
            `first persona reply at ${firstResponseMs}ms exceeded budget ${FIRST_RESPONSE_BUDGET_MS}ms`,
            { expectedPersonas, repliedPersonas: Object.keys(personaResponseMs).length, firstResponseMs, fullResponseMs: elapsedMs, personaResponseMs },
          );
        }
      }
    }

    if (Object.keys(personaResponseMs).length >= expectedPersonas) break;
    await sleep(POLL_INTERVAL_MS);
  }

  const repliedPersonas = Object.keys(personaResponseMs).length;
  const fullResponseMs = Date.now() - startWait;

  if (repliedPersonas === 0) {
    return failBucket(
      'no_personas_replied',
      `no persona replied to probe ${probeShortId} within ${ALL_RESPONSE_BUDGET_MS}ms — storm-fix may have over-corrected into total silence`,
      { expectedPersonas, repliedPersonas: 0, fullResponseMs, personaResponseMs },
    );
  }

  if (repliedPersonas < MIN_FAIR_RESPONSE_COUNT) {
    return failBucket(
      'fairness_violated',
      `only ${repliedPersonas} of ${expectedPersonas} expected personas replied (need >= ${MIN_FAIR_RESPONSE_COUNT}) — first-claim-wins coordination is too sticky`,
      { expectedPersonas, repliedPersonas, firstResponseMs, fullResponseMs, personaResponseMs },
    );
  }

  if (firstResponseMs === undefined) {
    return failBucket('no_personas_replied', 'unreachable: replied personas > 0 but first response never recorded');
  }

  if (fullResponseMs > ALL_RESPONSE_BUDGET_MS) {
    return failBucket(
      'all_response_budget_exceeded',
      `full reply set settled at ${fullResponseMs}ms exceeded budget ${ALL_RESPONSE_BUDGET_MS}ms`,
      { expectedPersonas, repliedPersonas, firstResponseMs, fullResponseMs, personaResponseMs },
    );
  }

  return {
    ok: true,
    success: {
      probe_short_id: probeShortId,
      expected_personas: expectedPersonas,
      replied_personas: repliedPersonas,
      first_response_ms: firstResponseMs,
      full_response_ms: fullResponseMs,
      persona_response_ms: personaResponseMs,
    },
  };
}

// =============================================================================
// Helpers
// =============================================================================

function failBucket(
  bucket: TimingFailureBucket,
  reason: string,
  observed?: { expectedPersonas: number; repliedPersonas: number; firstResponseMs?: number; fullResponseMs?: number; personaResponseMs: Record<string, number> },
): TimingResult {
  return {
    ok: false,
    failure: {
      bucket,
      reason,
      observed: observed
        ? {
            expected_personas: observed.expectedPersonas,
            replied_personas: observed.repliedPersonas,
            first_response_ms: observed.firstResponseMs,
            full_response_ms: observed.fullResponseMs,
            persona_response_ms: observed.personaResponseMs,
          }
        : undefined,
    },
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

// =============================================================================
// Entry point
// =============================================================================

async function main(): Promise<void> {
  console.log('💬  multi-persona-response-timing smoke starting…');
  const result = await runMultiPersonaResponseTimingSmoke();
  if (result.ok) {
    console.log('✅ PASS', JSON.stringify(result.success, null, 2));
    process.exit(0);
  }
  console.error('❌ FAIL bucket=' + result.failure.bucket);
  console.error('   reason: ' + result.failure.reason);
  if (result.failure.observed) {
    console.error('   observed:');
    console.error('     expected_personas:  ' + result.failure.observed.expected_personas);
    console.error('     replied_personas:   ' + result.failure.observed.replied_personas);
    if (result.failure.observed.first_response_ms !== undefined) {
      console.error('     first_response_ms:  ' + result.failure.observed.first_response_ms);
    }
    if (result.failure.observed.full_response_ms !== undefined) {
      console.error('     full_response_ms:   ' + result.failure.observed.full_response_ms);
    }
    console.error('     persona_response_ms:');
    for (const [persona, ms] of Object.entries(result.failure.observed.persona_response_ms)) {
      console.error(`       ${persona}: ${ms}ms`);
    }
  }
  process.exit(1);
}

if (require.main === module) {
  main().catch((e) => {
    console.error('❌ FAIL bucket=no_personas_replied (unhandled exception)');
    console.error(e);
    process.exit(1);
  });
}
