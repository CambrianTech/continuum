/**
 * `SceneSurface` — the `Surface` for a 3D scene (outlier B of #187). It is the
 * maximally-different second implementation the outlier-validation discipline
 * (`CLAUDE.md` § Methodical Process) demands: if the one `Surface` trait fits BOTH a
 * Playwright DOM page AND a scene-graph 3D world without forcing, mobile/animation/video
 * in the middle are trivial. Building it is what proved the trait's generic shape (see
 * `surface.ts` — SEE/JUDGE/REASON universal; only VIEW-hints + ACT-verbs are per-surface).
 *
 * ## It consumes the REAL scene invariant, not a parallel model
 *
 * The structure it reasons over is the backend-neutral `SceneDescription` (#107/#108) —
 * the SAME type the Bevy renderer instantiates. That is the compression rule: one scene
 * model, three producers (RON / builder / birther), and now a *second consumer* (this
 * perception probe) beside Bevy's instantiate seam. `probe()` walks that tree; `act()`
 * mutates it (orbit the camera, move a node); the description's own `From`/instantiate
 * conversions on the Rust side are untouched.
 *
 * ## render() is a reference projector; the production render is Bevy
 *
 * Exactly as `DomSurface` delegates its pixels to Playwright, a real 3D surface delegates
 * its pixels to Bevy's offscreen render-target readback (`live/video/bevy_renderer`). That
 * path is GPU-bound and avatar-slot-shaped — not a headless, deterministic, dependency-free
 * function. So the SEE channel here is a small deterministic **software projector** (orbit
 * camera → orthographic projection → painter's-order filled boxes → PNG): enough to prove
 * the trait's SEE/JUDGE channels bend to a scene (an orbit visibly changes the frame, so
 * `diff` reports a real ratio), swappable for a `BevySurface` that keeps this exact trait
 * and only changes how `render()` gets its bytes. No Bevy, no GPU, no browser — runnable
 * anywhere, which is what makes it glass-box verifiable today.
 */

import { PNG } from 'pngjs';
import { imageDiff } from './imageDiff';
import {
  ActError,
  type Delta,
  type Percept,
  type ProbeNode,
  type SetViewportAction,
  type StructuredState,
  type Surface,
  type ViewSpec,
} from './surface';
import type { SceneDescription } from '../../protocol/typescript/scene/SceneDescription';
import type { SceneNode } from '../../protocol/typescript/scene/SceneNode';
import type { NodePayload } from '../../protocol/typescript/scene/NodePayload';
import type { Vec3Desc } from '../../protocol/typescript/scene/Vec3Desc';

/** The scene surface's view-hints (outlier B): the neutral `ViewSpec` (framebuffer size)
 *  plus the 3D viewpoint — a camera the render orbits around. This is the same axis
 *  `DomViewSpec` fills with `selector`/`theme`; a 3D view needs a camera instead. */
export interface SceneViewSpec extends ViewSpec {
  /** Orbit camera pose for this render. Angles in RADIANS; omitted fields keep current. */
  readonly camera?: {
    readonly azimuth?: number;
    readonly elevation?: number;
    readonly distance?: number;
  };
}

/** The scene surface's act-verbs (outlier B): the universal base `Action` (`setViewport`)
 *  plus 3D drivers. `orbitCamera` is the viewpoint hot-swap (the scene analogue of the
 *  DOM's `injectCss`): re-frame the LIVE scene with no rebuild. */
export type SceneAction =
  | SetViewportAction
  | { readonly kind: 'orbitCamera'; readonly azimuth?: number; readonly elevation?: number; readonly distance?: number }
  | { readonly kind: 'moveNode'; readonly id: string; readonly dx?: number; readonly dy?: number; readonly dz?: number }
  | { readonly kind: 'setBackdrop'; readonly r: number; readonly g: number; readonly b: number };

interface Camera {
  azimuth: number;
  elevation: number;
  distance: number;
}

export interface SceneSurfaceOptions {
  readonly scene: SceneDescription;
  readonly viewport?: { readonly width: number; readonly height: number };
  readonly camera?: { readonly azimuth?: number; readonly elevation?: number; readonly distance?: number };
}

/** The kinds that get a drawn box (Group/Camera are transform-only/invisible; Environment
 *  is the global backdrop). */
const DRAWABLE = new Set(['prop', 'avatar', 'light']);

/** The payload discriminant as a lower-case tag (`"Group"` string variant, or the single
 *  key of the `{ Avatar: … }` object variant). */
function payloadKind(payload: NodePayload): string {
  if (typeof payload === 'string') return payload.toLowerCase();
  const key = Object.keys(payload)[0];
  return key ? key.toLowerCase() : 'node';
}

/** Deterministic djb2 hash → hue (0..1). No `Math.random`/`Date` — renders are reproducible. */
function hueOf(id: string): number {
  let h = 5381;
  for (let i = 0; i < id.length; i++) h = ((h << 5) + h + id.charCodeAt(i)) >>> 0;
  return (h % 360) / 360;
}

/** HSL→RGB (0..255). */
function hsl(hue: number, sat: number, light: number): [number, number, number] {
  const c = (1 - Math.abs(2 * light - 1)) * sat;
  const x = c * (1 - Math.abs(((hue * 6) % 2) - 1));
  const m = light - c / 2;
  const seg = Math.floor(hue * 6) % 6;
  const [r, g, b] = (
    [
      [c, x, 0],
      [x, c, 0],
      [0, c, x],
      [0, x, c],
      [x, 0, c],
      [c, 0, x],
    ] as const
  )[seg] ?? [c, x, 0];
  return [Math.round((r + m) * 255), Math.round((g + m) * 255), Math.round((b + m) * 255)];
}

export class SceneSurface implements Surface<SceneViewSpec, SceneAction> {
  private width: number;
  private height: number;

  private constructor(
    private scene: SceneDescription,
    private readonly camera: Camera,
    width: number,
    height: number,
  ) {
    this.width = width;
    this.height = height;
  }

  /** Take ownership of a mutable copy of the scene (the input stays immutable) and settle
   *  a camera. Ready to render/probe/act on return. */
  static open(opts: SceneSurfaceOptions): SceneSurface {
    const scene = structuredClone(opts.scene);
    const camera: Camera = {
      azimuth: opts.camera?.azimuth ?? 0.6,
      elevation: opts.camera?.elevation ?? 0.35,
      distance: opts.camera?.distance ?? 6,
    };
    return new SceneSurface(scene, camera, opts.viewport?.width ?? 640, opts.viewport?.height ?? 480);
  }

  /** Project a world point through the orbit camera to screen (x,y) + camera-space depth. */
  private project(p: Vec3Desc, cam: Camera): { sx: number; sy: number; depth: number } {
    const ca = Math.cos(cam.azimuth);
    const sa = Math.sin(cam.azimuth);
    // rotate about Y (azimuth)
    const x1 = p.x * ca + p.z * sa;
    const z1 = -p.x * sa + p.z * ca;
    const y1 = p.y;
    // rotate about X (elevation)
    const ce = Math.cos(cam.elevation);
    const se = Math.sin(cam.elevation);
    const y2 = y1 * ce - z1 * se;
    const z2 = y1 * se + z1 * ce;
    const zoom = (Math.min(this.width, this.height) / cam.distance) * 0.9;
    return { sx: this.width / 2 + x1 * zoom, sy: this.height / 2 - y2 * zoom, depth: z2 };
  }

  /** Flatten the scene graph to world-positioned drawables (translation inherited down the
   *  tree — the classic scene-graph transform inheritance this proves the probe reasons over). */
  private flatten(): { node: SceneNode; world: Vec3Desc; kind: string }[] {
    const out: { node: SceneNode; world: Vec3Desc; kind: string }[] = [];
    const walk = (node: SceneNode, parent: Vec3Desc): void => {
      const t = node.transform.translation;
      const world: Vec3Desc = { x: parent.x + t.x, y: parent.y + t.y, z: parent.z + t.z };
      out.push({ node, world, kind: payloadKind(node.payload) });
      for (const child of node.children) walk(child, world);
    };
    walk(this.scene.root, { x: 0, y: 0, z: 0 });
    return out;
  }

  /** Screen box for a node from its world position + scale (used by both render and probe,
   *  so the probe's `box` is exactly where the pixels are). */
  private screenBox(world: Vec3Desc, scale: Vec3Desc, cam: Camera): { x: number; y: number; width: number; height: number; depth: number } {
    const { sx, sy, depth } = this.project(world, cam);
    const zoom = (Math.min(this.width, this.height) / cam.distance) * 0.9;
    const half = Math.max(2, scale.x * zoom * 0.5);
    return { x: Math.round(sx - half), y: Math.round(sy - half), width: Math.round(half * 2), height: Math.round(half * 2), depth };
  }

  render(view: SceneViewSpec = {}): Promise<Percept> {
    if (view.viewport) {
      this.width = view.viewport.width;
      this.height = view.viewport.height;
    }
    const cam: Camera = {
      azimuth: view.camera?.azimuth ?? this.camera.azimuth,
      elevation: view.camera?.elevation ?? this.camera.elevation,
      distance: view.camera?.distance ?? this.camera.distance,
    };
    const { width, height } = this;
    const buf = Buffer.alloc(width * height * 4);
    // Backdrop (SceneDescription.backdrop — linear RGBA 0..1 → 0..255).
    const bd = this.scene.backdrop;
    const br = Math.round(bd.r * 255);
    const bg = Math.round(bd.g * 255);
    const bb = Math.round(bd.b * 255);
    for (let i = 0; i < width * height; i++) {
      buf[i * 4] = br;
      buf[i * 4 + 1] = bg;
      buf[i * 4 + 2] = bb;
      buf[i * 4 + 3] = 255;
    }
    // Drawables, painter's order (far → near).
    const boxes = this.flatten()
      .filter((d) => DRAWABLE.has(d.kind))
      .map((d) => {
        const box = this.screenBox(d.world, d.node.transform.scale, cam);
        const light = d.kind === 'light' ? 0.75 : d.kind === 'avatar' ? 0.6 : 0.5;
        const sat = d.kind === 'light' ? 0.9 : 0.65;
        return { box, color: hsl(hueOf(d.node.id), sat, light) };
      })
      .sort((a, b) => b.box.depth - a.box.depth);
    for (const { box, color } of boxes) {
      const x0 = Math.max(0, box.x);
      const y0 = Math.max(0, box.y);
      const x1 = Math.min(width, box.x + box.width);
      const y1 = Math.min(height, box.y + box.height);
      for (let y = y0; y < y1; y++) {
        for (let x = x0; x < x1; x++) {
          const i = (y * width + x) * 4;
          buf[i] = color[0];
          buf[i + 1] = color[1];
          buf[i + 2] = color[2];
          buf[i + 3] = 255;
        }
      }
    }
    const png = new PNG({ width, height });
    buf.copy(png.data);
    const bytes = new Uint8Array(PNG.sync.write(png));
    return Promise.resolve({ kind: 'image', mime: 'image/png', bytes, width, height });
  }

  probe(): Promise<StructuredState> {
    const cam = this.camera;
    const buildNode = (node: SceneNode, parent: Vec3Desc): ProbeNode => {
      const t = node.transform.translation;
      const world: Vec3Desc = { x: parent.x + t.x, y: parent.y + t.y, z: parent.z + t.z };
      const s = node.transform.scale;
      const box = this.screenBox(world, s, cam);
      const kind = payloadKind(node.payload);
      return {
        tag: kind,
        name: node.id,
        box: { x: box.x, y: box.y, width: box.width, height: box.height },
        attrs: {
          kind,
          position: `${world.x.toFixed(2)},${world.y.toFixed(2)},${world.z.toFixed(2)}`,
          scale: `${s.x.toFixed(2)},${s.y.toFixed(2)},${s.z.toFixed(2)}`,
        },
        children: node.children.map((c) => buildNode(c, world)),
      };
    };
    const tree = buildNode(this.scene.root, { x: 0, y: 0, z: 0 });
    return Promise.resolve({ title: this.scene.root.id, tree });
  }

  act(action: SceneAction): Promise<void> {
    switch (action.kind) {
      case 'setViewport':
        this.width = action.width;
        this.height = action.height;
        return Promise.resolve();
      case 'orbitCamera':
        // Viewpoint hot-swap — re-frame the live scene, no rebuild (the 3D `injectCss`).
        this.camera.azimuth += action.azimuth ?? 0;
        this.camera.elevation += action.elevation ?? 0;
        this.camera.distance = Math.max(0.5, this.camera.distance + (action.distance ?? 0));
        return Promise.resolve();
      case 'moveNode': {
        const node = this.findNode(this.scene.root, action.id);
        if (!node) return Promise.reject(new ActError(`moveNode: no node with id "${action.id}" in the scene`));
        const t = node.transform.translation;
        node.transform = {
          ...node.transform,
          translation: { x: t.x + (action.dx ?? 0), y: t.y + (action.dy ?? 0), z: t.z + (action.dz ?? 0) },
        };
        return Promise.resolve();
      }
      case 'setBackdrop':
        this.scene = { ...this.scene, backdrop: { r: action.r, g: action.g, b: action.b, a: 1 } };
        return Promise.resolve();
      default: {
        // A verb from another surface's vocabulary (e.g. a DOM `click`) — fail loud (reject,
        // per the trait contract: `act` REJECTS with ActError, never throws synchronously).
        const foreign = action as { kind: string };
        return Promise.reject(new ActError(`SceneSurface does not support action "${foreign.kind}"`));
      }
    }
  }

  private findNode(node: SceneNode, id: string): SceneNode | undefined {
    if (node.id === id) return node;
    for (const child of node.children) {
      const hit = this.findNode(child, id);
      if (hit) return hit;
    }
    return undefined;
  }

  // JUDGE is universal — the SAME pixel diff DomSurface uses (see imageDiff.ts).
  diff(before: Percept, after: Percept): Delta {
    return imageDiff(before, after);
  }

  close(): Promise<void> {
    return Promise.resolve();
  }
}
