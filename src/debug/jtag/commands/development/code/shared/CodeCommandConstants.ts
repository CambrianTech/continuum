/**
 * Code Command Constants
 *
 * All development/code/* command names defined here.
 * Usage:
 *   await Commands.execute(CODE_COMMANDS.READ, params);
 */

export const CODE_COMMANDS = {
  /** Read source code from a file */
  READ: 'code/read',

  /** Search for patterns in code */
  FIND: 'code/find',
} as const;

/**
 * Type-safe code command names
 */
export type CodeCommand = typeof CODE_COMMANDS[keyof typeof CODE_COMMANDS];
