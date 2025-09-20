/**
 * Command Daemon Base - Shared Logic
 * 
 * Base class for CommandDaemon implementations.
 * Contains shared logic while allowing browser/server specific implementations.
 */

import { DaemonBase } from './DaemonBase';
import type { JTAGContext, JTAGMessage, CommandParams, CommandResult } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type{ CommandBase, CommandEntry } from './CommandBase';
import type { CommandResponse } from './CommandResponseTypes';
import { createCommandErrorResponse, createCommandSuccessResponse } from './CommandResponseTypes';
import { type UUID } from '../../../system/core/types/CrossPlatformUUID';
import { globalSessionContext } from '../../../system/core/types/SystemScopes';
import { JTAGClient } from '../../../system/core/client/shared/JTAGClient';
import type { CommandErrorResponse, CommandSuccessResponse } from './CommandResponseTypes';


export abstract class CommandDaemon extends DaemonBase {
  public readonly subpath: string = 'commands';
  public commands: Map<string, CommandBase<CommandParams, CommandResult>> = new Map();

  /**
   * Static convenience method - same as BaseWidget.executeCommand but domain-owned
   * Gives CommandDaemon control over caching, optimization, batching, retries
   *
   * @example
   * // ONE LINE CHANGE: Replace this.executeCommand with CommandDaemon.execute
   * const result = await CommandDaemon.execute<DataListParams, DataListResult<UserData>>('data/list', {
   *   collection: COLLECTIONS.USERS
   * });
   */
  static async execute<P extends CommandParams, R extends CommandResult>(
    command: string,
    params?: Omit<P, 'context' | 'sessionId'> | P
  ): Promise<R> {
    try {
      // Use window.jtag directly like BaseWidget does
      const client = (window as any).jtag;
      if (!client?.commands) {
        throw new Error('JTAG client not available - system not ready');
      }

      // Auto-inject context and sessionId if not already provided (same logic as BaseWidget)
      let finalParams = params || {} as P;
      if (!('context' in finalParams) || !('sessionId' in finalParams)) {
        const jtagClient = await JTAGClient.sharedInstance;
        finalParams = {
          context: jtagClient.context,
          sessionId: jtagClient.sessionId,
          ...finalParams
        } as P;
      }

      // Execute command through the global JTAG system - same as BaseWidget
      const wrappedResult = await client.commands[command](finalParams) as CommandResponse;

      if (!wrappedResult.success) {
        const commandError = wrappedResult as CommandErrorResponse;
        throw new Error(commandError.error ?? `Command ${command} failed without error message`);
      }

      // Type-safe access to commandResult for success responses
      const successResult = wrappedResult as CommandSuccessResponse;

      // Extract the actual command result from the wrapped response
      const finalResult = successResult.commandResult as R;

      // Check if commandResult is missing and use direct result
      if (!finalResult && wrappedResult.success) {
        return wrappedResult as unknown as R;
      }

      return finalResult;
    } catch (error) {
      console.error(`❌ CommandDaemon: Command ${command} failed:`, error);
      throw error;
    }
  }

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('command-daemon', context, router);
  }

  /**
   * Register a command with this daemon
   */
  register(name: string, command: CommandBase<CommandParams, CommandResult>): void {
    this.commands.set(name, command);
  }

  /**
   * Initialize commands for this environment
   * Browser: Auto-discovers from manifest + browser-specific commands
   * Server: Auto-discovers from manifest + server-specific commands  
   */
  protected async initialize(): Promise<void> {
    for (const commandEntry of this.commandEntries) {
      try {
        const command = this.createCommand(commandEntry, this.context, commandEntry.name);
        if (command) {
          this.register(commandEntry.name, command);
        }
      } catch (error: unknown) {
        console.error(`❌ Failed to create browser command ${commandEntry.name}:`, error);
      }
    }
    
  }

  protected abstract get commandEntries(): CommandEntry[];

  protected abstract createCommand(entry: CommandEntry, context: JTAGContext, subpath: string): CommandBase<CommandParams, CommandResult> | null;

  /**
   * Execute command - LITERAL COPY of BaseWidget.executeCommand method signature and behavior
   */
  public async executeCommand<T>(command: string, params?: Record<string, any>): Promise<T> {
    try {
      // Wait for JTAG system to be ready  
      await this.waitForSystemReady();
      
      // Get the JTAG client from window
      const jtagClient = (window as any).jtag;
      if (!jtagClient?.commands) {
        throw new Error('JTAG client not available even after system ready event');
      }
      
      // Execute command through the global JTAG system - gets wrapped response
      const wrappedResult = await jtagClient.commands[command](params);
      
      // Extract the actual command result from the wrapped response
      return wrappedResult.commandResult as T;
    } catch (error) {
      console.error(`❌ ${this.toString()}: JTAG operation ${command} failed:`, error);
      throw error;
    }
  }

  /**
   * Commands interface - EXACT copy of BaseWidget.executeCommand pattern
   */
  get commandsInterface(): Record<string, <T>(command: string, params?: Record<string, any>) => Promise<T>> {
    return new Proxy({}, {
      get: (target, commandName: string) => {
        return async <T>(command: string, params?: Record<string, any>): Promise<T> => {
          return await this.executeCommand<T>(command, params);
        };
      }
    });
  }

  /**
   * Handle incoming messages (from MessageSubscriber interface)
   */
  async handleMessage(message: JTAGMessage): Promise<CommandResponse> {
    console.log(`📨 ${this.toString()}: Handling message to ${message.endpoint}`);
    
    // Extract command name from endpoint
    const commandName = this.extractCommand(message.endpoint);
    const command = this.commands.get(commandName);
    
    // Extract session context from incoming payload for response accountability
    const requestPayload = message.payload; // Payload contains context + sessionId
    const requestContext = requestPayload.context ?? this.context;
    
    if (!requestPayload.sessionId) {
      throw new Error(`SECURITY: All commands require valid sessionId. Missing sessionId for command: ${commandName}`);
    }
    const requestSessionId = requestPayload.sessionId;
    
    if (!command) {
      return createCommandErrorResponse(`Command '${commandName}' not found in ${this.context.environment} context`, requestContext, commandName, requestSessionId);
    }

    try {
      // Execute command with session context for dual logging
      const result = await globalSessionContext.withSession(requestSessionId, async () => {
        return await command.execute(message.payload);
      });
      // Wrap raw command results in CommandResponse - maintains session accountability
      return createCommandSuccessResponse(result, requestContext, undefined, requestSessionId);
    } catch (e) {
      const error = e as Error;
      return createCommandErrorResponse(error?.message ?? 'Command execution failed', requestContext, commandName, requestSessionId);
    }
  }

  /**
   * Execute command directly (used by all command invocations)
   */
  async execute(commandName: string, sessionId: UUID, params?: CommandParams): Promise<CommandResult> {
    const command = this.commands.get(commandName);
    if (!command) {
      throw new Error(`Command '${commandName}' not available in ${this.context.environment} context`);
    }

    const fullParams = command.withDefaults(params ?? {}, sessionId, this.context);

    console.log(`⚡ ${this.toString()}: Executing ${commandName} directly`, fullParams);

    return await command.execute(fullParams);
  }

  /**
   * Extract command name from endpoint path
   */
  private extractCommand(endpoint: string): string {
    // endpoint format: "browser/commands/screenshot" or "server/commands/file/save"
    const parts = endpoint.split('/');
    
    // Find the 'commands' segment and extract everything after it
    const commandsIndex = parts.findIndex(part => part === 'commands');
    if (commandsIndex === -1 || commandsIndex === parts.length - 1) {
      throw new Error(`Invalid command endpoint format: ${endpoint}`);
    }
    
    // Return everything after 'commands' joined with '/'
    // e.g., "server/commands/file/save" -> "file/save"
    return parts.slice(commandsIndex + 1).join('/');
  }


  /**
   * Get available commands
   */
  getAvailableCommands(): string[] {
    return Array.from(this.commands.keys());
  }

  /**
   * Get command by name
   */
  getCommand(name: string): CommandBase | undefined {
    return this.commands.get(name);
  }

  /**
   * Wait for window.jtag to be available (copied from BaseWidget)
   */
  private async waitForSystemReady(): Promise<void> {
    return new Promise((resolve) => {
      // Check if system is already ready
      const jtagClient = (window as any).jtag;
      if (jtagClient && jtagClient.commands) {
        resolve();
        return;
      }
      
      // Simple polling - check every 100ms for window.jtag
      const checkReady = () => {
        const jtag = (window as any).jtag;
        if (jtag && jtag.commands) {
          resolve();
        } else {
          setTimeout(checkReady, 100);
        }
      };
      
      checkReady();
    });
  }
}