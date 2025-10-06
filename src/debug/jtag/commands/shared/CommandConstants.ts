/**
 * Command Constants - Central Re-Export
 *
 * Aggregates all command constants from modular locations.
 * Provides both granular imports and convenience ALL_COMMANDS.
 *
 * Usage:
 *   // Granular import (preferred for tree-shaking)
 *   import { DATA_COMMANDS } from './commands/data/shared/DataCommandConstants';
 *   await Commands.execute(DATA_COMMANDS.LIST, params);
 *
 *   // Central import (convenience)
 *   import { ALL_COMMANDS } from './commands/shared/CommandConstants';
 *   await Commands.execute(ALL_COMMANDS.DATA.LIST, params);
 */

// Re-export all modular constants
export { DATA_COMMANDS, type DataCommand } from '../data/shared/DataCommandConstants';
export { DEBUG_COMMANDS, type DebugCommand } from '../debug/shared/DebugCommandConstants';
export { STATE_COMMANDS, type StateCommand } from '../state/shared/StateCommandConstants';
export { FILE_COMMANDS, type FileCommand } from '../file/shared/FileCommandConstants';
export { THEME_COMMANDS, type ThemeCommand } from '../theme/shared/ThemeCommandConstants';
export { UI_COMMANDS, type UICommand } from './UICommandConstants';
export {
  SYSTEM_COMMANDS,
  SESSION_COMMANDS,
  USER_COMMANDS,
  TEST_COMMANDS,
  PIPE_COMMANDS,
  type SystemCommand,
  type SessionCommand,
  type UserCommand,
  type TestCommand,
  type PipeCommand
} from './SystemCommandConstants';

// Import for aggregation
import { DATA_COMMANDS } from '../data/shared/DataCommandConstants';
import { DEBUG_COMMANDS } from '../debug/shared/DebugCommandConstants';
import { STATE_COMMANDS } from '../state/shared/StateCommandConstants';
import { FILE_COMMANDS } from '../file/shared/FileCommandConstants';
import { THEME_COMMANDS } from '../theme/shared/ThemeCommandConstants';
import { UI_COMMANDS } from './UICommandConstants';
import {
  SYSTEM_COMMANDS,
  SESSION_COMMANDS,
  USER_COMMANDS,
  TEST_COMMANDS,
  PIPE_COMMANDS
} from './SystemCommandConstants';

/**
 * Aggregated command constants - convenience for autocomplete
 */
export const ALL_COMMANDS = {
  DATA: DATA_COMMANDS,
  DEBUG: DEBUG_COMMANDS,
  STATE: STATE_COMMANDS,
  FILE: FILE_COMMANDS,
  THEME: THEME_COMMANDS,
  UI: UI_COMMANDS,
  SYSTEM: SYSTEM_COMMANDS,
  SESSION: SESSION_COMMANDS,
  USER: USER_COMMANDS,
  TEST: TEST_COMMANDS,
  PIPE: PIPE_COMMANDS,
} as const;

/**
 * Get all command names as flat array (for validation/debugging)
 */
export const ALL_COMMAND_NAMES = [
  ...Object.values(DATA_COMMANDS),
  ...Object.values(DEBUG_COMMANDS),
  ...Object.values(STATE_COMMANDS),
  ...Object.values(FILE_COMMANDS),
  ...Object.values(THEME_COMMANDS),
  ...Object.values(UI_COMMANDS),
  ...Object.values(SYSTEM_COMMANDS),
  ...Object.values(SESSION_COMMANDS),
  ...Object.values(USER_COMMANDS),
  ...Object.values(TEST_COMMANDS),
  ...Object.values(PIPE_COMMANDS),
] as const;

/**
 * Type-safe union of all command names
 */
export type CommandName = typeof ALL_COMMAND_NAMES[number];

/**
 * Check if a string is a valid command name
 */
export function isValidCommand(command: string): command is CommandName {
  return ALL_COMMAND_NAMES.includes(command as CommandName);
}

/**
 * Parse command to get domain and action
 *
 * @example
 * parseCommand('data/list') // { domain: 'data', action: 'list' }
 * parseCommand('screenshot') // { domain: 'ui', action: 'screenshot' }
 */
export function parseCommand(command: string): { domain: string; action: string } | null {
  const parts = command.split('/');

  if (parts.length === 1) {
    // Top-level command (screenshot, ping, etc.)
    return { domain: 'ui', action: parts[0] };
  } else if (parts.length === 2) {
    // Domain/action command (data/list, theme/set, etc.)
    return { domain: parts[0], action: parts[1] };
  } else if (parts.length === 3) {
    // Nested command (test/run/suite)
    return { domain: parts[0], action: `${parts[1]}/${parts[2]}` };
  }

  return null;
}
