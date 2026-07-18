/**
 * The `Surface` contract — universal eyes/ears/hands for what a persona creates or
 * observes. Spec: `docs/architecture/PERCEPTION-SURFACE.md` (#187). Perception is the
 * DUAL of production: a DOM page, a 3D scene, a video timeline, a live camera all
 * implement this ONE trait, exposing three channels + a diff.
 *
 * ## What the outlier-B exercise proved (DOM outlier A vs Scene outlier B)
 *
 * Building the maximally-different second surface (`SceneSurface`, a Bevy-shaped 3D
 * scene) against the same trait `DomSurface` (Playwright/web) was extracted from showed
 * exactly which parts of perception are universal and which are surface-flavored:
 *
 *   - **SEE (`Percept`) and JUDGE (`Delta`) are universal** — both surfaces produce the
 *     identical `image/png` Percept and consume the identical pixel `Delta` (one shared
 *     `imageDiff`). A rendered frame is a rendered frame; a pixel diff is a pixel diff.
 *   - **REASON (`StructuredState` / `ProbeNode`) is universal** — a DOM tree and a scene
 *     graph are both "a tree of named, boxed nodes". Only `url` is web-specific (optional).
 *   - **VIEW-hints (`ViewSpec`) and ACT-verbs (`Action`) are surface-specific** — a DOM
 *     view has `selector/theme`; a 3D view has a `camera`. A DOM acts by `click/injectCss`;
 *     a scene acts by `orbitCamera/moveNode`. These are the ONLY axes that differ.
 *
 * So the trait is **generic over exactly those two axes** — `Surface<V, A>` — and every
 * surface owns its own `ViewSpec`/`Action` union (no central god-enum every new surface
 * must edit). `setViewport` is the one genuinely-universal actuator (every surface has a
 * canvas size), so it lives on the base `Action`.
 */

/** How to render — the viewpoint/region the Percept is taken from. The neutral BASE:
 *  every surface can be sized. A DOM surface extends this with selector/theme; a 3D
 *  surface extends it with a camera (see `DomViewSpec`, `SceneViewSpec`). */
export interface ViewSpec {
  /** Render at this size (CSS px for a UI, framebuffer px for a scene). Omit = current. */
  readonly viewport?: { readonly width: number; readonly height: number };
}

/** WHAT THEY SEE/HEAR. A union so a consumer picks fidelity; today an image, later a
 *  filmstrip (motion/transitions) or audio. Bytes are the encoded artifact (PNG).
 *  UNIVERSAL across every surface — a rendered frame is a rendered frame. */
export type Percept =
  | {
      readonly kind: 'image';
      readonly mime: 'image/png';
      readonly bytes: Uint8Array;
      readonly width: number;
      readonly height: number;
    }
  | { readonly kind: 'filmstrip'; readonly frames: readonly Percept[] };

/** One node of THE STRUCTURE THEY REASON OVER — a DOM element, a scene-graph node, a
 *  layout box. Kept minimal + surface-neutral: identity, geometry, text, a few attrs.
 *  `box` is the node's projected 2D bounds in the rendered frame — valid for ANY surface
 *  (a DOM layout box, a scene node's projected screen rect), which is what lets a persona
 *  aim an action at a node instead of a pixel. */
export interface ProbeNode {
  /** Element tag / node type (`div`, `button`; a scene node's payload kind). */
  readonly tag: string;
  /** Accessibility role, when the surface exposes one (`button`, `heading`). */
  readonly role?: string;
  /** Accessible / display name — the human-meaningful label (a scene node's id). */
  readonly name?: string;
  /** Visible text directly on this node (not descendants). */
  readonly text?: string;
  /** Projected 2D bounds in the rendered frame — how a persona reasons about position. */
  readonly box?: { readonly x: number; readonly y: number; readonly width: number; readonly height: number };
  /** A curated set of load-bearing attributes (`id`, `class`, `data-*`, `href`; for a
   *  scene node: `position`, `scale`, `kind`). */
  readonly attrs?: Readonly<Record<string, string>>;
  readonly children: readonly ProbeNode[];
}

/** The full structural probe of a surface at an instant — pixels are for judging,
 *  THIS is for reasoning and for aiming actions (target a node, not a pixel).
 *  UNIVERSAL: a DOM tree and a scene graph are both "a tree of named, boxed nodes". */
export interface StructuredState {
  /** The surface's location identity, when it has one (a page URL). Absent for a scene,
   *  a live camera, an in-memory surface — a scene has no URL. */
  readonly url?: string;
  /** The surface's human label (a page `<title>`, a scene's root name). */
  readonly title: string;
  /** The layout/DOM tree, or the scene graph — the geometry view. */
  readonly tree: ProbeNode;
  /** The accessibility tree, when the surface exposes one (the semantic view). */
  readonly a11y?: ProbeNode;
}

/** The neutral BASE actuator marker — every act-verb is a discriminated `{ kind }`. A
 *  surface's concrete `Action` union (`DomAction`, `SceneAction`) is a SUPERSET of this,
 *  so the trait's `A extends Action` says "A is some actuator vocabulary", never forcing a
 *  central god-union every new surface must edit. */
export interface Action {
  readonly kind: string;
}

/** Resize the surface's canvas — the ONE universal concrete actuator (every surface, DOM
 *  or 3D or video, has a framebuffer size). Every surface's `Action` union includes it. */
export interface SetViewportAction extends Action {
  readonly kind: 'setViewport';
  readonly width: number;
  readonly height: number;
}

/** THE MONEY SIGNAL — before/after. `ratio` (0..1) of pixels that changed; the training
 *  label for "did my change do what I intended". UNIVERSAL — one `imageDiff` serves every
 *  surface. Structural delta grows this later. */
export interface Delta {
  readonly pixelsChanged: number;
  readonly totalPixels: number;
  readonly ratio: number;
}

/** The universal contract, generic over the two surface-specific axes the outlier-B
 *  exercise isolated: `V` = this surface's view-hints, `A` = this surface's act-verbs.
 *  A DOM page, a 3D scene, a video, a live camera implement it; a persona perceives via
 *  the universal `render`/`probe`/`diff` identically, and acts through the surface's own
 *  `A`. Observe-only surfaces (a live camera) reject `act` with `ActError`. */
export interface Surface<V extends ViewSpec = ViewSpec, A extends Action = Action> {
  /** Render a Percept from the given viewpoint (defaults to the whole current view). */
  render(view?: V): Promise<Percept>;
  /** The structural (+ accessibility) probe — what the persona reasons over. */
  probe(): Promise<StructuredState>;
  /** Drive the surface and settle. Rejects with `ActError` if unsupported/failed. */
  act(action: A): Promise<void>;
  /** Before/after pixel delta. Pure — the caller supplies the two Percepts. */
  diff(before: Percept, after: Percept): Delta;
  /** Release the surface's resources (browser, renderer, stream). */
  close(): Promise<void>;
}

/** Thrown when an `act` is unsupported by this surface (e.g. a live-camera surface, or a
 *  verb from a different surface's vocabulary) or fails to apply — fail loud, never a
 *  silent no-op ([[fallbacks-are-illegal-fail-loud]]). */
export class ActError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ActError';
  }
}
