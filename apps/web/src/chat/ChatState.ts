/**
 * `ChatState` — a `ChatViewState` payload lifted into a positron `ViewState`.
 *
 * On the wire the `kind` and `revision` live on the `StateEnvelope`, NOT on the
 * payload — `ChatViewState` has neither field. positron's renderer contract,
 * though, keys off a state object that carries its own `kind`/`revision` (a
 * `ViewState`). So the app MERGES the envelope's `kind`/`revision` onto the
 * payload once, at the seam where a `StateConnection` sink hands a
 * `StateEnvelope` to the widget. That merged object is `ChatState`.
 *
 * This is the one place the two positron halves — neutral transport
 * (`StateEnvelope`) and concrete payload (`ChatViewState`) — are joined, and it
 * lives in the APP, not the SDK: the SDK stays a neutral envelope courier
 * ([[headless-core-many-clients]]).
 */

import type { ChatViewState, StateEnvelope } from '@continuum/sdk-typescript';

/** A `ChatViewState` carrying the envelope's `kind`/`revision` — the positron
 *  `ViewState` shape a renderer consumes. */
export type ChatState = ChatViewState & {
  readonly kind: string;
  readonly revision?: number;
};

/** The wire `kind` string this widget subscribes to and renders. Single-sourced
 *  here so the subscription and the render seam can't drift. */
export const CHAT_KIND = 'chat';

/**
 * Lift a `chat` `StateEnvelope` into a `ChatState`. Fails loud on a kind
 * mismatch — a `StateConnection` routes by kind, so a non-chat envelope reaching
 * here is a wiring bug, never something to coerce ([[fallbacks-are-illegal-fail-loud]]).
 */
export function chatStateFromEnvelope(envelope: StateEnvelope): ChatState {
  if (envelope.kind !== CHAT_KIND) {
    throw new Error(
      `chatStateFromEnvelope: expected kind '${CHAT_KIND}', got '${envelope.kind}'. ` +
        'A non-chat envelope reached the chat merge seam — check the StateConnection routing.',
    );
  }
  // The payload IS a ChatViewState (the substrate serializes it under this kind);
  // narrow it and graft the envelope-level kind/revision so it satisfies ViewState.
  return {
    ...(envelope.payload as ChatViewState),
    kind: envelope.kind,
    revision: envelope.revision,
  };
}
