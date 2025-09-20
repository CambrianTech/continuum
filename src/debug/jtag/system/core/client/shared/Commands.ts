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
   * Execute a command with clean interface
   */
  static async execute<P extends CommandParams, R extends CommandResult>(
    command: string,
    params?: Omit<P, 'context' | 'sessionId'> | P
  ): Promise<R> {
    try {
      // Get the shared JTAG client instance (works in both browser and server)
      const jtagClient = await JTAGClient.sharedInstance;

      // Auto-inject context and sessionId if not already provided
      const finalParams: CommandParams = {
        context: jtagClient.context,
        sessionId: jtagClient.sessionId,
        ...(params || {})
      };

      // Use the JTAG client's elegant daemon interface for command execution
      const result = await jtagClient.daemons.commands.execute<CommandParams, R>(command, finalParams);

      return result;
    } catch (error) {
      console.error(`❌ Commands: Command ${command} failed:`, error);
      throw error;
    }
  }
}