/**
 * Canvas projections — a design-bench observation → the neutral
 * `CanvasContentBody` (DESIGN-BENCH-VISUAL-CRAFT.md §5: the run room's
 * canvas region is the persona's RENDERED page, live, re-observed on her
 * writes — the walk-in sees the design evolve).
 *
 * Pure functions, target-free: the same projection feeds web, tui, and RAG.
 * The input mirrors the future core canvas source verbatim (snake_case wire
 * keys, the ArenaViewState precedent): the core's eye-node observe results —
 * artifact + screenshot handle + craft facts (#2397) — fold into this
 * snapshot; a projection never reads files or re-observes.
 *
 * Honesty rules (same contract as bench/arena): a source-less observation
 * folds to the awaiting frame, an ungraded page carries no score, absent
 * facts stay ABSENT — never a fabricated zero.
 */

import type {
  CanvasArtifactVM,
  CanvasCheckVM,
  CanvasContentBody,
  CanvasScoreVM,
  CanvasViewportVM,
} from '@continuum/patterns';

/** The envelope kind the canvas feed rides on (the core-side canvas source
 *  publishes `kind: "canvas"` — the ONE wiring point named in the seam). */
export const CANVAS_KIND = 'canvas';

/**
 * Lift a `canvas` `StateEnvelope` into a `CanvasViewState`. Fails loud on a
 * kind mismatch (the bench/serving fold contract).
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function canvasFromEnvelope(envelope: { readonly kind: string; readonly payload: unknown }): CanvasViewState {
  if (envelope.kind !== CANVAS_KIND) {
    throw new Error(
      `canvasFromEnvelope: expected kind '${CANVAS_KIND}', got '${envelope.kind}'. ` +
        'A non-canvas envelope reached the canvas merge seam — check the StateConnection routing.',
    );
  }
  return envelope.payload as CanvasViewState;
}

/** One graded oracle check as it rides the wire (§3 tiers: `v1` structure
 *  UiCheck, `v2` measured-craft StyleCheck). */
export interface CanvasCheckRow {
  readonly name: string;
  readonly tier: 'v1' | 'v2';
  readonly passed: boolean;
  readonly detail?: string;
}

/** The live-feed envelope state for a canvas region (`kind: "canvas"`).
 *  Mirrors the Arena/Bench state pattern: the core's canvas source folds the
 *  latest observe result into this snapshot. Every field optional except
 *  nothing — an empty snapshot IS the pre-first-observation room. */
export interface CanvasViewState {
  /** The artifact's display name ("index.html — pricing card"). */
  readonly artifact_title?: string;
  /** The page as inline self-contained HTML (the persona's actual writing). */
  readonly artifact_html?: string;
  /** The artifact's URL when it is served rather than inlined. */
  readonly artifact_url?: string;
  /** Last-observed screenshot as a data URL — pixels-only fallback. */
  readonly screenshot_data_url?: string;
  /** The observing citizen's display name. */
  readonly persona?: string;
  /** Epoch ms of the last observation. */
  readonly observed_at_ms?: number;
  /** The observation's viewport, when the observe carried one. */
  readonly viewport?: { readonly width: number; readonly height: number };
  /** Observation count for this artifact (ticks as she iterates). */
  readonly revision?: number;
  /** Graded oracle checks (V1·V2 gates), when the room's oracle has run. */
  readonly checks?: readonly CanvasCheckRow[];
  /** The V3 judge-panel objective 0..=1, when a panel has scored. */
  readonly judge?: number;
}

/** Fold the wire's artifact sources into the neutral artifact VM — or
 *  `undefined` when NO renderable source exists (the awaiting frame; a
 *  title alone is not a page). */
function artifactOf(view: CanvasViewState): CanvasArtifactVM | undefined {
  if (
    view.artifact_html === undefined &&
    view.artifact_url === undefined &&
    view.screenshot_data_url === undefined
  ) {
    return undefined;
  }
  return {
    title: view.artifact_title ?? 'untitled artifact',
    ...(view.artifact_html !== undefined ? { html: view.artifact_html } : {}),
    ...(view.artifact_url !== undefined ? { url: view.artifact_url } : {}),
    ...(view.screenshot_data_url !== undefined ? { screenshot: view.screenshot_data_url } : {}),
  };
}

/** The scorecard VM — exists only when the oracle actually graded (at least
 *  one check on the wire). Failures sort FIRST (the facts a citizen iterates
 *  against lead); ties keep wire order (stable). */
function scoreOf(view: CanvasViewState): CanvasScoreVM | undefined {
  const rows = view.checks;
  if (!rows || rows.length === 0) return undefined;
  const checks: CanvasCheckVM[] = rows
    .map(
      (c): CanvasCheckVM => ({
        name: c.name,
        tier: c.tier,
        passed: c.passed,
        ...(c.detail !== undefined ? { detail: c.detail } : {}),
      }),
    )
    .sort((a, b) => Number(a.passed) - Number(b.passed));
  return {
    passed: checks.filter((c) => c.passed).length,
    total: checks.length,
    checks,
    ...(view.judge !== undefined ? { judge: view.judge } : {}),
  };
}

/** The canvas region's content body from the live-feed state. `feedLive` is
 *  true only when the canvas subscription has actually delivered (same
 *  contract as serving/arena/bench). */
export function canvasContentBody(view?: CanvasViewState): CanvasContentBody {
  if (!view) return { feedLive: false };
  const artifact = artifactOf(view);
  const viewport: CanvasViewportVM | undefined = view.viewport
    ? { width: view.viewport.width, height: view.viewport.height }
    : undefined;
  const score = scoreOf(view);
  return {
    ...(artifact ? { artifact } : {}),
    ...(view.persona !== undefined ? { persona: view.persona } : {}),
    ...(view.observed_at_ms !== undefined && view.observed_at_ms > 0
      ? { observedAtMs: view.observed_at_ms }
      : {}),
    ...(viewport ? { viewport } : {}),
    ...(score ? { score } : {}),
    ...(view.revision !== undefined ? { revision: view.revision } : {}),
    feedLive: true,
  };
}
