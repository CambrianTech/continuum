/**
 * Debug Command Constants
 *
 * All debug/* command names defined here.
 * Usage:
 *   import { DEBUG_COMMANDS } from './commands/debug/shared/DebugCommandConstants';
 *   await Commands.execute(DEBUG_COMMANDS.LOGS, params);
 */

export const DEBUG_COMMANDS = {
  /** Analyze system logs with filtering and search */
  LOGS: 'debug/logs',

  /** Inspect HTML/DOM structure of widgets */
  HTML_INSPECTOR: 'debug/html-inspector',

  /** Get widget internal state */
  WIDGET_STATE: 'debug/widget-state',

  /** Debug widget event system */
  WIDGET_EVENTS: 'debug/widget-events',

  /** Test scroll behavior and intersection observer */
  SCROLL_TEST: 'debug/scroll-test',

  /** Test CRUD synchronization with UI */
  CRUD_SYNC: 'debug/crud-sync',

  /** Interact with widget (click, type, etc.) */
  WIDGET_INTERACT: 'debug/widget-interact',

  /** Trigger error for testing */
  ERROR: 'debug/error',

  /** Send test chat message */
  CHAT_SEND: 'debug/chat-send',

  /** Debug content types */
  CONTENT_TYPES: 'debug/content-types',

  /** Debug academy sessions */
  ACADEMY_SESSIONS: 'debug/academy-sessions',

  /** Check artifacts system */
  ARTIFACTS_CHECK: 'debug/artifacts-check',
} as const;

/**
 * Type-safe debug command names
 */
export type DebugCommand = typeof DEBUG_COMMANDS[keyof typeof DEBUG_COMMANDS];
