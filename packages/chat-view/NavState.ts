/**
 * The `kind="nav"` envelope→state seam — the nav analogue of `ChatState.ts`.
 *
 * A citizen's `NavViewState` (open tabs with derived unread, current tab, read
 * cursors, bookmarks) arrives on the SAME `StateConnection` as chat, from the
 * session's `?me=` scoped per-user substrate. This file single-sources the wire
 * kind string and the fail-loud lift so subscription and merge can't drift —
 * identical discipline to `CHAT_KIND` / `chatStateFromEnvelope`.
 */

import type { NavViewState, StateEnvelope } from '@continuum/sdk-typescript';

/** The wire `kind` string the nav view is published under (Rust
 *  `NavViewState::KIND`). Single-sourced here for the TS side. */
export const NAV_KIND = 'nav';

/**
 * Lift a `nav` `StateEnvelope` into a `NavViewState`. Fails loud on a kind
 * mismatch — a `StateConnection` routes by kind, so a non-nav envelope reaching
 * here is a wiring bug, never something to coerce
 * ([[fallbacks-are-illegal-fail-loud]]).
 */
export function navStateFromEnvelope(envelope: StateEnvelope): NavViewState {
  if (envelope.kind !== NAV_KIND) {
    throw new Error(
      `navStateFromEnvelope: expected kind '${NAV_KIND}', got '${envelope.kind}'. ` +
        'A non-nav envelope reached the nav merge seam — check the StateConnection routing.',
    );
  }
  return envelope.payload as NavViewState;
}
