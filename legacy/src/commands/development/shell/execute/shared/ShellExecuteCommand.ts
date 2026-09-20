/**
 * Shell Execute Command - Shared Base
 *
 * Provides safe shell command execution for PersonaUsers and other AI agents.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ShellExecuteParams, ShellExecuteResult } from './ShellExecuteTypes';
import { isAllowedCommand, sanitizeArgs, normalizeTimeout, normalizeMaxOutputSize } from './ShellExecuteTypes';

export abstract class ShellExecuteCommand extends CommandBase<ShellExecuteParams, ShellExecuteResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/shell/execute', context, subpath, commander);
  }

  /**
   * Validate command parameters before execution
   * Checks whitelist, sanitizes args, normalizes limits
   */
  protected validateParams(params: ShellExecuteParams): { valid: boolean; error?: string } {
    // Check command is provided
    if (!params.command) {
      return { valid: false, error: 'Command is required' };
    }

    // Check command is whitelisted
    if (!isAllowedCommand(params.command)) {
      return {
        valid: false,
        error: `Command "${params.command}" is not allowed. Use one of: curl, wget, ping, dig, etc.`
      };
    }

    // Validate working directory if provided
    if (params.cwd) {
      // Must be absolute path for security
      if (!params.cwd.startsWith('/')) {
        return { valid: false, error: 'Working directory must be an absolute path' };
      }
    }

    return { valid: true };
  }

  /**
   * Normalize and sanitize parameters for safe execution
   */
  protected normalizeParams(params: ShellExecuteParams): ShellExecuteParams {
    return {
      ...params,
      args: params.args ? sanitizeArgs(params.args) : [],
      timeout: normalizeTimeout(params.timeout),
      maxOutputSize: normalizeMaxOutputSize(params.maxOutputSize)
    };
  }

  /**
   * Format error result with consistent structure
   */
  protected formatErrorResult(params: ShellExecuteParams, error: string, executedCommand?: string): ShellExecuteResult {
    return {
      context: params.context,
      sessionId: params.sessionId,
      success: false,
      error,
      executedCommand,
      exitCode: -1
    };
  }

  /**
   * Format success result with consistent structure
   */
  protected formatSuccessResult(
    params: ShellExecuteParams,
    stdout: string,
    stderr: string,
    exitCode: number,
    executedCommand: string,
    executionTimeMs: number,
    truncated: boolean = false
  ): ShellExecuteResult {
    return {
      context: params.context,
      sessionId: params.sessionId,
      success: exitCode === 0,
      stdout,
      stderr,
      exitCode,
      executedCommand,
      executionTimeMs,
      truncated,
      error: exitCode !== 0 ? stderr : undefined
    };
  }
}
