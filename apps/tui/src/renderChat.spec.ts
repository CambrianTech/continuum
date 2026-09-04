import { describe, it, expect } from 'vitest';
import type { ChatViewModel } from '@continuum/chat-view';
import { renderChat } from './renderChat';

/** A minimal view model builder so each test states only what it exercises. */
function vm(overrides: Partial<ChatViewModel> = {}): ChatViewModel {
  return {
    roomName: 'general',
    roomId: 'room-1',
    purpose: 'chat',
    memberCount: 0,
    activeCount: 0,
    members: [],
    messages: [],
    transcript: [],
    isEmpty: true,
    ...overrides,
  };
}

describe('renderChat (ANSI)', () => {
  it('renders an honest empty state, not an error, when there are no messages', () => {
    // what this catches: a blank/erroring conversation panel when a room is quiet
    // — the empty state is a normal render, matching the web widget's contract.
    const out = renderChat(vm({ isEmpty: true }), false);
    expect(out).toContain('No messages yet — say hello.');
    expect(out).not.toMatch(/error/i);
  });

  it('marks active vs idle members with distinct presence glyphs', () => {
    // what this catches: losing the live presence distinction (● active / ○ idle)
    // when projecting airc presence into the roster — the "WHO" panel's whole job.
    const out = renderChat(
      vm({
        memberCount: 2,
        activeCount: 1,
        members: [
          { id: 'a', name: 'Asha', kind: 'agent', active: true, runtime: '', vitals: {}, lastSeenMs: 0 },
          { id: 'b', name: 'Bo', kind: 'human', active: false, runtime: '', vitals: {}, lastSeenMs: 0 },
        ],
      }),
      false,
    );
    expect(out).toContain('● * Asha');
    expect(out).toContain('○ > Bo');
  });

  it('shows a runtime tag only when the substrate resolved one', () => {
    // what this catches: fabricating a runtime badge for an unresolved origin
    // (empty string must render nothing, never "[]") — no invented provenance.
    const out = renderChat(
      vm({
        memberCount: 2,
        activeCount: 2,
        members: [
          { id: 'a', name: 'Asha', kind: 'agent', active: true, runtime: 'claude', vitals: {}, lastSeenMs: 0 },
          { id: 'b', name: 'Nyx', kind: 'agent', active: true, runtime: '', vitals: {}, lastSeenMs: 0 },
        ],
      }),
      false,
    );
    expect(out).toContain('Asha [claude]');
    expect(out).toContain('Nyx');
    expect(out).not.toContain('[]');
  });

  it('renders a message line with its time, sender and content', () => {
    // what this catches: dropping any of the three fields that make a turn
    // readable in the "WHAT" panel (when / who / what was said).
    const out = renderChat(
      vm({
        isEmpty: false,
        messages: [
          {
            id: 'm1',
            senderId: 's1',
            senderName: 'Asha',
            kind: 'agent',
            content: 'hello there',
            time: '14:03',
            runtime: '',
          },
        ],
      }),
      false,
    );
    expect(out).toContain('14:03');
    expect(out).toContain('Asha');
    expect(out).toContain('hello there');
  });

  it('emits zero ANSI escape codes when colour is disabled', () => {
    // what this catches: colour bleeding into the testable/plain path — the pure
    // renderer must produce clean text so tests assert on content, and so a
    // non-TTY consumer (pipe, log) gets no escape noise.
    const out = renderChat(
      vm({
        isEmpty: false,
        memberCount: 1,
        activeCount: 1,
        members: [{ id: 'a', name: 'Asha', kind: 'agent', active: true, runtime: 'claude', vitals: {}, lastSeenMs: 0 }],
        messages: [
          {
            id: 'm1',
            senderId: 's1',
            senderName: 'Asha',
            kind: 'agent',
            content: 'hi',
            time: '14:03',
            runtime: 'claude',
          },
        ],
      }),
      false,
    );
    // eslint-disable-next-line no-control-regex
    expect(out).not.toMatch(/\x1b\[/);
  });

  it('shows an empty-roster placeholder rather than a bare "WHO" heading', () => {
    // what this catches: a naked section header with nothing under it when no one
    // has joined — the panel states its emptiness instead of looking broken.
    const out = renderChat(vm({ members: [] }), false);
    expect(out).toContain('(no one here yet)');
  });
});
