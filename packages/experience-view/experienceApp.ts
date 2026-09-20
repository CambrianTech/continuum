/**
 * The Experience (Join Contract) activity as a positron app, defined ONCE.
 *
 * Mount it on any `RenderTarget` — web (Lit), terminal (ANSI), mobile (Flutter), or a
 * persona's RAG grounding — via `@continuum/patterns`' `mount(app, source, target,
 * sink)`. The app is purely the manifest → `WorkspaceView` projection; the frameworks
 * paint, positron defines ([[three-separable-layers-recipe-positron-universe]]). This is
 * the second `defineApp` consumer, after `chat-view`'s `chatApp`.
 */

import { defineApp, type AppDefinition } from '@continuum/patterns';
import { experienceWorkspace, type ExperienceState } from './experienceWorkspace';

/** The Experience app — projects a room's manifest + region payloads into the neutral
 *  who/what/where. `universe` (look/lore) is left to the target's default until a recipe
 *  names one. */
export const experienceApp: AppDefinition<ExperienceState> = defineApp({
  project: experienceWorkspace,
});
