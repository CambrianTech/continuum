/**
 * The `kind="bench"` envelope→state seam — the academy benchmark board's feed
 * (#329: a benchmark IS a live room; the run rows ARE the panel). Same
 * fail-loud discipline as `ServingState.ts`: single-sourced kind string, no
 * coercion.
 */

import type { BenchViewState, StateEnvelope } from '@continuum/sdk-typescript';

/** The wire `kind` string the bench view is published under (Rust
 *  `BenchViewState::KIND`). */
export const BENCH_KIND = 'bench';

/**
 * Lift a `bench` `StateEnvelope` into a `BenchViewState`.
 * Fails loud on a kind mismatch ([[fallbacks-are-illegal-fail-loud]]).
 */
export function benchFromEnvelope(envelope: StateEnvelope): BenchViewState {
  if (envelope.kind !== BENCH_KIND) {
    throw new Error(
      `benchFromEnvelope: expected kind '${BENCH_KIND}', got '${envelope.kind}'. ` +
        'A non-bench envelope reached the bench merge seam — check the StateConnection routing.',
    );
  }
  return envelope.payload as BenchViewState;
}
