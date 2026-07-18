/**
 * renderChat (Lit) unit spec — the web surface of the shared chat seam.
 *
 * The terminal twin of apps/tui's renderChat.spec: same input (a `ChatViewModel`
 * from the SHARED `@continuum/chat-view`), a totally different output (a Lit
 * `TemplateResult`, not an ANSI string). Together the two specs prove the
 * north-star's 2/3 — web and terminal render the IDENTICAL seam-produced model,
 * differing only in surface.
 *
 * To keep this DOM-free (no jsdom, no `@lit-labs/ssr`), it drives the model
 * through the REAL read pipe both clients run — `chatViewModel ∘
 * chatStateFromEnvelope` off a `StateEnvelope` — then flattens the returned
 * template tree (its static `strings` + interpolated `values`, recursively) and
 * asserts every model fact reaches the markup. That proves the web renderer
 * faithfully carries what the seam produced, without a browser.
 */

import { describe, it, expect } from 'vitest';
import { chatStateFromEnvelope, chatViewModel, CHAT_KIND } from '@continuum/chat-view';
import type { ChatViewModel } from '@continuum/chat-view';
import type {
  ChatMessageView,
  ChatViewState,
  RosterSlotView,
  SenderKind,
  StateEnvelope,
} from '@continuum/sdk-typescript';
import { renderChat } from './renderChat';

const kind = (k: SenderKind['kind']): SenderKind => ({ kind: k });

const member = (over: Partial<RosterSlotView> = {}): RosterSlotView => ({
  member_id: 'm-1',
  display_name: 'Asha',
  kind: kind('agent'),
  integrations: {},
  provenance: { runtime: '' },
  active: true,
  last_seen_ms: 0,
  vitals: {},
  ...over,
});

const message = (over: Partial<ChatMessageView> = {}): ChatMessageView => ({
  id: 'msg-1',
  room_id: 'room-1',
  sender_id: 's-1',
  sender_name: 'Joel',
  sender_kind: kind('human'),
  integrations: {},
  provenance: { runtime: '' },
  content: 'hello',
  timestamp: 0,
  ...over,
});

/** Project a `ChatViewState` payload through the exact pipe apps/web's index.ts
 *  runs, so the renderer is fed what the seam actually produces. */
const project = (payload: ChatViewState): ChatViewModel => {
  const env: StateEnvelope = { kind: CHAT_KIND, revision: 1, layer: 'ephemeral', payload };
  return chatViewModel(chatStateFromEnvelope(env));
};

/** A Lit `TemplateResult` exposes its static `strings` and interpolated `values`.
 *  We read them structurally rather than render to DOM. */
interface LitTemplateLike {
  readonly strings: readonly string[];
  readonly values: readonly unknown[];
}
function isTemplateLike(node: object): node is LitTemplateLike {
  return 'strings' in node && 'values' in node;
}

/** Flatten a Lit template tree to every string that would reach the markup — the
 *  static chunks plus every interpolated primitive, following nested templates
 *  and `.map` arrays. `nothing`/symbols/objects with no text contribute nothing. */
function flatten(node: unknown, out: string[] = []): string[] {
  if (typeof node === 'string') {
    out.push(node);
  } else if (typeof node === 'number') {
    out.push(String(node));
  } else if (Array.isArray(node)) {
    for (const child of node as readonly unknown[]) flatten(child, out);
  } else if (typeof node === 'object' && node !== null && isTemplateLike(node)) {
    for (const s of node.strings) out.push(s);
    for (const v of node.values) flatten(v, out);
  }
  return out;
}

/** The full flattened markup text of a rendered view model. */
const markup = (vm: ChatViewModel): string => flatten(renderChat(vm)).join('');

describe('renderChat (Lit)', () => {
  // what this catches: the three panels must all reach the markup from one
  // seam-projected snapshot — room (where), each roster member (who), each
  // message (what). A regression dropping a facet would blank a whole panel, and
  // it must match the terminal renderer's contract fact-for-fact.
  it('renders room, roster, and messages from a seam-projected snapshot', () => {
    const vm = project({
      room_id: 'room-1',
      room_name: 'general', purpose: 'chat',
      roster: [
        member({ member_id: 'asha', display_name: 'Asha', kind: kind('agent'), provenance: { runtime: 'claude' } }),
        member({ member_id: 'joel', display_name: 'Joel', kind: kind('human'), active: false }),
      ],
      messages: [
        message({ id: 'm1', sender_name: 'Joel', sender_kind: kind('human'), content: 'hi Asha', timestamp: 0 }),
      ],
    });
    const chunks = flatten(renderChat(vm));

    // Every model fact is interpolated, so it appears as its OWN chunk. flatten
    // pushes all static template strings first, then all interpolated values —
    // static/value adjacency is NOT preserved — so assert exact chunk membership,
    // never a joined-string substring. (A substring '1' would be satisfied by the
    // '1' in 'room-1'; a substring 'active' by the header's static
    // title="active / total" — both would pass with a broken count/roster.)
    // WHERE: room identity + the active/total counts.
    expect(chunks).toContain('general'); // roomName
    expect(chunks).toContain('room-1'); // roomId
    expect(chunks).toContain('1'); // activeCount value (1 of 2 present at snapshot)
    expect(chunks).toContain('2'); // memberCount value
    // WHO: both members, each presence rendered as its own 'active'/'idle' value.
    expect(chunks).toContain('Asha');
    expect(chunks).toContain('Joel');
    expect(chunks).toContain('active'); // Asha present
    expect(chunks).toContain('idle'); // Joel idle
    // WHAT: the turn's sender, content and time all reach the markup as values.
    expect(chunks).toContain('hi Asha');
    expect(chunks).toContain('00:00');
  });

  // what this catches: a member's live vitals must actually DRAW a stat meter — a
  // labelled, width-filled bar reaches the markup — while a member reporting no
  // vitals draws NONE (never a fabricated bar). The chatViewModel spec proves the
  // VM carries the field; this proves the renderer turns it into a meter. The tile's
  // stat row draws SPD (speed) + PAR (params = model size, the LOADOUT figure); this
  // pins the `speed` vital → the SPD meter.
  it('draws a persona stat meter per vital, and none for a member without vitals', () => {
    const withVitals = project({
      room_id: 'room-1',
      room_name: 'general',
      purpose: 'chat',
      roster: [member({ member_id: 'a', display_name: 'Asha', kind: kind('agent'), vitals: { speed: 80 } })],
      messages: [],
    });
    const chunks = flatten(renderChat(withVitals));
    expect(chunks).toContain('SPD'); // the stat label (speed → SPD)
    expect(chunks).toContain('80'); // the fill width value (single member → unambiguous)
    expect(markup(withVitals)).toContain('stat-fill'); // the meter bar rendered

    // A member reporting no vitals → no meter markup at all.
    const noVitals = project({
      room_id: 'room-1',
      room_name: 'general',
      purpose: 'chat',
      roster: [member({ member_id: 'j', display_name: 'Joel', kind: kind('human'), vitals: {} })],
      messages: [],
    });
    expect(markup(noVitals)).not.toContain('stat-fill');
  });

  // what this catches: an empty conversation must draw the honest empty-state
  // string, not an error and not a bare void — matching the ANSI renderer's
  // "No messages yet — say hello." contract exactly.
  it('renders an honest empty state when there are no messages', () => {
    const vm = project({ room_id: 'room-1', room_name: 'general', purpose: 'chat', roster: [], messages: [] });
    const text = markup(vm);
    expect(text).toContain('No messages yet — say hello.');
    expect(text).not.toMatch(/error/i);
  });

  // what this catches: a runtime badge must appear ONLY when the substrate
  // resolved an origin — an unresolved '' runtime must inject no badge (no
  // fabricated provenance, [[positron-identity-security-first-class]]). The badge
  // markup (`class="runtime"`) must appear exactly once for one resolved member.
  it('badges a resolved runtime once and never fabricates one for the unresolved', () => {
    const vm = project({
      room_id: 'room-1',
      room_name: 'general', purpose: 'chat',
      roster: [
        member({ member_id: 'asha', display_name: 'Asha', provenance: { runtime: 'claude' } }),
        member({ member_id: 'nyx', display_name: 'Nyx', provenance: { runtime: '' } }),
      ],
      messages: [],
    });
    const chunks = flatten(renderChat(vm));
    const badgeCount = chunks.filter((c) => c.includes('class="runtime"')).length;
    expect(badgeCount).toBe(1); // only Asha's resolved origin, never Nyx's ''
    expect(chunks.join('')).toContain('claude');
  });
});
