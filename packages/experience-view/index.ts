/**
 * `@continuum/experience-view` — the shared, framework-free Experience (Join Contract)
 * projection. The manifest → `WorkspaceView` view model, single-sourced so every
 * renderer (web Lit, terminal, mobile Flutter, RAG) draws the same who/what/where and
 * they can't drift. Holds NO transport, NO DOM, NO ANSI.
 */

export {
  EXPERIENCE_KIND,
  ROSTER_KIND,
  experienceFromEnvelope,
  rosterFromEnvelope,
} from './ExperienceState';

export { experienceWorkspace, rosterListing, roomsListing } from './experienceWorkspace';
export type { ExperienceState } from './experienceWorkspace';

// The Experience activity as a positron app, defined ONCE — mount on any RenderTarget.
export { experienceApp } from './experienceApp';
