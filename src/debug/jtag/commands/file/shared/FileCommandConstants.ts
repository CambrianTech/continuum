/**
 * File Command Constants
 *
 * All file/* command names defined here.
 * Usage:
 *   import { FILE_COMMANDS } from './commands/file/shared/FileCommandConstants';
 *   await Commands.execute(FILE_COMMANDS.LOAD, params);
 */

export const FILE_COMMANDS = {
  /** Load file contents */
  LOAD: 'file/load',

  /** Save file contents */
  SAVE: 'file/save',

  /** Append to file */
  APPEND: 'file/append',
} as const;

/**
 * Type-safe file command names
 */
export type FileCommand = typeof FILE_COMMANDS[keyof typeof FILE_COMMANDS];
