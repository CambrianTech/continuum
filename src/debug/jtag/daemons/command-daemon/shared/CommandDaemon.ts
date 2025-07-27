/**
 * Command Daemon Base - Shared Logic
 * 
 * Base class for CommandDaemon implementations.
 * Contains shared logic while allowing browser/server specific implementations.
 */

import { DaemonBase } from '../../../shared/DaemonBase';
import type { JTAGContext, JTAGMessage, CommandParams, CommandResult } from '../../../shared/JTAGTypes';
import { JTAGMessageFactory } from '../../../shared/JTAGTypes';
import type { JTAGRouter } from '../../../shared/JTAGRouter';
import type{ CommandBase, CommandEntry } from './CommandBase';
import type { CommandResponse } from './CommandResponseTypes';
import { createCommandErrorResponse, createCommandSuccessResponse } from './CommandResponseTypes';
import { type UUID } from '../../../shared/CrossPlatformUUID';


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
          const fullParams = validCommand.withDefaults(params || {}, sessionId);
          
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
    const requestPayload = message.payload as any; // Payload contains context + sessionId
    const requestContext = requestPayload.context || this.context;
    
    if (!requestPayload.sessionId) {
      throw new Error(`SECURITY: All commands require valid sessionId. Missing sessionId for command: ${commandName}`);
    }
    const requestSessionId = requestPayload.sessionId;
    
    if (!command) {
      return createCommandErrorResponse(`Command '${commandName}' not found in ${this.context.environment} context`, requestContext, commandName, requestSessionId);
    }

    try {
      const result = await command.execute(message.payload);
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
  async execute(commandName: string, params?: CommandParams): Promise<CommandResult> {
    const command = this.commands.get(commandName);
    if (!command) {
      throw new Error(`Command '${commandName}' not available in ${this.context.environment} context`);
    }

    console.log(`⚡ ${this.toString()}: Executing ${commandName} directly`);
    // SECURITY: Direct command execution requires explicit session context
    throw new Error(`SECURITY: Direct command execution requires session context. Use message-based routing instead for command: ${commandName}`);
  }

  /**
   * Extract command name from endpoint path
   */
  private extractCommand(endpoint: string): string {
    // endpoint format: "browser/commands/screenshot" or "commands/screenshot"
    const parts = endpoint.split('/');
    return parts[parts.length - 1]; // Get last part (command name)
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