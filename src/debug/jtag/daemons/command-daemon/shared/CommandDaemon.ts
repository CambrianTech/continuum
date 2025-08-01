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


export abstract class CommandDaemon extends DaemonBase {
  public readonly subpath: string = 'commands';
  public commands: Map<string, CommandBase<CommandParams, CommandResult>> = new Map();

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('command-daemon', context, router);
  }

  /**
   * Register a command with this daemon
   */
  register(name: string, command: CommandBase<CommandParams, CommandResult>): void {
    this.commands.set(name, command);
    console.log(`🎯 ${this.toString()}: Registered command '${name}'`);
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
          console.log(`📦 Registered browser command: ${commandEntry.name} (${commandEntry.className})`);
        }
      } catch (error: unknown) {
        console.error(`❌ Failed to create browser command ${commandEntry.name}:`, error);
      }
    }
    
    console.log(`🎯 ${this.toString()}: Auto-initialized ${this.commands.size} browser commands`);
  }

  protected abstract get commandEntries(): CommandEntry[];

  protected abstract createCommand(entry: CommandEntry, context: JTAGContext, subpath: string): CommandBase<CommandParams, CommandResult> | null;

  /**
   * Commands interface - elegant proxy with proper type extraction
   */
  get commandsInterface(): Record<string, (params?: CommandParams) => Promise<CommandResult>> {
    return new Proxy({}, {
      get: (target, commandName: string) => {
        const command = this.commands.get(commandName);
        if (!command) {
          throw new Error(`Command '${commandName}' not found. Available: ${Array.from(this.commands.keys()).join(', ')}`);
        }
        
        // Assert non-null command for TypeScript
        const validCommand: CommandBase<CommandParams, CommandResult> = command;
        
        // Return elegantly typed async function that executes command directly
        return async (params?: CommandParams) => {
          console.log(`⚡ ${this.toString()}: Executing ${commandName} directly with session context`);
          
          // Only add session context if missing from params
          const sessionId = this.context.uuid as UUID;
          const fullParams = validCommand.withDefaults(params || {}, sessionId, this.context);
          
          // Execute the command directly - it will handle its own routing if needed
          return await validCommand.execute(fullParams);
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
   * Execute command directly (used by JTAGSystem.commands.screenshot())
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
}