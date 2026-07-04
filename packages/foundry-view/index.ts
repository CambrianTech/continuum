/**
 * `@continuum/foundry-view` — the shared, framework-free foundry projection.
 *
 * Mirror of `@continuum/chat-view` for the `foundry` widget kind: the
 * envelope→state merge (`forgeStateFromEnvelope` / `FOUNDRY_KIND` / `ForgeState`)
 * and the projections onto the consumer-neutral pattern primitives (the model
 * `Listing`, the foundry `Content`/`ContextPanel`). Renderers live in the clients;
 * this package holds NO transport, NO DOM, NO ANSI — only the wire decode +
 * projection, typed against `@continuum/sdk-typescript` and `@continuum/patterns`.
 */

export { FOUNDRY_KIND, forgeStateFromEnvelope } from './ForgeState';
export type { ForgeState } from './ForgeState';

export { modelsListing, foundryContextPanel, foundryContent } from './patternProjections';
export type { ForgeContentBody } from './patternProjections';
