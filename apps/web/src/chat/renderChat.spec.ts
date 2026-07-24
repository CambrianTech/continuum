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
import { chatStateFromEnvelope, chatViewModel, roomsListingFromNav, CHAT_KIND } from '@continuum/chat-view';
import { RoomsPanel } from '../render/RoomsPanel';
import type { ChatViewModel } from '@continuum/chat-view';
import type {
  ChatMessageView,
  ChatViewState,
  NavViewState,
  RosterSlotView,
  SenderKind,
  StateEnvelope,
} from '@continuum/sdk-typescript';
import { renderChat } from './renderChat';
import {
  LISTING_SELECT,
  avatarState,
  navSelectTarget,
  roomSelectTarget,
  type ListingSelectDetail,
} from '../render/parts';

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
  genes: [],
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
    expect(markup(withVitals)).toContain('meter-fill'); // the meter bar rendered

    // A member reporting no vitals → no meter markup at all.
    const noVitals = project({
      room_id: 'room-1',
      room_name: 'general',
      purpose: 'chat',
      roster: [member({ member_id: 'j', display_name: 'Joel', kind: kind('human'), vitals: {} })],
      messages: [],
    });
    expect(markup(noVitals)).not.toContain('meter-fill');
  });

  // what this catches: the QUE revival + the GENOME instrument panel. `queue`
  // present-at-0 must still DRAW its labelled empty track (the reference tile's
  // empty QUE row — an idle persona is visible, not blank); a member with live
  // vitals draws the four-slot genome panel, and a radiated gene NAME reaches
  // the lit slot's tooltip (real adapter names, never anonymous chips).
  it('draws the QUE track at zero and names lit genome slots from genes', () => {
    const view = project({
      room_id: 'room-1',
      room_name: 'general',
      purpose: 'chat',
      roster: [
        member({
          member_id: 'a',
          display_name: 'Asha',
          kind: kind('agent'),
          vitals: { activity: 0, queue: 0 },
          genes: ['rust-hands'],
        }),
      ],
      messages: [],
    });
    const chunks = flatten(renderChat(view));
    expect(chunks).toContain('QUE'); // the labelled track drew at 0
    const html = markup(view);
    expect(html).toContain('genome-panel');
    expect(html).toContain('genome-slot'); // the four equipment slots
    expect(html).toContain('rust-hands'); // the lit slot is NAMED by its gene
  });

  // what this catches: the avatar ring's state ladder — error outranks the live
  // token rail's `speaking` overlay, which outranks the (2s-sampled) radiator
  // thinking heuristic, which outranks bare presence. A mis-ranked ladder shows
  // a green idle ring on a persona mid-sentence.
  it('avatarState ranks error > speaking > thinking > presence', () => {
    expect(avatarState({ error: 1, speaking: 100 }, true)).toBe('error');
    expect(avatarState({ speaking: 100, reason: 90 }, true)).toBe('speaking');
    expect(avatarState({ reason: 90 }, true)).toBe('thinking');
    expect(avatarState({}, true)).toBe('active');
    expect(avatarState({}, false)).toBe('idle');
  });

  // what this catches: a member's LOADOUT must DRAW the model·size·ctx strip with
  // the renderer's unit formatting (raw 24_000_000_000 → "24B", 32768 → "32k ctx"),
  // while a member with no loadout draws NO strip (never a fabricated model line).
  // This is the "model size, context size" the glass-box tile surfaces.
  it('draws the loadout strip with unit formatting, and none without a loadout', () => {
    const withLoadout = project({
      room_id: 'room-1',
      room_name: 'general',
      purpose: 'chat',
      roster: [
        member({
          member_id: 'a',
          display_name: 'Asha',
          kind: kind('agent'),
          loadout: { model: 'devstral-24b', params: 24_000_000_000, context_window: 32_768 },
        }),
      ],
      messages: [],
    });
    expect(markup(withLoadout)).toContain('class="loadout"'); // the strip rendered
    const chunks = flatten(renderChat(withLoadout));
    expect(chunks).toContain('devstral-24b'); // model id, verbatim
    expect(chunks).toContain('24B'); // raw params → billions
    expect(chunks).toContain('32k ctx'); // raw window → k, labelled

    const noLoadout = project({
      room_id: 'room-1',
      room_name: 'general',
      purpose: 'chat',
      roster: [member({ member_id: 'j', display_name: 'Joel', kind: kind('human') })],
      messages: [],
    });
    expect(markup(noLoadout)).not.toContain('class="loadout"');
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

  // what this catches: brick 1's remainder — the rooms-rail cells must be
  // SELECTABLE: each cell carries a click handler that fires the composed
  // LISTING_SELECT event with the ROOMS listing id + that cell's room id (the
  // detail `roomSelectTarget` routes to `nav/select`). A regression here means
  // clicking a room dispatches nothing, or dispatches the wrong room. DOM-free:
  // the handlers are plucked from the template tree and invoked with a stub
  // currentTarget capturing the dispatched CustomEvent.
  it('rooms cells fire the select event with their room id when clicked', () => {
    const nav: NavViewState = {
      user_id: 'me',
      current_tab: 'room-1',
      open_tabs: [
        { id: 'room-1', title: 'general', kind: 'room', unread: 0, purpose: 'chat' },
        { id: 'room-2', title: 'code', kind: 'room', unread: 3, purpose: 'chat' },
      ],
      last_read: {},
      bookmarks: [],
    };
    // The rooms cells render inside <rooms-panel> (the dense rooms section);
    // its render() is a plain template — walk THAT tree for the handlers, the
    // same structural pluck as before, no document mount needed.
    const panel = new RoomsPanel();
    panel.view = roomsListingFromNav(nav, 'room-1');
    const handlers: ((e: Event) => void)[] = [];
    const collect = (node: unknown): void => {
      if (typeof node === 'function') {
        handlers.push(node as (e: Event) => void);
      } else if (Array.isArray(node)) {
        for (const child of node as readonly unknown[]) collect(child);
      } else if (typeof node === 'object' && node !== null && isTemplateLike(node)) {
        for (const v of node.values) collect(v);
      }
    };
    collect(panel.render());
    expect(handlers.length).toBeGreaterThan(0);

    // Invoke each handler with a stub currentTarget capturing what it fires.
    // Keydown handlers no-op (the stub event carries no Enter key), the facet
    // buttons' click handlers take no event and fire nothing here; the cell
    // click handlers must each fire ONE ListingSelect for their own cell.
    const fired: CustomEvent<ListingSelectDetail>[] = [];
    const stubEvent = {
      currentTarget: {
        dispatchEvent: (ev: Event): boolean => {
          fired.push(ev as CustomEvent<ListingSelectDetail>);
          return true;
        },
      },
    } as unknown as Event;
    for (const handler of handlers) handler(stubEvent);

    const selects = fired.filter((ev) => ev.type === LISTING_SELECT);
    expect(selects.map((ev) => ev.detail)).toEqual([
      { listingId: 'rooms', id: 'room-1', group: 'room' },
      { listingId: 'rooms', id: 'room-2', group: 'room' },
    ]);
    // …and the widget-side router turns exactly the rooms detail into a switch
    // target; a roster pick is NOT a room switch (it routes as a persona select).
    expect(roomSelectTarget({ listingId: 'rooms', id: 'room-2', group: 'room' })).toBe('room-2');
    expect(roomSelectTarget({ listingId: 'roster', id: 'asha' })).toBeNull();
  });

  // what this catches: the kind-aware select routing (`navSelectTarget`) — the
  // ONE rule that decides what a listing pick dispatches. A rooms-rail pick
  // routes by the cell's group (the nav tab's target kind): persona tabs open
  // the persona HOME (kind 'persona'), room tabs switch rooms; a ROSTER pick
  // (a citizen's tile) IS the persona select; content tabs and unknown listings
  // stay inert. Regression here = a persona click hijacks the room, or a
  // roster click goes dead.
  it('navSelectTarget routes room picks to rooms and persona picks to the persona home', () => {
    expect(navSelectTarget({ listingId: 'rooms', id: 'room-2', group: 'room' })).toEqual({
      target: 'room-2',
      kind: 'room',
    });
    expect(navSelectTarget({ listingId: 'rooms', id: 'asha', group: 'persona' })).toEqual({
      target: 'asha',
      kind: 'persona',
    });
    expect(navSelectTarget({ listingId: 'roster', id: 'asha' })).toEqual({
      target: 'asha',
      kind: 'persona',
    });
    expect(navSelectTarget({ listingId: 'rooms', id: 'doc-1', group: 'content' })).toBeNull();
    expect(navSelectTarget({ listingId: 'nodes', id: 'local' })).toBeNull();
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
