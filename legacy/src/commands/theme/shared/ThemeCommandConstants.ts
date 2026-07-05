/**
 * Theme Command Constants
 *
 * All theme/* command names defined here.
 * Usage:
 *   import { THEME_COMMANDS } from './commands/theme/shared/ThemeCommandConstants';
 *   await Commands.execute(THEME_COMMANDS.SET, params);
 */

export const THEME_COMMANDS = {
  /** Get current theme */
  GET: 'theme/get',

  /** List available themes */
  LIST: 'theme/list',

  /** Set active theme */
  SET: 'theme/set',
} as const;

/**
 * Type-safe theme command names
 */
export type ThemeCommand = typeof THEME_COMMANDS[keyof typeof THEME_COMMANDS];
