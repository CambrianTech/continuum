/**
 * The pattern-primitive proof: ONE projection, many render targets.
 *
 * The whole thesis of ACTIVITY-ROOM-PATTERNS.md is that a pattern is a
 * consumer-neutral transform — the human's UI and the persona's grounding are the
 * same projection rendered two ways. These tests build two minimal `RenderTarget`s
 * (a web-like HTML string, a RAG-like grounding block) and prove the SAME shapes
 * render correctly to both, that `Content` dispatches on room purpose, and that an
 * unregistered purpose fails loud.
 */

import { describe, it, expect } from 'vitest';
import {
  createContentRegistry,
  listingWidget,
  type ListingView,
  type ContentView,
  type WorkspaceView,
  type ContextPanelView,
  type ContentRegistry,
  type RenderTarget,
} from './index';

/** A sample roster projected to a Listing — the people-Listing the chat activity uses. */
const roster: ListingView = {
  id: 'roster',
  title: 'Users & Agents',
  cells: [
    { id: 'a', title: 'Asha', glyph: '🤖', badges: ['agent', 'persona'], status: 'active' },
    { id: 'j', title: 'Joel', glyph: '🧑', badges: ['human'], status: 'idle' },
  ],
};

/** A web-like target: the primitives → an HTML string. Content dispatches through the
 *  registry it is built with, so adding an activity is registering a renderer. */
function htmlTarget(content: ContentRegistry<string>): RenderTarget<string> {
  const self: RenderTarget<string> = {
    listing: (v) =>
      `<section class="listing" data-id="${v.id}"><h3>${v.title}</h3><ul>` +
      v.cells
        .map((c) => `<li data-status="${c.status ?? 'none'}">${c.glyph ?? ''} ${c.title}</li>`)
        .join('') +
      `</ul></section>`,
    content: (v) => content.render(v),
    contextPanel: (v) => v.listings.map((l) => self.listing(l)).join(''),
    widget: (w) => `<div class="widget" data-kind="${w.kind}">${self.listing(w.body as ListingView)}</div>`,
    workspace: (v) =>
      `<nav>${self.listing(v.nav)}</nav><aside>${v.left.map((w) => self.widget(w)).join('')}</aside>` +
      `<main>${self.content(v.content)}</main><section>${self.contextPanel(v.context)}</section>`,
  };
  return self;
}

/** A RAG target: the SAME shapes → a persona-grounding markdown block. */
function ragTarget(content: ContentRegistry<string>): RenderTarget<string> {
  const self: RenderTarget<string> = {
    listing: (v) =>
      `## ${v.title}\n` +
      v.cells
        .map(
          (c) =>
            `- ${c.title}${c.status === 'active' ? ' (here)' : ''}` +
            (c.badges?.length ? ` [${c.badges.join(', ')}]` : ''),
        )
        .join('\n'),
    content: (v) => content.render(v),
    contextPanel: (v) => v.listings.map((l) => self.listing(l)).join('\n\n'),
    widget: (w) => self.listing(w.body as ListingView),
    workspace: (v) =>
      `${self.listing(v.nav)}\n\n${v.left.map((w) => self.widget(w)).join('\n\n')}\n\n` +
      `${self.content(v.content)}\n\n${self.contextPanel(v.context)}`,
  };
  return self;
}

describe('pattern primitives — one projection, many render targets', () => {
  // what this catches: the SAME ListingView renders correctly to a web (HTML) target
  // AND a RAG (grounding) target — the consumer-neutrality thesis (eyes and mind from
  // one projection). If a primitive leaked a surface assumption, one would break.
  it('renders one Listing to both a web target and a RAG target', () => {
    const web = htmlTarget(createContentRegistry<string>()).listing(roster);
    expect(web).toContain('<h3>Users & Agents</h3>');
    expect(web).toContain('🤖 Asha');
    expect(web).toContain('data-status="active"');

    const rag = ragTarget(createContentRegistry<string>()).listing(roster);
    expect(rag).toBe('## Users & Agents\n- Asha (here) [agent, persona]\n- Joel [human]');
  });

  // what this catches: Content dispatches on room PURPOSE (the MIME handler) and fails
  // loud on an unregistered purpose — an unknown activity is a wiring bug, never a
  // silent blank ([[fallbacks-are-illegal-fail-loud]]).
  it('dispatches Content by purpose and fails loud on an unknown one', () => {
    const reg = createContentRegistry<string>();
    reg.register<{ text: string }>('chat', (b) => `conversation: ${b.text}`);
    reg.register<{ recipe: string }>('foundry', (b) => `config for ${b.recipe}`);

    expect(reg.render({ purpose: 'chat', body: { text: 'hi' } })).toBe('conversation: hi');
    expect(reg.render({ purpose: 'foundry', body: { recipe: 'qwen' } })).toBe('config for qwen');
    expect(() => reg.render({ purpose: 'scada', body: {} })).toThrow(
      /no content renderer for room purpose "scada"/,
    );
  });

  // what this catches: a full Workspace composes nav + left listings + purpose-content
  // + context on a target with NO bespoke per-activity code — the shell is identical
  // across activities; only Content/Context vary with the room's purpose.
  it('composes a full Workspace from the primitives on one target', () => {
    const content = createContentRegistry<string>();
    content.register<{ text: string }>('chat', (b) => `<p>${b.text}</p>`);
    const target = htmlTarget(content);

    const ws: WorkspaceView = {
      nav: { id: 'rooms', title: 'Rooms', cells: [{ id: 'general', title: 'general', status: 'active' }] },
      left: [listingWidget(roster)],
      content: { purpose: 'chat', body: { text: 'hello' } } satisfies ContentView<{ text: string }>,
      context: { listings: [] } satisfies ContextPanelView,
    };

    const out = target.workspace(ws);
    expect(out).toContain('general'); // nav (rooms-Listing == tab bar)
    expect(out).toContain('Asha'); // left people-Listing
    expect(out).toContain('<p>hello</p>'); // Content dispatched by purpose "chat"
  });
});
