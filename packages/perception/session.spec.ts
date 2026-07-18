/**
 * `PerceptionSession` — the persona loop, proven headless over a scene (#187).
 *
 * what these catch:
 *  - observe() combines SEE + REASON (a Percept AND the structure) in one call.
 *  - interact() drives the surface and returns the before/after Delta with NO caller
 *    bookkeeping — the money signal falls out of the loop (a real orbit → nonzero ratio).
 *  - the before-frame of interact() is the last observed frame (auto-diff), so an
 *    observe → interact sequence measures exactly the interaction's effect.
 */

import { describe, it, expect } from 'vitest';
import { PerceptionSession } from './session';
import type { SceneDescription } from '../../protocol/typescript/scene/SceneDescription';
import type { NodePayload } from '../../protocol/typescript/scene/NodePayload';

const R = { x: 0, y: 0, z: 0, w: 1 };
const s = (n: number) => ({ x: n, y: n, z: n });

function scene(): SceneDescription {
  const prop: NodePayload = { Prop: { asset: { source: 'crate.glb', kind: 'Mesh' } } };
  return {
    version: 1,
    backdrop: { r: 0.05, g: 0.06, b: 0.1, a: 1 },
    root: {
      id: 'stage',
      transform: { translation: { x: 0, y: 0, z: 0 }, rotation: R, scale: s(1) },
      payload: 'Group',
      physics: null,
      children: [
        { id: 'box-a', transform: { translation: { x: -1, y: 0, z: 0 }, rotation: R, scale: s(1) }, payload: prop, physics: null, children: [] },
        { id: 'box-b', transform: { translation: { x: 1, y: 0, z: 0 }, rotation: R, scale: s(1.3) }, payload: prop, physics: null, children: [] },
      ],
    },
  };
}

describe('PerceptionSession (the persona loop)', () => {
  it('observe() returns pixels AND structure', async () => {
    const session = PerceptionSession.openScene({ scene: scene(), viewport: { width: 240, height: 180 } });
    const obs = await session.observe();

    expect(obs.percept.kind).toBe('image');
    if (obs.percept.kind !== 'image') throw new Error('expected image');
    expect(obs.percept.width).toBe(240);
    expect(obs.structure.title).toBe('stage');
    expect(obs.structure.tree.children.map((c) => c.name)).toEqual(['box-a', 'box-b']);

    await session.close();
  });

  it('interact() drives the surface and returns the before/after Delta (money signal, no bookkeeping)', async () => {
    const session = PerceptionSession.openScene({ scene: scene(), viewport: { width: 240, height: 180 } });
    await session.observe(); // establish the "before" frame

    const { observation, delta } = await session.interact([{ kind: 'orbitCamera', azimuth: 0.9 }]);

    expect(delta.totalPixels).toBe(240 * 180);
    expect(delta.ratio).toBeGreaterThan(0); // the orbit changed the frame
    expect(observation.structure.tree.children).toHaveLength(2); // fresh structure came back too

    await session.close();
  });

  it('interact() works even with no prior observe() — it renders a before-frame itself', async () => {
    const session = PerceptionSession.openScene({ scene: scene() });
    // No observe() first; interact must still produce a valid delta (renders its own before).
    const { delta } = await session.interact([{ kind: 'moveNode', id: 'box-a', dx: 2, dy: 1 }]);
    expect(delta.ratio).toBeGreaterThan(0);
    await session.close();
  });
});
