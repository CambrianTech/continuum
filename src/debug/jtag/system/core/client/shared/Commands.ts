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

import { JTAGClient } from './JTAGClient';
import type { CommandParams, CommandResult } from '../../types/JTAGTypes';
import type {
  CommandName,
  CommandInputFor,
  CommandResultFor,
  CommandParamsFor
} from './CommandRegistry';

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
    // Get the shared JTAG client instance (works in both browser and server)
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