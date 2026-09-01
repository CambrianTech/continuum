/**
 * `<rooms-panel>` — the dense Rooms rail section (the old sidebar's Rooms
 * widget): header with live count, an All/Rooms/DMs filter row, the room cells
 * (name + purpose description + unread pill via the generic `listingCell`), and
 * the "+ Start a conversation" affordance.
 *
 * WHY an element and not a pure fragment: the filter facet is view-local UI
 * state (which lens the reader is looking through), exactly like `<chat-widget>`'s
 * expand set — it belongs to the renderer, never the projection. The facet runs
 * over the neutral `ListingCell.group` (the nav tab's target kind), per
 * POSITRON-WIDGET-SOPHISTICATION.md: "filter = a facet over groups", no new vocab.
 *
 * Light DOM on purpose: the element renders inside `<chat-widget>`'s shadow tree
 * and reuses its `.cell`/`.who-head` styling + the composed `LISTING_SELECT`
 * event path — one stylesheet, one select seam, no duplicate skin.
 *
 * "+ Start a conversation" is rendered DISABLED with an honest "coming soon"
 * title — the room-create verb isn't wired yet, and a dead-looking-live button
 * is a fake action ([[commands-do-real-work-and-return-receipts-not-promise-slop]]).
 */

import { LitElement, html, nothing, type TemplateResult } from 'lit';
import type { ListingCell, ListingView } from '@continuum/patterns';
import { listingCell } from './parts';

/** The three lenses. `dms` has no marker in the data yet — it filters to the
 *  honest empty state until DM rooms carry one. */
export type RoomsFacet = 'all' | 'rooms' | 'dms';

/** Pure facet rule over the neutral `group` key — unit-testable without a DOM.
 *  `rooms` = room-shaped groups ONLY (a nav tab's `room` kind, or a
 *  purpose-grouped focused-room cell) — open persona/content tabs are real
 *  activities but reached via their own controls (roster tiles, the tab
 *  strip), so the rail's default view keeps them out of the room list;
 *  `dms` = cells explicitly grouped `dm` (none yet — honest empty, never a
 *  guess from titles). */
export function facetCells(
  cells: readonly ListingCell[],
  facet: RoomsFacet,
): readonly ListingCell[] {
  switch (facet) {
    case 'all':
      return cells;
    case 'rooms':
      return cells.filter(
        (c) => c.group !== 'dm' && c.group !== 'persona' && c.group !== 'content',
      );
    case 'dms':
      return cells.filter((c) => c.group === 'dm');
  }
}

const FACETS: readonly { id: RoomsFacet; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'rooms', label: 'Rooms' },
  { id: 'dms', label: 'DMs' },
];

export class RoomsPanel extends LitElement {
  static override properties = {
    view: { attribute: false },
    heading: { attribute: false },
    _facet: { state: true },
  };

  /** The already-projected rooms `ListingView` (cells carry title/subtitle/
   *  count/group). Pushed by the widget renderer on every workspace paint. */
  view?: ListingView;

  /** Section heading (the PanelWidget title, e.g. "Rooms"). */
  heading = 'Rooms';

  /** Default facet is ROOMS (Joel 2026-07-30): persona/content activities are
   *  reachable via their own controls — the rail shows rooms unless the reader
   *  explicitly widens to All. */
  private _facet: RoomsFacet = 'rooms';

  /** Render into the light DOM — inherit `<chat-widget>`'s shadow stylesheet
   *  and let `LISTING_SELECT` bubble through the one existing seam. */
  protected override createRenderRoot(): HTMLElement {
    return this;
  }

  override render(): TemplateResult {
    const view = this.view;
    if (!view) return html``;
    // Tree order (#2632 slice b): roots keep their order; each root's child
    // activities follow it directly. A child whose parent isn't visible in
    // this facet renders as a root — never hidden work.
    const faceted = facetCells(view.cells, this._facet);
    // AN OUTSIDER'S LIST (Joel, 2026-08-31: "I don't really even comprehend
    // what those rooms are"): the top level is DURABLE places only. Work
    // rooms (anything with a parent_ref) either nest under their visible
    // parent or fold into ONE collapsed "work rooms" group — never
    // interleaved as pseudo-roots wearing branch ticks for rooms they don't
    // belong to. Deduped by id: the same room renders once, ever.
    const seen = new Set<string>();
    const dedup = faceted.filter((c) => (seen.has(c.id) ? false : (seen.add(c.id), true)));
    const ids = new Set(dedup.map((c) => c.id));
    const places = dedup.filter((c) => c.parent === undefined);
    const childrenOf = (id: string): readonly ListingCell[] =>
      dedup.filter((c) => c.parent === id);
    const orphans = dedup.filter((c) => c.parent !== undefined && !ids.has(c.parent));
    const cells = places.flatMap((r) => [r, ...childrenOf(r.id)]);
    return html`
      <section class="rail-widget" data-widget="rooms" data-id=${view.id}>
        <div class="who-head">
          <span class="who-title">${this.heading}</span>
          <span class="rooms-facets" role="tablist" aria-label="room filter">
            ${FACETS.map(
              (f) => html`<button
                class="rooms-facet"
                role="tab"
                aria-selected=${this._facet === f.id ? 'true' : 'false'}
                ?data-active=${this._facet === f.id}
                @click=${(): void => {
                  this._facet = f.id;
                }}
              >
                ${f.label}
              </button>`,
            )}
          </span>
          <span class="who-count">${view.cells.length}</span>
        </div>
        ${cells.length > 0 || orphans.length > 0
          ? html`<ul class="cells rooms-cells">
              ${cells.map((c) => listingCell(c, view.id))}
              ${orphans.length > 0
                ? html`<details class="rooms-work">
                    <summary>work rooms · ${orphans.length}</summary>
                    <ul class="cells">
                      ${orphans.map((c) => listingCell(c, view.id))}
                    </ul>
                  </details>`
                : nothing}
            </ul>`
          : html`<div class="rooms-empty">
              ${this._facet === 'dms' ? 'No direct messages yet' : 'No rooms yet'}
            </div>`}
        <button class="rooms-start" disabled title="coming soon — room creation isn't wired yet">
          + Start a conversation
        </button>
      </section>
    `;
  }
}

customElements.define('rooms-panel', RoomsPanel);

declare global {
  interface HTMLElementTagNameMap {
    'rooms-panel': RoomsPanel;
  }
}
