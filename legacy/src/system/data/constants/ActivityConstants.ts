/**
 * Activity Constants - Stable identifiers for collaborative activities
 *
 * Activities are runtime instances of Recipes where participants collaborate.
 * Unlike rooms (which are chat-only), activities can be:
 * - Canvas drawings
 * - Browser co-browsing sessions
 * - Game sessions
 * - Academy lessons
 *
 * These uniqueIds are used to find activities reliably across the system.
 */

export const ACTIVITY_UNIQUE_IDS = {
  // Collaborative canvas - the default canvas for drawing
  CANVAS_MAIN: 'canvas-main',

  // Browser co-browsing session
  BROWSER_MAIN: 'browser-main'
} as const;

export type ActivityUniqueId = typeof ACTIVITY_UNIQUE_IDS[keyof typeof ACTIVITY_UNIQUE_IDS];
