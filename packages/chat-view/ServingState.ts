/**
 * The `kind="serving"` envelope→state seam — the serving glass box's feed
 * (#141 slice 1: the beat-WASTE pager control loop on screen). Same fail-loud
 * discipline as `SystemMetricsState.ts`: single-sourced kind string, no
 * coercion.
 */

import type { ServingViewState, StateEnvelope } from '@continuum/sdk-typescript';

/** The wire `kind` string the serving view is published under (Rust
 *  `ServingViewState::KIND`). */
export const SERVING_KIND = 'serving';

/**
 * Lift a `serving` `StateEnvelope` into a `ServingViewState`.
 * Fails loud on a kind mismatch ([[fallbacks-are-illegal-fail-loud]]).
 */
export function servingFromEnvelope(envelope: StateEnvelope): ServingViewState {
  if (envelope.kind !== SERVING_KIND) {
    throw new Error(
      `servingFromEnvelope: expected kind '${SERVING_KIND}', got '${envelope.kind}'. ` +
        'A non-serving envelope reached the serving merge seam — check the StateConnection routing.',
    );
  }
  return envelope.payload as ServingViewState;
}
