/**
 * `@continuum/perception` — the Perception Surface (#187): universal eyes/ears/hands for
 * what personas create or observe. `docs/architecture/PERCEPTION-SURFACE.md`.
 *
 * Today: the `Surface` contract + `DomSurface` (the web surface via Playwright). Next
 * bricks: extract the trait against a 3D/video outlier, CV-aid perception adapters.
 */

export * from './surface';
export { DomSurface, findChromium, type DomSurfaceOptions } from './domSurface';
