/**
 * System Command Constants
 *
 * System-level commands (session, user, test, etc.)
 * Usage:
 *   import { SYSTEM_COMMANDS } from './commands/shared/SystemCommandConstants';
 *   await Commands.execute(SYSTEM_COMMANDS.PING, params);
 */

export const SYSTEM_COMMANDS = {
  /** Ping server to check connectivity */
  PING: 'ping',

  /** Execute shell command */
  EXEC: 'exec',

  /** List available commands */
  LIST: 'list',

  /** Get process registry info */
  PROCESS_REGISTRY: 'process-registry',

  /** Compile TypeScript */
  COMPILE_TYPESCRIPT: 'compile-typescript',
} as const;

export const SESSION_COMMANDS = {
  /** Create new session */
  CREATE: 'session/create',

  /** Destroy session */
  DESTROY: 'session/destroy',
} as const;

export const USER_COMMANDS = {
  /** Create new user */
  CREATE: 'user/create',
} as const;

export const TEST_COMMANDS = {
  /** Run test */
  TEST: 'test',

  /** Run test suite */
  RUN_SUITE: 'test/run/suite',

  /** Test routing chaos */
  ROUTING_CHAOS: 'test/routing-chaos',
} as const;

export const PIPE_COMMANDS = {
  /** Chain multiple commands */
  CHAIN: 'utilities/pipe/chain',
} as const;

/**
 * Type-safe system command names
 */
export type SystemCommand = typeof SYSTEM_COMMANDS[keyof typeof SYSTEM_COMMANDS];
export type SessionCommand = typeof SESSION_COMMANDS[keyof typeof SESSION_COMMANDS];
export type UserCommand = typeof USER_COMMANDS[keyof typeof USER_COMMANDS];
export type TestCommand = typeof TEST_COMMANDS[keyof typeof TEST_COMMANDS];
export type PipeCommand = typeof PIPE_COMMANDS[keyof typeof PIPE_COMMANDS];
