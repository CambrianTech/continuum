/**
 * The ONE canonical test scene (compression rule: one place builds "a valid scene the
 * specs assert against"). Shared by `sceneSurface.spec.ts` and `session.spec.ts` so both
 * exercise the same `SceneDescription` shape instead of hand-rolling near-duplicate fixtures.
 *
 * Test-only: not re-exported from `index.ts`.
 */

import type { SceneDescription } from '../../protocol/typescript/scene/SceneDescription';
import type { SceneNode } from '../../protocol/typescript/scene/SceneNode';
import type { NodePayload } from '../../protocol/typescript/scene/NodePayload';

const IDENTITY_ROT = { x: 0, y: 0, z: 0, w: 1 };
const ONE = { x: 1, y: 1, z: 1 };

function node(
  id: string,
  payload: NodePayload,
  pos: { x: number; y: number; z: number },
  scale: { x: number; y: number; z: number } = ONE,
): SceneNode {
  return {
    id,
    transform: { translation: pos, rotation: IDENTITY_ROT, scale },
    payload,
    physics: null,
    children: [],
  };
}

/**
 * A transform-only Group root ("stage") over two Props and a Light, on a dark backdrop.
 * Exercises drawables (prop/light), a non-drawable (group), and world-position inheritance.
 * `box-b` sits at world x=1.2 (→ probe `position` "1.20,0.00,0.00") at 1.4× scale.
 */
export function testScene(): SceneDescription {
  const prop: NodePayload = { Prop: { asset: { source: 'crate.glb', kind: 'Mesh' } } };
  const light: NodePayload = { Light: { kind: 'Directional', color: { r: 1, g: 1, b: 1, a: 1 }, intensity: 1 } };
  const root: SceneNode = {
    id: 'stage',
    transform: { translation: { x: 0, y: 0, z: 0 }, rotation: IDENTITY_ROT, scale: ONE },
    payload: 'Group',
    physics: null,
    children: [
      node('box-a', prop, { x: -1.2, y: 0, z: 0 }, { x: 1, y: 1, z: 1 }),
      node('box-b', prop, { x: 1.2, y: 0, z: 0 }, { x: 1.4, y: 1.4, z: 1.4 }),
      node('key-light', light, { x: 0, y: 2, z: 1 }),
    ],
  };
  return { version: 1, backdrop: { r: 0.05, g: 0.06, b: 0.1, a: 1 }, root };
}
