/**
 * State Command Constants
 *
 * All state/* command names defined here.
 * Usage:
 *   import { STATE_COMMANDS } from './commands/state/shared/StateCommandConstants';
 *   await Commands.execute(STATE_COMMANDS.GET, params);
 */

export const STATE_COMMANDS = {
  /** Get current state */
  GET: 'state/get',

  /** Create new state entry */
  CREATE: 'state/create',

  /** Update existing state */
  UPDATE: 'state/update',
} as const;

/**
 * Type-safe state command names
 */
export type StateCommand = typeof STATE_COMMANDS[keyof typeof STATE_COMMANDS];
