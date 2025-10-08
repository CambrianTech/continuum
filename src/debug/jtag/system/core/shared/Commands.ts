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
   * Execute a command - automatic type inference from registry
   * Usage: await Commands.execute('screenshot', { querySelector: 'body' });
   */
  static execute<TCommand extends CommandName>(
    command: TCommand,
    params?: CommandInputFor<TCommand>
  ): Promise<CommandResultFor<TCommand>>;

  /**
   * Execute a command - manual typing for commands not in registry
   * Usage: await Commands.execute<StateCreateParams, StateCreateResult>('state/create', params);
   */
  static execute<T extends CommandParams, U extends CommandResult>(
    command: string,
    params?: Omit<T, 'context' | 'sessionId'>
  ): Promise<U>;

  // Implementation
  static async execute(
    command: string,
    params?: any
  ): Promise<any> {
    // Server-side optimization: If we're already in a server context with a CommandDaemon,
    // route internally instead of creating a new client connection
    if (typeof process !== 'undefined' && (globalThis as any).__JTAG_COMMAND_DAEMON__) {
      const commandDaemon = (globalThis as any).__JTAG_COMMAND_DAEMON__;
      const finalParams = {
        context: params?.context || (globalThis as any).__JTAG_CONTEXT__,
        sessionId: params?.sessionId || (globalThis as any).__JTAG_SESSION_ID__,
        ...(params || {})
      } as any;

      // Route command internally via CommandDaemon
      const commandInstance = commandDaemon.commands.get(command);
      if (commandInstance) {
        return await commandInstance.execute(finalParams);
      }
    }

    // Client-side or fallback: Use JTAGClient
    const jtagClient = await JTAGClient.sharedInstance;

    // Auto-inject context and sessionId
    const finalParams = {
      context: jtagClient.context,
      sessionId: jtagClient.sessionId,
      ...(params || {})
    } as any;

    // Execute and get typed result (unwrapped by daemons.commands.execute)
    return await jtagClient.daemons.commands.execute<any, any>(command, finalParams);
  }
}