/**
 * Command Daemon Base - Shared Logic
 * 
 * Base class for CommandDaemon implementations.
 * Contains shared logic while allowing browser/server specific implementations.
 */

import { DaemonBase } from '../../../shared/DaemonBase';
import { JTAGContext, JTAGMessage, CommandParams } from '../../../shared/JTAGTypes';
import { JTAGRouter } from '../../../shared/JTAGRouter';
import { CommandBase } from './CommandBase';
import { CommandResponse, CommandErrorResponse, CommandSuccessResponse } from '../../../shared/ResponseTypes';

export abstract class CommandDaemonBase extends DaemonBase {
  public readonly subpath: string = 'commands';
  protected commands: Map<string, CommandBase> = new Map<string, CommandBase>();

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('command-daemon', context, router);
  }

  /**
   * Initialize daemon-specific functionality (from DaemonBase)
   */
  protected async initialize(): Promise<void> {
    // Initialize commands for this environment
    await this.initializeCommands();
    console.log(`🎯 ${this.toString()}: Command daemon initialized with ${this.commands.size} commands`);
  }

  /**
   * Register a command with this daemon
   */
  register(name: string, command: CommandBase): void {
    this.commands.set(name, command);
    console.log(`🎯 ${this.toString()}: Registered command '${name}'`);
  }

  /**
   * Initialize commands for this environment
   * Browser: Auto-discovers from manifest + browser-specific commands
   * Server: Auto-discovers from manifest + server-specific commands  
   */
  protected abstract initializeCommands(): Promise<void>;


  /**
   * Handle incoming messages (from MessageSubscriber interface)
   */
  async handleMessage(message: JTAGMessage): Promise<CommandResponse> {
    console.log(`📨 ${this.toString()}: Handling message to ${message.endpoint}`);
    
    // Extract command name from endpoint
    const commandName = this.extractCommand(message.endpoint);
    const command = this.commands.get(commandName);
    
    if (!command) {
      return new CommandErrorResponse(`Command '${commandName}' not found in ${this.context.environment} context`, commandName);
    }

    try {
      const result = await command.execute(message.payload);
      // Wrap raw command results in CommandResponse
      return new CommandSuccessResponse(result);
    } catch (error: any) {
      return new CommandErrorResponse(error.message || 'Command execution failed', commandName);
    }
  }

  /**
   * Execute command directly (used by JTAGSystem.commands.screenshot())
   */
  async execute(commandName: string, params: CommandParams): Promise<any> {
    const command = this.commands.get(commandName);
    if (!command) {
      throw new Error(`Command '${commandName}' not available in ${this.context.environment} context`);
    }

    console.log(`⚡ ${this.toString()}: Executing ${commandName} directly`);
    return await command.execute(params);
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