/**
 * The Experience app's data source for the web client — assembles the composite
 * `ExperienceState` from the three region-payload kinds a room's Workspace needs and
 * pushes it on every change:
 *   - `"experience"` → the manifest (STRUCTURE: purpose / regions / affordances /
 *     membership-standing),
 *   - `"roster"`     → the rich roster (DISPLAY: names / kind glyphs / vitals meters), and
 *   - `"chat"`       → the content BODY (messages), projected through the SHARED
 *     `@continuum/chat-view` seam so it reads identically to the standalone chat widget.
 *
 * The app itself (`experienceApp`) never names its transport; this injects the
 * subscribe seam so the SAME app mounts against a real core, a replay, or a test
 * fixture ([[logical-portability-for-unknown-future-integrations]]). Path-3 in action:
 * the manifest drives the shell, each region binds to its own live payload kind.
 */

import type { AppSource } from '@continuum/patterns';
import type { Experience, RosterViewState, StateEnvelope } from '@continuum/sdk-typescript';
import {
  CHAT_KIND,
  chatStateFromEnvelope,
  chatViewModel,
  type ChatContentBody,
} from '@continuum/chat-view';
import {
  EXPERIENCE_KIND,
  ROSTER_KIND,
  experienceFromEnvelope,
  rosterFromEnvelope,
  type ExperienceState,
} from '@continuum/experience-view';

/** The subscribe seam this source needs — `StateConnection.on` narrowed to
 *  `(kind, sink) → { off }`. Injected so the source is testable without a live socket. */
export type OnKind = (
  kind: string,
  sink: (envelope: StateEnvelope) => void,
) => { off(): void };

/**
 * Build the Experience app's `AppSource` over a subscribe seam. Subscribes the three
 * kinds, keeps the latest of each, and emits a composite `ExperienceState` whenever any
 * changes — but ONLY once the manifest has arrived (the room has no Workspace without
 * its structure). Returns a teardown that `off()`s every subscription.
 */
export function makeExperienceSource(on: OnKind): AppSource<ExperienceState> {
  return (onState) => {
    let manifest: Experience | undefined;
    let roster: RosterViewState | undefined;
    let contentBody: ChatContentBody | undefined;

    const emit = (): void => {
      if (manifest) onState({ manifest, roster, contentBody });
    };

    const subs = [
      on(EXPERIENCE_KIND, (env) => {
        manifest = experienceFromEnvelope(env);
        emit();
      }),
      on(ROSTER_KIND, (env) => {
        roster = rosterFromEnvelope(env);
        emit();
      }),
      on(CHAT_KIND, (env) => {
        // Project the chat payload through the shared seam → the content body the
        // "chat" content renderer draws (identical to the standalone chat widget).
        const vm = chatViewModel(chatStateFromEnvelope(env));
        contentBody = { messages: vm.messages, transcript: vm.transcript, isEmpty: vm.isEmpty };
        emit();
      }),
    ];

    return () => {
      for (const sub of subs) sub.off();
    };
  };
}
