/**
 * historyRowsFromPoll spec — the storage-page → transcript-row mapping.
 */

import { describe, it, expect } from 'vitest';
import { historyRowsFromPoll } from './historyProjections';
import type { RosterMemberVM } from './chatViewModel';

const member = (id: string, name: string): RosterMemberVM => ({
  id,
  name,
  kind: 'agent',
  active: true,
  runtime: '',
  vitals: {},
  lastSeenMs: 0,
  genes: [],
});

const stored = (id: string, senderId: string, text: string, ts = '2026-07-30T04:05:00Z') => ({
  id,
  roomId: 'room-1',
  senderId,
  timestamp: ts,
  content: { text },
  metadata: { source: 'user' },
  status: 'sent',
});

describe('historyRowsFromPoll', () => {
  // what this catches: the storage entity shape ({content:{text}, ISO
  // timestamp, no sender name}) must land on the SAME MessageRowVM the live
  // tail renders — roster-resolved name/kind, deterministic UTC HH:MM time.
  // A regression here makes scrolled-back history render blank or alien.
  it('maps stored entities onto transcript rows with roster identity', () => {
    const rows = historyRowsFromPoll(
      [stored('m1', 'asha-id', 'older words')],
      [member('asha-id', 'Asha')],
    );
    expect(rows).toEqual([
      {
        id: 'm1',
        senderId: 'asha-id',
        senderName: 'Asha',
        kind: 'agent',
        content: 'older words',
        time: '04:05',
        runtime: '',
      },
    ]);
  });

  // what this catches: an off-roster sender (departed member, pre-roster
  // history) must degrade honestly — short-id name, metadata.source deciding
  // human vs agent — never a crash and never a fabricated name.
  it('falls back to short-id + metadata.source for unknown senders', () => {
    const rows = historyRowsFromPoll(
      [stored('m2', 'aaaabbbb-cccc-dddd-eeee-ffff00001111', 'hi')],
      [],
    );
    expect(rows[0]?.senderName).toBe('aaaabbbb');
    expect(rows[0]?.kind).toBe('human'); // metadata.source === 'user'
  });

  // what this catches: the live-tail dedup — a timestamp-tie page can return
  // a row already on screen; it must be dropped, and malformed records must
  // be skipped without poisoning the rest of the page.
  it('dedups against the live tail and skips malformed records', () => {
    const rows = historyRowsFromPoll(
      [stored('dup', 's', 'seen'), { notAnEntity: true }, stored('ok', 's', 'kept')],
      [],
      new Set(['dup']),
    );
    expect(rows.map((r) => r.id)).toEqual(['ok']);
  });
});
