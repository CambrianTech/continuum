/**
 * `facetCells` — the All/Rooms/DMs lens over the neutral `ListingCell.group`
 * (POSITRON-WIDGET-SOPHISTICATION.md: "filter = a facet over groups"). Pure and
 * DOM-free, so the routing decision is pinned without a browser.
 */

import { describe, it, expect } from 'vitest';
import type { ListingCell } from '@continuum/patterns';
import { facetCells } from './RoomsPanel';

const cells: readonly ListingCell[] = [
  { id: 'a', title: 'general', group: 'room' },
  { id: 'b', title: 'joel-dm', group: 'dm' },
  { id: 'c', title: 'focused', group: 'chat' }, // pre-nav single-room cell (group = purpose)
];

describe('facetCells', () => {
  // what this catches: the facet is a pure lens — All passes everything through,
  // Rooms excludes only explicit DMs (a purpose-grouped cell is room-shaped),
  // and DMs shows ONLY explicit `dm` groups — never a title-based guess.
  it('facets over the group key without guessing', () => {
    expect(facetCells(cells, 'all')).toHaveLength(3);
    expect(facetCells(cells, 'rooms').map((c) => c.id)).toEqual(['a', 'c']);
    expect(facetCells(cells, 'dms').map((c) => c.id)).toEqual(['b']);
  });
});
