/**
 * The `kind="system-metrics"` envelope→state seam — the SYS gauge's feed
 * (POSITRON-WIDGET-SOPHISTICATION.md brick 2). Same fail-loud discipline as
 * `ChatState.ts` / `NavState.ts`: single-sourced kind string, no coercion.
 */

import type { StateEnvelope, SystemMetricsViewState } from '@continuum/sdk-typescript';

/** The wire `kind` string the node's resource view is published under (Rust
 *  `SystemMetricsViewState::KIND`). */
export const SYSTEM_METRICS_KIND = 'system-metrics';

/**
 * Lift a `system-metrics` `StateEnvelope` into a `SystemMetricsViewState`.
 * Fails loud on a kind mismatch ([[fallbacks-are-illegal-fail-loud]]).
 */
export function systemMetricsFromEnvelope(envelope: StateEnvelope): SystemMetricsViewState {
  if (envelope.kind !== SYSTEM_METRICS_KIND) {
    throw new Error(
      `systemMetricsFromEnvelope: expected kind '${SYSTEM_METRICS_KIND}', got '${envelope.kind}'. ` +
        'A non-metrics envelope reached the metrics merge seam — check the StateConnection routing.',
    );
  }
  return envelope.payload as SystemMetricsViewState;
}
