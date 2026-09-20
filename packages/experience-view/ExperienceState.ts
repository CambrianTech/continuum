/**
 * The envelope→payload seams for the Experience (Join Contract) activity — the two
 * region payload kinds a renderer subscribes to and joins into one Workspace:
 *   - `EXPERIENCE_KIND` (`"experience"`) → the room's manifest (STRUCTURE: purpose,
 *     regions, affordances, membership-standing, layout), and
 *   - `ROSTER_KIND` (`"roster"`) → the rich roster payload (DISPLAY data: names, kinds,
 *     vitals meters) the manifest's minimal `Member` intentionally omits (path-3
 *     per-region ViewStates).
 *
 * Kinds are single-sourced here so the subscription and the render seam can't drift,
 * mirroring `@continuum/chat-view`'s `CHAT_KIND`. Fails loud on a kind mismatch — a
 * `StateConnection` routes by kind, so a wrong envelope here is a wiring bug, never
 * something to coerce ([[fallbacks-are-illegal-fail-loud]]).
 */

import type { Experience, RosterViewState, StateEnvelope } from '@continuum/sdk-typescript';

/** The wire `kind` carrying the room's Experience manifest. Matches Rust `Experience::KIND`. */
export const EXPERIENCE_KIND = 'experience';

/** The wire `kind` carrying the room's rich roster. Matches Rust `RosterViewState::KIND`. */
export const ROSTER_KIND = 'roster';

/** Narrow an `experience` `StateEnvelope` to its `Experience` payload. */
export function experienceFromEnvelope(envelope: StateEnvelope): Experience {
  if (envelope.kind !== EXPERIENCE_KIND) {
    throw new Error(
      `experienceFromEnvelope: expected kind '${EXPERIENCE_KIND}', got '${envelope.kind}'. ` +
        'A non-experience envelope reached the manifest seam — check the StateConnection routing.',
    );
  }
  return envelope.payload as Experience;
}

/** Narrow a `roster` `StateEnvelope` to its `RosterViewState` payload. */
export function rosterFromEnvelope(envelope: StateEnvelope): RosterViewState {
  if (envelope.kind !== ROSTER_KIND) {
    throw new Error(
      `rosterFromEnvelope: expected kind '${ROSTER_KIND}', got '${envelope.kind}'. ` +
        'A non-roster envelope reached the roster seam — check the StateConnection routing.',
    );
  }
  return envelope.payload as RosterViewState;
}
