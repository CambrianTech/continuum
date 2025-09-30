/**
 * Commands - Clean Client Interface
 *
 * Provides simple static interface for command execution.
 * Replaces CommandDaemon.execute() with cleaner naming.
 */

import { JTAGClient } from './JTAGClient';
import type { CommandParams, CommandResult } from '../../types/JTAGTypes';

export class Commands {
  /**
   * Execute a command with clean interface - unwraps CommandResponse metadata
   *
   * Generic pattern: <T extends CommandParams, U extends CommandResult>
   * - T: Input params type (unused, kept for compatibility)
   * - U: Output result type (what you get back)
   */
  static async execute<T extends CommandParams, U extends CommandResult>(
    command: string,
    params?: Omit<T, 'context' | 'sessionId'>
  ): Promise<U> {
    // Get the shared JTAG client instance (works in both browser and server)
    const jtagClient = await JTAGClient.sharedInstance;

    // Auto-inject context and sessionId
    const finalParams = {
      context: jtagClient.context,
      sessionId: jtagClient.sessionId,
      ...(params || {})
    } as T;

    // Execute and get typed result (unwrapped by daemons.commands.execute)
    return await jtagClient.daemons.commands.execute<T, U>(command, finalParams);
  }
}