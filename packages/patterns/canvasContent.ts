/**
 * The CANVAS region's neutral `Content` body — the design-bench run room's
 * live artifact render (DESIGN-BENCH-VISUAL-CRAFT.md §5: "slot content whose
 * payload is the persona's RENDERED page, live, re-observed on her writes —
 * the walk-in sees the design evolve"). The same seam the 3D universe
 * payloads ride ([[universes-are-positron-asset-payloads]]): a region whose
 * payload is a live artifact render, never a bespoke viewer.
 *
 * Shapes only: consumer-neutral, DOM-free. The artifact travels as inline
 * self-contained HTML (a target renders it live — web uses a sandboxed
 * iframe), as a URL when the artifact is served, or as the last-observed
 * screenshot when only pixels are available. Honesty rules as everywhere:
 * no artifact yet = the awaiting frame; an unmeasured page carries no score
 * chip; RAW timestamps so every surface formats its own idiom — never a
 * fabricated pulse.
 */

/** The `Content` purpose key the canvas region dispatches on. A design-bench
 *  run room's canvas slot declares this purpose. */
export const CANVAS_PURPOSE = 'canvas';

/** The viewport the observation was taken at — the same `{width, height}`
 *  parameterization `perception/observe` takes (§1: shipped). RAW pixels;
 *  a target formats the chip ("1440×900"). */
export interface CanvasViewportVM {
  readonly width: number;
  readonly height: number;
}

/** One graded check of the craft scorecard — the oracle ladder's objective
 *  rungs (§3): `v1` = structure (`UiCheck` on the element tree), `v2` =
 *  measured craft (`StyleCheck` over rects + computed styles). V3 aesthetic
 *  judgment is an OBJECTIVE, never a gate — it rides `CanvasScoreVM.judge`,
 *  not this list. */
export interface CanvasCheckVM {
  /** Human-readable assertion ("contrast: hero text ≥ 4.5:1"). */
  readonly name: string;
  /** Oracle tier: `v1` structure gate, `v2` measured-craft gate. */
  readonly tier: 'v1' | 'v2';
  readonly passed: boolean;
  /** Optional measured fact ("3.1:1 measured at 360w") — the receipt a
   *  citizen iterates against. Absent = the check carries only its verdict. */
  readonly detail?: string;
}

/** The craft scorecard summary — gates first (V1·V2 checks), the weighed
 *  judge objective beside them when one exists. */
export interface CanvasScoreVM {
  /** Gate checks passed (V1+V2). */
  readonly passed: number;
  /** Gate checks total (V1+V2). */
  readonly total: number;
  /** The individual gate checks, failures first upstream or not — a target
   *  draws them in given order (the projection owns ordering). */
  readonly checks: readonly CanvasCheckVM[];
  /** The V3 judge-panel objective 0..=1, when a panel has scored — a WEIGHT,
   *  never a gate (§3). Absent = unjudged, honest. */
  readonly judge?: number;
}

/** The rendered artifact itself. Exactly one source field is expected; a
 *  target prefers `html` (live render) over `url` (live, served) over
 *  `screenshot` (last pixels). All absent never occurs on a body that
 *  carries an artifact — the projection folds a source-less observation to
 *  `artifact: undefined` (the awaiting frame) instead. */
export interface CanvasArtifactVM {
  /** What the page is ("index.html — pricing card", the task's artifact
   *  name) — the stage header's title. */
  readonly title: string;
  /** The page as inline self-contained HTML — a target renders it LIVE
   *  (web: `<iframe sandbox srcdoc>`). The persona's actual writing. */
  readonly html?: string;
  /** The artifact's URL when it is served rather than inlined. */
  readonly url?: string;
  /** The last-observed screenshot as a data URL — the pixels-only fallback
   *  when no renderable source is available (a target draws an `<img>`). */
  readonly screenshot?: string;
}

/** The canvas region's content body. `artifact` absent renders the awaiting
 *  frame (the room exists, no page observed yet — the frame is the promise);
 *  every other field is honestly absent until an observation carries it. */
export interface CanvasContentBody {
  /** The rendered page, when at least one observation has landed. */
  readonly artifact?: CanvasArtifactVM;
  /** The observing citizen ("Asha") — the canvas shows WHO, always,
   *  when an observation names her. */
  readonly persona?: string;
  /** WHEN the page was last observed (epoch ms, RAW — every surface
   *  formats its own idiom). Absent or 0 = unknown, no stamp drawn. */
  readonly observedAtMs?: number;
  /** The observation's viewport, when the observe carried one. */
  readonly viewport?: CanvasViewportVM;
  /** The craft scorecard, when the room's oracle has graded — absent =
   *  unmeasured, the stage renders without a score chip (never a dressed 0). */
  readonly score?: CanvasScoreVM;
  /** Observation count for this artifact (the design-is-a-LOOP pulse: the
   *  walk-in sees "obs #7" tick as she iterates). Absent = not tracked. */
  readonly revision?: number;
  /** True only when a live observe stream is attached — a static projection
   *  renders the honest "snapshot" banner (same contract as serving/bench). */
  readonly feedLive: boolean;
}
