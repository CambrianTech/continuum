/**
 * messageDigest unit spec — the transcript's digest tier
 * (PERCEPTION-RESOLUTION-CONTRACT: no layer may flood).
 *
 * Pins the MECHANICAL classification: under-threshold bodies stay full tier;
 * over-threshold bodies project head + exact tail counts; a repetition-dominated
 * remainder surfaces the `uniq -c` histogram. The live incident this regresses:
 * a persona's degenerate repetition wall (hundreds of "ae0e-" tokens) rendered
 * full-height in the web chat, flooding every human in #general.
 */

import { describe, it, expect } from 'vitest';
import {
  messageDigest,
  DIGEST_OVER_CHARS,
  DIGEST_OVER_LINES,
  DIGEST_HEAD_LINES,
  DIGEST_HEAD_CHARS,
} from './messageDigest';
import { chatViewModel } from './chatViewModel';
import type { ChatState } from './ChatState';

describe('messageDigest', () => {
  // what this catches: a normal chat message must NOT be digested — collapsing
  // ordinary conversation would make the transcript unreadable. The full tier
  // is the default; the digest is the exception for floods.
  it('leaves a short message on the full tier', () => {
    expect(messageDigest('hello')).toBeUndefined();
    expect(messageDigest('a'.repeat(DIGEST_OVER_CHARS))).toBeUndefined(); // at the bound, not over
    expect(messageDigest(Array(DIGEST_OVER_LINES).fill('line').join('\n'))).toBeUndefined();
  });

  // what this catches: the line-count trigger — a message one line over the
  // bound digests, with the head capped at DIGEST_HEAD_LINES and the tail
  // summary counting EXACTLY the collapsed lines/chars (mechanical, verifiable).
  it('digests an over-line message with exact tail counts', () => {
    const lines = Array.from({ length: DIGEST_OVER_LINES + 1 }, (_, i) => `line-${i}`);
    const digest = messageDigest(lines.join('\n'));
    expect(digest).toBeDefined();
    expect(digest?.head).toBe(lines.slice(0, DIGEST_HEAD_LINES).join('\n'));
    const collapsed = lines.slice(DIGEST_HEAD_LINES).join('\n');
    expect(digest?.tailSummary).toBe(
      `… +${lines.length - DIGEST_HEAD_LINES} lines (${collapsed.length} chars)`,
    ); // counts stay under 1,000 here — grouping is pinned separately below.
    // distinct lines: no single unit dominates, so no fabricated histogram.
    expect(digest?.histogram).toBeUndefined();
  });

  // what this catches: the char-count trigger — one unbroken 2,000-char line has
  // only 1 line but still floods a 68ch bubble; the head must cap at
  // DIGEST_HEAD_CHARS and the remainder counts as one line.
  it('digests an over-char single-line message, capping the head by chars', () => {
    const digest = messageDigest('x'.repeat(2000));
    expect(digest).toBeDefined();
    expect(digest?.head).toBe('x'.repeat(DIGEST_HEAD_CHARS));
    expect(digest?.tailSummary).toBe('… +1 line (1,700 chars)');
  });

  // what this catches: the incident shape — a remainder dominated by ONE
  // repeated line must surface the `uniq -c` histogram ("mostly N× '…'") so a
  // degenerate wall is NAMED, not just hidden. Counts are exact.
  it('surfaces a repetition histogram when one line dominates the remainder', () => {
    const wall = ['deploy trace follows:', ...Array<string>(219).fill('ae0e-')].join('\n');
    const digest = messageDigest(wall);
    expect(digest).toBeDefined();
    // head = title + first 5 wall lines; remainder = the other 214 repeats
    // (214×"ae0e-" joined by newlines = 214·5 + 213 = 1,283 chars).
    expect(digest?.histogram).toBe(`mostly 214× 'ae0e-'`);
    expect(digest?.tailSummary).toBe('… +214 lines (1,283 chars)');
  });

  // what this catches: the token-shaped wall (repeats separated by spaces, not
  // newlines) must ALSO be caught — the live wall was token-joined. The
  // whitespace-token pass is the fallback when no line dominates.
  it('surfaces a repetition histogram for a space-separated token wall', () => {
    const wall = `trace:\n${Array(400).fill('ae0e-').join(' ')}`;
    const digest = messageDigest(wall);
    expect(digest).toBeDefined();
    expect(digest?.histogram).toMatch(/^mostly \d+× 'ae0e-'$/);
  });

  // what this catches: a long but varied remainder must NOT get a histogram —
  // claiming "mostly N× …" about non-repetitive text would be a fabricated
  // pattern, the exact dishonesty the mechanical rule exists to avoid.
  it('omits the histogram when nothing dominates', () => {
    const varied = Array.from({ length: 40 }, (_, i) => `unique line number ${i} with detail`);
    expect(messageDigest(varied.join('\n'))?.histogram).toBeUndefined();
  });

  // what this catches: big counts must group thousands deterministically
  // ("38,102", never locale-dependent) — the tail summary is asserted in
  // screenshots and must render identically on every machine.
  it('groups thousands in the tail summary deterministically', () => {
    // 1,994 collapsed lines of 20 chars + 1,993 joining newlines = 41,873 chars.
    const digest = messageDigest(Array(2000).fill('aaaaaaaaaaaaaaaaaaaa').join('\n'));
    expect(digest?.tailSummary).toBe('… +1,994 lines (41,873 chars)');
  });

  // what this catches: a long repeated unit must be elided in the histogram
  // quote — the histogram itself must never become a second flood.
  it('elides a long repeated unit in the histogram quote', () => {
    const unit = 'this-single-repeated-line-is-much-longer-than-the-sample-cap';
    const digest = messageDigest(Array(50).fill(unit).join('\n'));
    // The char-capped head cuts mid-line, leaving a partial line + 45 full
    // repeats in the remainder — 45/46 dominates, quoted elided at 24 chars.
    expect(digest?.histogram).toBe(`mostly 45× '${unit.slice(0, 24)}…'`);
  });

  // what this catches: the digest cutting a message MID-FENCE — the head ended
  // with a dangling ``` that rendered as literal backtick noise instead of a
  // code card (live 2026-07-30: Claude's wordstats reply in cambriantech).
  it('never splits a fenced code block: code-heavy messages render full', () => {
    const fence = `\`\`\`rust\n${Array(30).fill('let x = 1;').join('\n')}\n\`\`\``;
    const msg = `Here is the code:\n\n${fence}\n\nThe key change is the entry ownership.`;
    // 35+ raw lines, but the projection counts the fence as ONE line — under
    // both bounds, so no digest at all; the code card handles its own size.
    expect(messageDigest(msg)).toBeUndefined();
  });

  // what this catches: prose floods still digest, but the head cut treats a
  // fence as atomic — the whole block survives into the head, never a partial.
  it('keeps a whole fence in the digest head when prose around it floods', () => {
    const fence = '```rust\nlet x = 1;\nlet y = 2;\n```';
    const prose = Array(40).fill('a line of prose that keeps going').join('\n');
    const digest = messageDigest(`intro\n${fence}\n${prose}`);
    expect(digest).toBeDefined();
    expect(digest?.head).toContain(fence);
    // Non-negotiable: no unterminated fence in the head.
    expect((digest?.head.match(/```/g) ?? []).length % 2).toBe(0);
  });

  // what this catches: an unterminated fence (streaming / author forgot to
  // close) swallowing the head cut — it must extend to end-of-message, not
  // leave a dangling opener.
  it('treats an unterminated fence as running to end of message', () => {
    const msg = `look:\n\`\`\`bash\n${Array(40).fill('echo hi').join('\n')}`;
    expect(messageDigest(msg)).toBeUndefined();
  });
});

describe('chatViewModel digest projection', () => {
  const state = (content: string): ChatState => ({
    kind: 'chat',
    revision: 1,
    room_id: 'room-1',
    room_name: 'general',
    purpose: 'chat',
    roster: [],
    acts: [],
    messages: [
      {
        id: 'msg-1',
        room_id: 'room-1',
        sender_id: 's-1',
        sender_name: 'Asha',
        sender_kind: { kind: 'agent' },
        integrations: {},
        provenance: { runtime: 'devstral' },
        content,
        timestamp: 0,
      },
    ],
  });

  // what this catches: the projection must stamp the digest tier onto the row
  // (so EVERY renderer inherits flood-proofing) while `content` still carries
  // the untouched original — the digest defers fidelity, never destroys it.
  it('stamps a digest onto an over-threshold row, keeping content verbatim', () => {
    const wall = Array(300).fill('ae0e-').join('\n');
    const row = chatViewModel(state(wall)).messages[0];
    expect(row?.digest).toBeDefined();
    expect(row?.digest?.histogram).toContain(`× 'ae0e-'`);
    expect(row?.content).toBe(wall);
    // a normal message carries no digest field at all — full tier.
    expect(chatViewModel(state('hello')).messages[0]?.digest).toBeUndefined();
  });
});
