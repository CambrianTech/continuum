/**
 * `SceneSurface` — the outlier-B integration proof (#187). Runs entirely headless (no GPU,
 * no Bevy, no browser), which is the point: it proves the ONE `Surface` trait bends to a
 * scene-graph/camera world the same way `DomSurface` bends to a DOM/browser world.
 *
 * what these catch:
 *  - render+diff (the SEE/JUDGE channels) work on a scene: an orbit visibly changes the
 *    frame, so `diff` reports a real nonzero ratio — the money signal.
 *  - probe (REASON) walks the REAL `SceneDescription` into the neutral `ProbeNode` tree
 *    with inherited world positions + projected boxes.
 *  - act rejects a foreign verb (a DOM `click`) with `ActError` — the observe/act boundary.
 */

import { describe, it, expect } from 'vitest';
import { SceneSurface, type SceneAction } from './sceneSurface';
import { ActError } from './surface';
import type { SceneDescription } from '../../protocol/typescript/scene/SceneDescription';
import type { SceneNode } from '../../protocol/typescript/scene/SceneNode';
import type { NodePayload } from '../../protocol/typescript/scene/NodePayload';

const IDENTITY_ROT = { x: 0, y: 0, z: 0, w: 1 };
const ONE = { x: 1, y: 1, z: 1 };

function node(id: string, payload: NodePayload, pos: { x: number; y: number; z: number }, scale = ONE): SceneNode {
  return {
    id,
    transform: { translation: pos, rotation: IDENTITY_ROT, scale },
    payload,
    physics: null,
    children: [],
  };
}

/** A minimal but real scene: a transform-only Group root over two Props and a Light, on a
 *  dark backdrop. Exercises drawables (prop/light), a non-drawable (group), and inheritance. */
function fixtureScene(): SceneDescription {
  const prop: NodePayload = { Prop: { asset: { source: 'crate.glb', kind: 'Mesh' } } };
  const light: NodePayload = { Light: { kind: 'Directional', color: { r: 1, g: 1, b: 1, a: 1 }, intensity: 1 } };
  const root: SceneNode = {
    id: 'stage',
    transform: { translation: { x: 0, y: 0, z: 0 }, rotation: IDENTITY_ROT, scale: ONE },
    payload: 'Group',
    physics: null,
    children: [
      node('crate-a', prop, { x: -1.2, y: 0, z: 0 }, { x: 1, y: 1, z: 1 }),
      node('crate-b', prop, { x: 1.2, y: 0, z: 0 }, { x: 1.4, y: 1.4, z: 1.4 }),
      node('key-light', light, { x: 0, y: 2, z: 1 }),
    ],
  };
  return { version: 1, backdrop: { r: 0.05, g: 0.06, b: 0.1, a: 1 }, root };
}

describe('SceneSurface (outlier B)', () => {
  it('renders, and an orbit changes the frame (SEE + JUDGE bend to a scene)', async () => {
    const surface = SceneSurface.open({ scene: fixtureScene(), viewport: { width: 320, height: 240 } });

    const before = await surface.render();
    expect(before.kind).toBe('image');
    if (before.kind !== 'image') throw new Error('expected image');
    expect(before.mime).toBe('image/png');
    expect(before.width).toBe(320);
    expect(before.height).toBe(240);
    expect(before.bytes.length).toBeGreaterThan(0);
    // PNG magic — a real encoded frame, not an empty buffer.
    expect(Array.from(before.bytes.slice(0, 4))).toEqual([0x89, 0x50, 0x4e, 0x47]);

    // Orbit the camera (the 3D hot-swap) and re-render — the picture must change.
    await surface.act({ kind: 'orbitCamera', azimuth: 0.8 });
    const after = await surface.render();

    const delta = surface.diff(before, after);
    expect(delta.totalPixels).toBe(320 * 240);
    expect(delta.ratio).toBeGreaterThan(0); // the money signal: the act did something visible

    await surface.close();
  });

  it('probes the real SceneDescription into a ProbeNode tree with inherited positions', async () => {
    const surface = SceneSurface.open({ scene: fixtureScene(), viewport: { width: 320, height: 240 } });
    const state = await surface.probe();

    expect(state.title).toBe('stage');
    expect(state.url).toBeUndefined(); // a scene has no URL — url is web-specific/optional
    expect(state.tree.tag).toBe('group');
    expect(state.tree.children).toHaveLength(3);

    const names = state.tree.children.map((c) => c.name);
    expect(names).toEqual(['crate-a', 'crate-b', 'key-light']);

    const crateB = state.tree.children.find((c) => c.name === 'crate-b');
    expect(crateB?.tag).toBe('prop');
    expect(crateB?.attrs?.position).toBe('1.20,0.00,0.00'); // inherited world position
    expect(crateB?.box?.width).toBeGreaterThan(0); // projected screen bounds present

    await surface.close();
  });

  it('rejects a foreign (DOM) action with ActError — the observe/act boundary', async () => {
    const surface = SceneSurface.open({ scene: fixtureScene() });
    // A DOM verb is not in SceneAction's vocabulary; force it to prove the boundary fails loud.
    await expect(surface.act({ kind: 'click', selector: 'button' } as unknown as SceneAction)).rejects.toBeInstanceOf(
      ActError,
    );
    await surface.close();
  });
});
