/**
 * Room Constants - Stable identifiers that won't break
 *
 * These uniqueIds are used to find rooms reliably across the system
 */

export const ROOM_UNIQUE_IDS = {
  GENERAL: 'general',
  ACADEMY: 'academy',
  SUPPORT: 'support',
  PANTHEON: 'pantheon',
  DEV_UPDATES: 'dev-updates',
  HELP: 'help',
  THEME: 'theme',
  SETTINGS: 'settings',
  CANVAS: 'canvas',
  OUTREACH: 'outreach',
  NEWSROOM: 'newsroom'
} as const;

export type RoomUniqueId = typeof ROOM_UNIQUE_IDS[keyof typeof ROOM_UNIQUE_IDS];
