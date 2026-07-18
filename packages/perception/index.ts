/**
 * `@continuum/perception` — the Perception Surface (#187): universal eyes/ears/hands for
 * what personas create or observe. `docs/architecture/PERCEPTION-SURFACE.md`.
 *
 * The `Surface` contract + two proven implementations spanning the extremes:
 *   - `DomSurface` (outlier A) — a web page via Playwright (screenshot + DOM/a11y + driver).
 *   - `SceneSurface` (outlier B) — a 3D scene over the real `SceneDescription` invariant.
 * Both fit the ONE trait without forcing, which validates it (SEE/JUDGE/REASON universal;
 * only view-hints + act-verbs per-surface). Next bricks: CV-aid perception adapters, wiring
 * a Surface as a persona-callable command, critique/score/vote.
 */

export * from './surface';
export { imageDiff } from './imageDiff';
export { DomSurface, findChromium, type DomSurfaceOptions, type DomViewSpec, type DomAction } from './domSurface';
export { SceneSurface, type SceneSurfaceOptions, type SceneViewSpec, type SceneAction } from './sceneSurface';
export { PerceptionSession, type Observation, type Interaction } from './session';
