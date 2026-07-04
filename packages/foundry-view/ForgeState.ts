/**
 * `ForgeState` — a `ForgeViewState` payload lifted into a positron `ViewState`.
 *
 * Mirror of `@continuum/chat-view`'s `ChatState`: on the wire the `kind`/`revision`
 * live on the `StateEnvelope`, not the payload, so a client merges them onto the
 * `ForgeViewState` once at the seam where a `StateConnection` sink hands the
 * envelope to a renderer. That merged object is `ForgeState`. Single-sourced here
 * so the subscription and the render seam can't drift.
 */

import type { ForgeViewState, StateEnvelope } from '@continuum/sdk-typescript';

/** A `ForgeViewState` carrying the envelope's `kind`/`revision` — the positron
 *  `ViewState` shape a foundry renderer consumes. */
export type ForgeState = ForgeViewState & {
  readonly kind: string;
  readonly revision?: number;
};

/** The wire `kind` string the foundry widget subscribes to and renders. Single-
 *  sourced so the subscription and the render seam can't drift. */
export const FOUNDRY_KIND = 'foundry';

/**
 * Lift a `foundry` `StateEnvelope` into a `ForgeState`. Fails loud on a kind
 * mismatch — a `StateConnection` routes by kind, so a non-foundry envelope reaching
 * here is a wiring bug, never something to coerce ([[fallbacks-are-illegal-fail-loud]]).
 */
export function forgeStateFromEnvelope(envelope: StateEnvelope): ForgeState {
  if (envelope.kind !== FOUNDRY_KIND) {
    throw new Error(
      `forgeStateFromEnvelope: expected kind '${FOUNDRY_KIND}', got '${envelope.kind}'. ` +
        'A non-foundry envelope reached the foundry merge seam — check the StateConnection routing.',
    );
  }
  return {
    ...(envelope.payload as ForgeViewState),
    kind: envelope.kind,
    revision: envelope.revision,
  };
}
