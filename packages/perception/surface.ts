/**
 * The `Surface` contract — universal eyes/ears/hands for what a persona creates or
 * observes. Spec: `docs/architecture/PERCEPTION-SURFACE.md` (#187). Perception is the
 * DUAL of production: a DOM page, a 3D scene, a video timeline, a live camera all
 * implement this ONE trait, exposing three channels + a diff. This file is the
 * consumer-neutral contract (no Playwright, no browser) so a future 3D/video Surface
 * implements the SAME shape — the trait `DomSurface` (outlier A) is extracted into.
 */

/** How to render — the viewpoint/time/region the Percept is taken from. */
export interface ViewSpec {
  /** Render at this viewport (CSS px). Omit to keep the surface's current size. */
  readonly viewport?: { readonly width: number; readonly height: number };
  /** Clip the render to this element (a CSS selector for the DOM). Omit = whole view. */
  readonly selector?: string;
  /** Force a colour scheme for the render (UI surfaces honour `prefers-color-scheme`). */
  readonly theme?: 'light' | 'dark';
  /** Capture the full scrollable page, not just the viewport. */
  readonly fullPage?: boolean;
}

/** WHAT THEY SEE/HEAR. A union so a consumer picks fidelity; today an image, later a
 *  filmstrip (motion/transitions) or audio. Bytes are the encoded artifact (PNG). */
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
 *  layout box. Kept minimal + surface-neutral: identity, geometry, text, a few attrs. */
export interface ProbeNode {
  /** Element tag / node type (`div`, `button`; a scene node's kind). */
  readonly tag: string;
  /** Accessibility role, when the surface exposes one (`button`, `heading`). */
  readonly role?: string;
  /** Accessible / display name — the human-meaningful label. */
  readonly name?: string;
  /** Visible text directly on this node (not descendants). */
  readonly text?: string;
  /** Layout box in surface coordinates — how a persona reasons about position/overflow. */
  readonly box?: { readonly x: number; readonly y: number; readonly width: number; readonly height: number };
  /** A curated set of load-bearing attributes (`id`, `class`, `data-*`, `href`). */
  readonly attrs?: Readonly<Record<string, string>>;
  readonly children: readonly ProbeNode[];
}

/** The full structural probe of a surface at an instant — pixels are for judging,
 *  THIS is for reasoning and for aiming actions (target a node, not a pixel). */
export interface StructuredState {
  readonly url: string;
  readonly title: string;
  /** The layout/DOM tree. */
  readonly tree: ProbeNode;
  /** The accessibility tree, when the surface exposes one (the semantic view). */
  readonly a11y?: ProbeNode;
}

/** WHAT THEY CAN DO — drive the surface, then re-perceive. `injectCss` is the
 *  hot-swap (retheme/relayout with NO redeploy) that makes iteration fast. */
export type Action =
  | { readonly kind: 'click'; readonly selector: string }
  | { readonly kind: 'type'; readonly selector: string; readonly text: string }
  | { readonly kind: 'press'; readonly key: string }
  | { readonly kind: 'hover'; readonly selector: string }
  | { readonly kind: 'injectCss'; readonly css: string }
  | { readonly kind: 'setViewport'; readonly width: number; readonly height: number };

/** THE MONEY SIGNAL — before/after. `ratio` (0..1) of pixels that changed; the training
 *  label for "did my change do what I intended". Structural delta grows this later. */
export interface Delta {
  readonly pixelsChanged: number;
  readonly totalPixels: number;
  readonly ratio: number;
}

/** The universal contract. A DOM page, a 3D scene, a video, a live camera implement it;
 *  a persona perceives/acts through it identically. Observe-only surfaces reject `act`. */
export interface Surface {
  /** Render a Percept from the given viewpoint (defaults to the whole current view). */
  render(view?: ViewSpec): Promise<Percept>;
  /** The structural + accessibility probe — what the persona reasons over. */
  probe(): Promise<StructuredState>;
  /** Drive the surface and settle. Rejects with `ActError` if unsupported/failed. */
  act(action: Action): Promise<void>;
  /** Before/after pixel delta. Pure — the caller supplies the two Percepts. */
  diff(before: Percept, after: Percept): Delta;
  /** Release the surface's resources (browser, renderer, stream). */
  close(): Promise<void>;
}

/** Thrown when an `act` is unsupported by this surface (e.g. a live-camera surface) or
 *  fails to apply — fail loud, never a silent no-op ([[fallbacks-are-illegal-fail-loud]]). */
export class ActError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ActError';
  }
}
