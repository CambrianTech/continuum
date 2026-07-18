/**
 * chatApp spec — the chat portal, defined ONCE, rendered through a RenderTarget.
 *
 * Proves the framework path end-to-end for a REAL activity: `chatApp` (a `defineApp`)
 * projects a `ChatState` snapshot → the neutral who/what/where `WorkspaceView`, and a
 * `RenderTarget` draws it. Browser-free — the target here is a string stand-in for
 * web/mobile/RAG, so this pins the define-once composition without a DOM.
 */

import { describe, it, expect } from 'vitest';
import {
  mount,
  createContentRegistry,
  type RenderTarget,
  type WorkspaceView,
  type ListingView,
  type PanelWidget,
  type ContentView,
  type ContextPanelView,
  type AppSource,
} from '@continuum/patterns';
import { chatApp } from './chatApp';
import type { ChatState, ChatContentBody } from './index';
import type { ChatMessageView, RosterSlotView, SenderKind } from '@continuum/sdk-typescript';

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

const chatState = (over: Partial<ChatState> = {}): ChatState => ({
  kind: 'chat',
  revision: 3,
  room_id: 'room-1',
  room_name: 'general',
  purpose: 'chat',
  messages: [],
  roster: [],
  ...over,
});

/** A string RenderTarget — a browser-free stand-in for web/mobile/RAG. Stringifies the
 *  content body so we assert on projected text without pinning VM field names. */
function stringTarget(): RenderTarget<string> {
  const content = createContentRegistry<string>();
  content.register<ChatContentBody>('chat', (b) => JSON.stringify(b.messages));
  const listing = (v: ListingView): string => `${v.title}[${v.cells.map((c) => c.title).join(',')}]`;
  // A rail widget renders its listing body, or its title for a non-listing kind (metrics).
  const widget = (w: PanelWidget): string =>
    w.kind === 'listing' ? listing(w.body as ListingView) : `<${w.title}>`;
  return {
    listing,
    content: (v: ContentView) => content.render(v),
    contextPanel: (v: ContextPanelView) => `ctx:${v.listings.length}`,
    widget,
    workspace: (v: WorkspaceView) =>
      `nav=${listing(v.nav)} who=${v.left.map(widget).join('|')} what=${content.render(v.content)}`,
  };
}

describe('chatApp — the chat portal defined once, rendered on a target', () => {
  // what this catches: chatApp composes chatViewModel + chatWorkspace into a defineApp
  // whose project() yields a WorkspaceView a RenderTarget draws to who/what/where. A
  // broken composition (wrong order, dropped purpose) would fail-loud on content dispatch
  // or lose the roster/room/messages — this is the framework path proven for a real activity.
  it('projects a chat snapshot to who / what / where through a target', () => {
    const state = chatState({
      roster: [
        member({ member_id: 'a', display_name: 'Asha' }),
        member({ member_id: 'b', display_name: 'Solenne' }),
      ],
      messages: [message({ id: 'x', sender_name: 'Asha', content: 'working on vitals' })],
    });
    const src: AppSource<ChatState> = (onState) => {
      onState(state);
      return () => {};
    };

    let out = '';
    mount(chatApp, src, stringTarget(), (o) => (out = o));

    expect(out).toContain('Asha'); // who
    expect(out).toContain('Solenne'); // who
    expect(out).toContain('general'); // where (room in nav)
    expect(out).toContain('working on vitals'); // what (the message content survives projection)
  });
});
