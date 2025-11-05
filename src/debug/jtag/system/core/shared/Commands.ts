/**
 * Commands - Clean Client Interface with Type-Safe Command Registry
 *
 * Provides elegant static interface for command execution with full type inference.
 * No more manual generic parameters - types are inferred from command name!
 *
 * Usage:
 *   // Type-safe! params and result types inferred automatically
 *   const result = await Commands.execute('screenshot', { querySelector: 'body' });
 *
 *   // IntelliSense shows all available commands
 *   await Commands.execute('file/save', { filepath: 'test.txt', content: 'hello' });
 */

import { JTAGClient } from '../client/shared/JTAGClient';
import type { CommandParams, CommandResult } from '../types/JTAGTypes';
import type {
  CommandName,
  CommandInputFor,
  CommandResultFor,
  CommandParamsFor
} from '../client/shared/CommandRegistry';

export class Commands {
  /**
   * Execute a command with full type safety
   * Context and sessionId are auto-injected, all other params required/optional as defined
   */
  static execute<T extends CommandParams = CommandParams, U extends CommandResult = CommandResult>(
    command: string,
    params?: Partial<T>
  ): Promise<U>;

  // Implementation
  static async execute<T extends CommandParams = CommandParams, U extends CommandResult = CommandResult>(
    command: string,
    params?: Partial<T>
  ): Promise<U> {
    // Server-side optimization: If we're already in a server context with a CommandDaemon,
    // route internally instead of creating a new client connection
    interface GlobalWithJTAG {
      __JTAG_COMMAND_DAEMON__?: unknown;
      __JTAG_CONTEXT__?: string;
      __JTAG_SESSION_ID__?: string;
    }
    const globalWithJTAG = globalThis as GlobalWithJTAG;

    if (typeof process !== 'undefined' && globalWithJTAG.__JTAG_COMMAND_DAEMON__) {
      interface CommandDaemonWithCommands {
        commands: Map<string, { execute(params: CommandParams): Promise<CommandResult> }>;
      }
      const commandDaemon = globalWithJTAG.__JTAG_COMMAND_DAEMON__ as CommandDaemonWithCommands;

      const finalParams: CommandParams = {
        context: params?.context || globalWithJTAG.__JTAG_CONTEXT__ || 'unknown',
        sessionId: params?.sessionId || globalWithJTAG.__JTAG_SESSION_ID__ || 'unknown',
        ...(params || {})
      } as T;

      // Route command internally via CommandDaemon
      const commandInstance = commandDaemon.commands.get(command);
      if (commandInstance) {
        return await commandInstance.execute(finalParams) as U;
      }
    }

    // Client-side or fallback: Use JTAGClient
    const jtagClient = await JTAGClient.sharedInstance;

    // Auto-inject context and sessionId
    const finalParams: T = {
      context: jtagClient.context,
      sessionId: jtagClient.sessionId,
      ...(params || {})
    } as T;

    // Execute and get typed result (unwrapped by daemons.commands.execute)
    return await jtagClient.daemons.commands.execute<T, U>(command, finalParams);
  }
}