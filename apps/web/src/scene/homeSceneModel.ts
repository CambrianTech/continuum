/**
 * The HOME SCENE MODEL — the engine-neutral, one-to-one contract for a
 * citizen's 3D home (CITIZEN-HOMES-ORTHOGRAPHIC.md).
 *
 * THIS is the "same scene" guarantee Joel set (2026-08-31: "build up the
 * scene for both livekit and this — one to one same 3d scene"): the model
 * carries semantic objects + their spatial facts; every renderer — the
 * three.js web element today, the native bevy target later, a VR camera on
 * either — builds its engine objects FROM this model and nothing else. The
 * LiveKit dock is a named surface in the model (`officeScreen`), so a call
 * renders as a video texture on the SAME mesh in every engine.
 *
 * CONVERGENCE NOTE (legacy audit, 2026-08-31): the core ALREADY defines the
 * canonical engine-neutral contract — `SceneDescription`
 * (core/…/bevy_renderer/scene/description.rs, ts-rs exported): scene-graph
 * tree, AssetRef/AssetKind (mesh · VRM · splat), RON/builder/birther
 * producers. THIS model is the interim v1 shim; v2 of `<home-scene>`
 * instantiates SceneDescription directly and this file dissolves into a
 * facts→SceneDescription builder ([[compression]] — no parallel scene
 * format survives).
 *
 * Facts, never fabrication: each element derives from live state (the
 * mapping table in the architecture doc). Region geometry is defaulted here
 * until home recipes carry authored layouts — the model's SHAPE is the
 * durable contract, the numbers are v1 furniture.
 */

/** One rectangular region of the home, in floor-plane meters. */
export interface HomeRegion {
  readonly key: 'office' | 'shelf' | 'garden' | 'floor';
  readonly x: number;
  readonly z: number;
  readonly w: number;
  readonly d: number;
}

/** The semantic scene — everything a renderer needs, nothing engine-shaped. */
export interface HomeSceneModel {
  /** Room shell footprint (meters). */
  readonly width: number;
  readonly depth: number;
  readonly wallHeight: number;
  /** She is home: lights the window, spawns her avatar. */
  readonly online: boolean;
  /** Live runs → glowing items on the desk (capped by the renderer). */
  readonly activeRuns: number;
  /** Resolved verdicts → trophies on the shelf. */
  readonly trophies: number;
  /** Paged-in genes → the plant's growth. */
  readonly genes: number;
  /** Speaking right now → the avatar faces the camera and pulses. */
  readonly speaking: boolean;
  /** Display name (floor plaque / nameplate). */
  readonly name: string;
  /** Her bevy-rendered VRM portrait (`/avatars/<identity>.png`, the
   *  avatar/snapshot pipe) — the REAL her, billboarded in the room. */
  readonly avatarUrl?: string;
  /** Default region layout (recipe-authored later). */
  readonly regions: readonly HomeRegion[];
}

/** Default v1 furniture layout — replaced by recipe geometry when it lands. */
export function defaultRegions(): readonly HomeRegion[] {
  return [
    { key: 'floor', x: 0, z: 0, w: 8, d: 6 },
    { key: 'office', x: 2.2, z: -1.2, w: 2.6, d: 1.4 },
    { key: 'shelf', x: -0.5, z: -2.6, w: 3.4, d: 0.5 },
    { key: 'garden', x: -3.1, z: 1.8, w: 1.2, d: 1.2 },
  ];
}

/** Build the model from the profile's live facts. */
export function homeSceneModel(facts: {
  readonly name: string;
  readonly online: boolean;
  readonly activeRuns: number;
  readonly trophies: number;
  readonly genes: number;
  readonly speaking?: boolean;
  readonly avatarUrl?: string;
}): HomeSceneModel {
  return {
    width: 8,
    depth: 6,
    wallHeight: 2.6,
    online: facts.online,
    activeRuns: facts.activeRuns,
    trophies: facts.trophies,
    genes: facts.genes,
    speaking: facts.speaking ?? false,
    name: facts.name,
    ...(facts.avatarUrl !== undefined ? { avatarUrl: facts.avatarUrl } : {}),
    regions: defaultRegions(),
  };
}
