/**
 * Shell Execute Command Types
 *
 * Provides safe shell command execution for PersonaUsers and other AI agents.
 * Commands are whitelisted and sanitized to prevent security issues.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

/**
 * Whitelisted shell commands that can be executed
 * Only these commands are allowed for security
 */
export const ALLOWED_COMMANDS = [
  'curl',
  'wget',
  'ping',
  'dig',
  'nslookup',
  'host',
  'whois',
  'traceroute',
  'netstat',
  'echo',
  'cat',
  'head',
  'tail',
  'wc',
  'grep',
  'ls',
  'pwd',
  'date',
  'uptime',
  'whoami',
  'uname',
  'which',
  'whereis'
] as const;

export type AllowedCommand = typeof ALLOWED_COMMANDS[number];

/**
 * Parameters for shell command execution
 */
export interface ShellExecuteParams extends CommandParams {
  /**
   * Command to execute (must be in ALLOWED_COMMANDS whitelist)
   */
  command: string;

  /**
   * Command arguments (sanitized for safety)
   */
  args?: string[];

  /**
   * Working directory (optional, defaults to current directory)
   * Must be an absolute path for security
   */
  cwd?: string;

  /**
   * Timeout in milliseconds (default: 30000, max: 60000)
   */
  timeout?: number;

  /**
   * Maximum output size in bytes (default: 1MB, max: 10MB)
   * Prevents memory exhaustion from large outputs
   */
  maxOutputSize?: number;

  /**
   * Environment variables to set (optional, sanitized)
   */
  env?: Record<string, string>;
}

/**
 * Result from shell command execution
 */
export interface ShellExecuteResult extends CommandResult {
  /**
   * Standard output from command
   */
  stdout?: string;

  /**
   * Standard error from command
   */
  stderr?: string;

  /**
   * Exit code (0 = success, non-zero = error)
   */
  exitCode?: number;

  /**
   * Whether command completed successfully
   */
  success: boolean;

  /**
   * Error message if command failed
   */
  error?: string;

  /**
   * Command that was executed (for debugging)
   */
  executedCommand?: string;

  /**
   * Execution time in milliseconds
   */
  executionTimeMs?: number;

  /**
   * Whether output was truncated due to size limit
   */
  truncated?: boolean;
}

/**
 * Validate that a command is in the whitelist
 */
export function isAllowedCommand(command: string): command is AllowedCommand {
  return ALLOWED_COMMANDS.includes(command as AllowedCommand);
}

/**
 * Sanitize command arguments to prevent injection attacks
 * Removes dangerous characters and patterns
 */
export function sanitizeArgs(args: string[]): string[] {
  return args.map(arg => {
    // Remove shell metacharacters that could be dangerous
    // Keep alphanumeric, spaces, hyphens, underscores, dots, slashes, colons, equals
    return arg.replace(/[^a-zA-Z0-9\s\-_./:=@]/g, '');
  });
}

/**
 * Validate and normalize timeout value
 */
export function normalizeTimeout(timeout?: number): number {
  const DEFAULT_TIMEOUT = 30000; // 30 seconds
  const MAX_TIMEOUT = 60000; // 60 seconds

  if (timeout === undefined) {
    return DEFAULT_TIMEOUT;
  }

  return Math.min(Math.max(timeout, 0), MAX_TIMEOUT);
}

/**
 * Validate and normalize max output size
 */
export function normalizeMaxOutputSize(maxOutputSize?: number): number {
  const DEFAULT_MAX_SIZE = 1024 * 1024; // 1MB
  const MAX_SIZE = 10 * 1024 * 1024; // 10MB

  if (maxOutputSize === undefined) {
    return DEFAULT_MAX_SIZE;
  }

  return Math.min(Math.max(maxOutputSize, 0), MAX_SIZE);
}
