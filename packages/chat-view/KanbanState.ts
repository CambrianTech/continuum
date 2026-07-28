/**
 * The `kind="kanban"` envelope→state seam — the board analogue of
 * `SystemMetricsState.ts`.
 *
 * The node's work board (`KanbanViewState`: lanes + cards) arrives on the SAME
 * `StateConnection` as chat/nav. The persona home's claims feed filters its
 * cards by assignee. This file single-sources the wire kind string and the
 * fail-loud lift — identical discipline to `CHAT_KIND` / `NAV_KIND`.
 */

import type { KanbanViewState, StateEnvelope } from '@continuum/sdk-typescript';

/** The wire `kind` string the work board is published under (Rust
 *  `KanbanViewState::KIND`). Single-sourced here for the TS side. */
export const KANBAN_KIND = 'kanban';

/**
 * Lift a `kanban` `StateEnvelope` into a `KanbanViewState`. Fails loud on a
 * kind mismatch — a `StateConnection` routes by kind, so a non-kanban envelope
 * reaching here is a wiring bug, never something to coerce
 * ([[fallbacks-are-illegal-fail-loud]]).
 */
export function kanbanStateFromEnvelope(envelope: StateEnvelope): KanbanViewState {
  if (envelope.kind !== KANBAN_KIND) {
    throw new Error(
      `kanbanStateFromEnvelope: expected kind '${KANBAN_KIND}', got '${envelope.kind}'. ` +
        'A non-kanban envelope reached the kanban merge seam — check the StateConnection routing.',
    );
  }
  return envelope.payload as KanbanViewState;
}
