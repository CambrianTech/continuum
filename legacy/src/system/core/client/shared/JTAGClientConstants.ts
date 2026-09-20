/**
 * JTAG Client Constants - Shared Bootstrap and System Messages
 * 
 * Centralized constants to prevent magic strings in detection logic.
 * Used by both JTAGClient and signal detection systems.
 */

export const JTAG_BOOTSTRAP_MESSAGES = {
  BOOTSTRAP_COMPLETE_PREFIX: 'Bootstrap complete! Discovered',
  BOOTSTRAP_COMPLETE_PATTERN: /Bootstrap complete! Discovered (\d+) commands/,
  DISCOVERY_START: 'Discovering available commands...',
  SESSION_UPDATE_PREFIX: 'Session updated',
} as const;

export const JTAG_LOG_PATTERNS = {
  COMMAND_COUNT_PATTERN: /Discovered (\d+) commands/,
  BOOTSTRAP_PATTERN: /Bootstrap complete/,
  BOOTSTRAP_STRING: 'Bootstrap complete',  // For grep searches
} as const;