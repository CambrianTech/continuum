/**
 * `webTarget` — positron's web `RenderTarget`. Lit paints; positron defines.
 *
 * Draws the neutral `WorkspaceView` (who/what/where) to Lit `TemplateResult`s: the
 * header + roster `Listing` (as member cards) + the center dispatched by room purpose
 * through the web Content registry. This is the piece that lets `apps/web` flow through
 * the framework — `renderChat` now delegates here, and `mount(chatApp, …, webTarget, …)`
 * becomes the composition root. Byte-identical to the former inline `renderChat` markup,
 * verified by the before/after screenshot of the live three-panel.
 */

import { html, nothing, type TemplateResult } from 'lit';
import {
  LIVE_PURPOSE,
  ROSTER_LISTING_ID,
  type ContinuonView,
  type RenderTarget,
  type WorkspaceView,
  type ListingCell,
  type ListingView,
  type ContentView,
  type ContextPanelView,
  type PanelWidget,
  type WorkspaceChrome,
} from '@continuum/patterns';
import {
  fireListingSelect,
  fireLiveFaceToggle,
  fireNavTabClose,
  renderListing,
  resizeHandle,
} from './parts';
import { webContentRegistry } from '../content/registry';
import { webWidgetRegistry } from './widgets';

/** The universe skins the Theme button cycles through — the SAME real
 *  `?universe=` axis `<chat-widget>` already keys its skins off ('' = the
 *  native continuum look). Cycling rewrites the query param, which re-embodies
 *  the app — a real action, not a dead chrome button. */
const UNIVERSES = ['', 'tron', 'ares', 'warcraft', 'crystal', 'cuddly', 'crt', 'forge', 'cosmos'] as const;

/** Advance the ?universe= query param to the next skin (pure on its input so
 *  the cycle order is unit-testable; the caller applies it to location). */
export function nextUniverse(current: string | null): string {
  const idx = UNIVERSES.indexOf((current ?? '') as (typeof UNIVERSES)[number]);
  return UNIVERSES[(idx + 1) % UNIVERSES.length] ?? '';
}

function cycleUniverse(): void {
  const params = new URLSearchParams(location.search);
  const next = nextUniverse(params.get('universe'));
  if (next === '') params.delete('universe');
  else params.set('universe', next);
  const query = params.toString();
  location.search = query;
}

/** Tab icon by the nav cell's group (the tab's target kind / purpose). */
function tabIcon(group: string | undefined): string {
  switch (group) {
    case 'persona':
      return '🤖';
    case 'content':
      return '📄';
    case 'foundry':
      return '🏭';
    default:
      return '💬';
  }
}

/** One nav tab — icon + title + unread pill + a LIVE close affordance.
 *  Clicking the tab fires the SAME composed LISTING_SELECT the rooms rail uses
 *  (listingId 'rooms'), so a tab pick IS a real nav/select round-trip. The ×
 *  fires NAV_TAB_CLOSE (→ `nav/close`) on NON-ROOM tabs; a room tab draws no ×
 *  — the room set is membership, not tab state. */
function navTab(cell: ListingCell): TemplateResult {
  // The cell's group carries the tab's target KIND — riding the detail so the
  // routing rule (`navSelectTarget`) picks room vs persona select.
  const select = (e: Event): void => {
    fireListingSelect(e, 'rooms', cell.id, cell.group);
  };
  const keySelect = (e: KeyboardEvent): void => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      select(e);
    }
  };
  const close = (e: Event): void => {
    // Never also fire the tab's select — closing is not focusing.
    e.stopPropagation();
    fireNavTabClose(e, cell.id);
  };
  return html`<span
    class="tab"
    data-status=${cell.status ?? 'none'}
    role="tab"
    tabindex="0"
    aria-selected=${cell.status === 'active' ? 'true' : 'false'}
    @click=${select}
    @keydown=${keySelect}
  >
    <span class="tab-icon">${tabIcon(cell.group)}</span>
    <span class="tab-title">${cell.title}</span>
    ${cell.count ? html`<span class="cell-count" title="unread">${cell.count}</span>` : nothing}
    ${cell.group !== undefined && cell.group !== 'chat' && cell.group !== 'room'
      ? html`<button class="tab-close" title="Close tab" aria-label="Close ${cell.title}" @click=${close}>×</button>`
      : nothing}
  </span>`;
}

/** The continuon widget's version badge, reused as the header's top-right badge
 *  — ONE version source (the projected ContinuonView), never a second literal. */
function versionOf(ws: WorkspaceView): string | undefined {
  const w = ws.left.find((widget) => widget.kind === 'continuon');
  return w ? (w.body as ContinuonView).version : undefined;
}

/** The participants `Listing` (id === ROSTER_LISTING_ID) among the rail's widgets — used
 *  only for the header's "active / total" count. NOT just the first listing (that may be
 *  the Rooms widget); the header counts PEOPLE, not rooms. */
function rosterOf(ws: WorkspaceView): ListingView | undefined {
  const w = ws.left.find(
    (widget) => widget.kind === 'listing' && (widget.body as ListingView).id === ROSTER_LISTING_ID,
  );
  return w ? (w.body as ListingView) : undefined;
}

export const webTarget: RenderTarget<TemplateResult> = {
  /** A `Listing`. The roster draws as rich member cards (the neutral cell now carries
   *  glyph/name/badges/status/meters); every other listing uses the generic cell. */
  listing(view: ListingView): TemplateResult {
    return renderListing(view);
  },

  /** The center, dispatched by room purpose through the web Content registry. */
  content(view: ContentView): TemplateResult {
    return webContentRegistry.render(view);
  },

  contextPanel(view: ContextPanelView): TemplateResult {
    return html`${view.listings.map((l) => this.listing(l))}`;
  },

  /** One left-rail widget, dispatched by kind through the web Widget registry. */
  widget(view: PanelWidget): TemplateResult {
    return webWidgetRegistry.render(view);
  },

  /** The three-panel who/what/where — reproduced from the WorkspaceView alone. The left
   *  rail is now a GLOBAL WIDGET STACK: each `PanelWidget` draws as a titled rail section
   *  (Metrics · Rooms · Users & Agents · …), dispatched by kind.
   *
   *  Shell geometry is the Discord reference: the rails run FULL HEIGHT; the tab
   *  strip, room header, transcript, and the host-supplied composer
   *  (`chrome.centerFooter`) are all scoped to the CENTER column — no chrome bar
   *  spans the whole window. */
  workspace(ws: WorkspaceView, chrome?: WorkspaceChrome<TemplateResult>): TemplateResult {
    // The focused room is the ACTIVE nav cell — with the live room set the
    // listing carries every room, so cells[0] is arbitrary order, not focus.
    const room = ws.nav.cells.find((c) => c.status === 'active') ?? ws.nav.cells[0];
    const roster = rosterOf(ws);
    const memberCount = roster?.cells.length ?? 0;
    const activeCount = roster?.cells.filter((c) => c.status === 'active').length ?? 0;
    const version = versionOf(ws);
    return html`
      <div class="panels" data-context=${ws.context.listings.length > 0 ? '' : nothing}>
        <aside class="who" aria-label="global widgets">
          ${ws.left.length > 0 ? ws.left.map((w) => this.widget(w)) : nothing}
        </aside>
        ${resizeHandle('who')}
        <section class="center" aria-label="focused activity">
          ${ws.nav.cells.length > 0
            ? html`<div class="tab-bar" role="tablist" aria-label="open activities">
                ${ws.nav.cells.map(navTab)}
              </div>`
            : nothing}
          <header class="room">
            <div class="room-name">${room?.title ?? ''}</div>
            <div class="room-meta">
              <span class="count" title="active / total">${activeCount}/${memberCount} here</span>
              <span class="live" title="live · ${room?.id ?? ''}"><span class="live-dot"></span>live</span>
              ${version
                ? html`<span class="continuon-version header-version" title="client build">${version}</span>`
                : nothing}
              <span class="header-controls">
                <button
                  class="hdr-btn hdr-live"
                  data-active=${ws.content.purpose === LIVE_PURPOSE ? '' : nothing}
                  @click=${(e: Event): void => {
                    fireLiveFaceToggle(e, ws.content.purpose !== LIVE_PURPOSE);
                  }}
                  title=${ws.content.purpose === LIVE_PURPOSE
                    ? 'in the live room — click to return to chat'
                    : "open this room's live face — the call grid"}
                >
                  📹 ${ws.content.purpose === LIVE_PURPOSE ? 'Live' : 'Go live'}
                </button>
                <button class="hdr-btn" @click=${cycleUniverse} title="cycle universe skin (?universe=)">
                  Theme
                </button>
                <button class="hdr-btn" disabled title="coming soon">Settings</button>
                <button class="hdr-btn" disabled title="coming soon">Browser</button>
                <button class="hdr-btn" disabled title="coming soon">Help</button>
              </span>
            </div>
          </header>
          <section class="what" aria-label="conversation">${this.content(ws.content)}</section>
          ${chrome?.centerFooter ?? nothing}
        </section>
        ${ws.context.listings.length > 0
          ? html`${resizeHandle('context')}<aside class="context" aria-label="activity context">
              ${ws.context.listings.map(
                (l) => html`<section class="rail-widget" data-widget="context">
                  <div class="who-head"><span class="who-title">${l.title}</span></div>
                  ${renderListing(l)}
                </section>`,
              )}
            </aside>`
          : nothing}
      </div>
    `;
  },
};
