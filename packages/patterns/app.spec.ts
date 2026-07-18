import { describe, it, expect } from 'vitest';
import {
  defineApp,
  mount,
  createContentRegistry,
  listingWidget,
  type WorkspaceView,
  type RenderTarget,
  type ListingView,
  type PanelWidget,
  type ContentView,
  type ContextPanelView,
  type AppSource,
} from './index';

/** A tiny domain state for the test — the app's input. */
interface RoomState {
  room: string;
  people: string[];
  message: string;
}

/** The app, defined ONCE: project domain state → the neutral who/what/where view-model.
 *  Zero dependency on any target or SDK — the whole point. */
const app = defineApp<RoomState>({
  universe: 'test',
  project: (s): WorkspaceView => ({
    nav: { id: 'rooms', title: 'Rooms', cells: [{ id: s.room, title: s.room }] },
    left: [
      listingWidget({
        id: 'roster',
        title: 'Users & Agents',
        cells: s.people.map((p) => ({ id: p, title: p })),
      }),
    ],
    content: { purpose: 'chat', body: { text: s.message } },
    context: { listings: [] },
  }),
});

/** Two DIFFERENT render targets over the SAME view-model — the outliers that validate
 *  "define once → many modalities". Each owns its content registry (per-target renderers). */
function makeTarget(label: string): RenderTarget<string> {
  const content = createContentRegistry<string>();
  content.register<{ text: string }>('chat', (b) => `${label}:msg(${b.text})`);
  const listing = (v: ListingView) =>
    `${label}:list(${v.title}:${v.cells.map((c) => c.title).join(',')})`;
  // A rail widget renders its listing body (the only kind this test uses).
  const widget = (w: PanelWidget) => listing(w.body as ListingView);
  return {
    listing,
    content: (v: ContentView) => content.render(v),
    contextPanel: (v: ContextPanelView) => `${label}:ctx(${v.listings.length})`,
    widget,
    workspace: (v: WorkspaceView) =>
      `${label}:ws[nav=${listing(v.nav)} left=${v.left.map(widget).join('|')} ${content.render(v.content)}]`,
  };
}

/** A source that pushes one state synchronously; returns a counting teardown. */
function oneShotSource(state: RoomState, onTeardown?: () => void): AppSource<RoomState> {
  return (onState) => {
    onState(state);
    return () => onTeardown?.();
  };
}

describe('defineApp / mount — define once, render on every modality', () => {
  // what this catches: the framework keystone. ONE AppDefinition renders through TWO
  // distinct RenderTargets off the SAME project(). If mount coupled the app to a target
  // (baked a DOM/Lit/Flutter assumption into defineApp), the two outputs could not diverge
  // by target while sharing an identical projection. This is the "web + mobile + RAG from
  // one app" proof at the contract level — before any real Lit/Flutter renderer exists.
  it('renders one app definition through two distinct targets', () => {
    const state: RoomState = { room: 'general', people: ['Asha', 'Solenne'], message: 'hi' };
    const src = oneShotSource(state);

    let web = '';
    let rag = '';
    mount(app, src, makeTarget('WEB'), (out) => (web = out));
    mount(app, src, makeTarget('RAG'), (out) => (rag = out));

    // Same projection (people + message present in both); different target formatting.
    expect(web).toContain('Asha,Solenne');
    expect(web).toContain('msg(hi)');
    expect(web.startsWith('WEB:')).toBe(true);
    expect(rag.startsWith('RAG:')).toBe(true);
    // The two outputs are IDENTICAL except the target label — same who/what/where,
    // different surface. That equivalence IS "define once → all modalities".
    expect(web.replace(/WEB/g, 'X')).toBe(rag.replace(/RAG/g, 'X'));
  });

  // what this catches: mount must return a working teardown (unsubscribes the source),
  // or long-lived mounts leak subscriptions on route/room changes.
  it('mount returns a teardown that unsubscribes the source', () => {
    let torn = 0;
    const src = oneShotSource({ room: 'r', people: [], message: '' }, () => (torn += 1));
    const stop = mount(app, src, makeTarget('T'), () => {});
    stop();
    expect(torn).toBe(1);
  });
});
