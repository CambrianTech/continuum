/**
 * Foundry → pattern-primitive projection tests.
 *
 * Proves the foundry activity expresses itself on the consumer-neutral primitives
 * (ACTIVITY-ROOM-PATTERNS.md outlier B): the model catalogue is a `Listing`, that
 * Listing is the foundry `ContextPanel`, and `Content` is keyed by the room's
 * `purpose`. Projections take `ForgeState`, so these fixtures need no live wiring.
 */

import { describe, it, expect } from 'vitest';
import type { ForgeState } from './ForgeState';
import { modelsListing, foundryContextPanel, foundryContent } from './patternProjections';

const state: ForgeState = {
  kind: 'foundry',
  revision: 1,
  room_id: 'room-f',
  room_name: 'foundry',
  purpose: 'foundry',
  models: [
    { model_id: 'Qwen/Qwen3-4B', display_name: 'Qwen3 4B', source: 'huggingface', params_b: 4 },
    { model_id: 'local/asha', display_name: 'Asha', source: 'local' }, // params_b absent = unknown
  ],
};

describe('foundry → pattern projections', () => {
  // what this catches: the catalogue projects to the model `Listing` — id/name carry
  // through, the subtitle labels params+source when known, and an unknown param count
  // (absent) yields just the source, never a fabricated "0B".
  it('projects the catalogue into the model Listing', () => {
    const l = modelsListing(state);
    expect(l.id).toBe('models');
    expect(l.title).toBe('Models');
    expect(l.cells).toHaveLength(2);
    const [qwen, asha] = l.cells;
    expect(qwen).toMatchObject({
      id: 'Qwen/Qwen3-4B',
      title: 'Qwen3 4B',
      subtitle: '4B · huggingface',
      badges: ['huggingface'],
    });
    expect(asha?.subtitle).toBe('local');
  });

  // what this catches: the model Listing is the foundry ContextPanel (the right-hand
  // list), not bespoke — the same primitive chat uses for its context.
  it('puts the model Listing in the ContextPanel', () => {
    const ctx = foundryContextPanel(state);
    expect(ctx.listings).toHaveLength(1);
    expect(ctx.listings[0]?.id).toBe('models');
  });

  // what this catches: Content is keyed by the room's purpose ("foundry"), so a
  // target's Content registry dispatches on it — the same dispatch chat uses, a
  // different body. Foundry renders on the SAME shell as chat, only Content/Context
  // differ.
  it('keys Content by the room purpose', () => {
    const content = foundryContent(state);
    expect(content.purpose).toBe('foundry');
    // `foundryContent` returns `ContentView<ForgeContentBody>`, so `body` is already
    // typed — no cast needed (unlike the chat spec, where the body is opaque at the
    // `WorkspaceView` boundary).
    expect(content.body.models).toHaveLength(2);
  });
});
