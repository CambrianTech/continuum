/**
 * JTAG Command Processor Daemon
 * 
 * Symmetric daemon that handles command processing on both client and server.
 * Registers as 'command' endpoint, getting /client/command and /server/command routes.
 */

import { BaseDaemon, DaemonMessage, DaemonResponse } from './shared/MessageSubscriber';
import { JTAGRouter } from './shared/JTAGRouter';

export interface CommandMessage {
  command: string;
  parameters: any;
  context: 'client' | 'server';
  executionId?: string;
}

export interface CommandResult {
  command: string;
  result: any;
  success: boolean;
  error?: string;
  executionTime: number;
  context: 'client' | 'server';
}

/**
 * Universal Command Processor - works in both client and server contexts
 */
export class CommandProcessorDaemon extends BaseDaemon<CommandMessage> {
  private context: 'client' | 'server';
  private commandRegistry = new Map<string, (params: any) => Promise<any>>();

  constructor(context: 'client' | 'server' = 'server') {
    super('command'); // Base endpoint is 'command'
    this.context = context;
    this.initializeCommands();
  }

  async handleMessage(message: DaemonMessage<CommandMessage>): Promise<DaemonResponse> {
    const { command, parameters, executionId } = message.payload;
    const startTime = Date.now();

    try {
      console.log(`⚡ CommandProcessor[${this.context}]: Executing '${command}'`);
      
      // Check if we have a handler for this command
      if (!this.commandRegistry.has(command)) {
        return this.createResponse(false, null, `Command '${command}' not found in ${this.context} context`);
      }

      // Execute the command
      const handler = this.commandRegistry.get(command)!;
      const result = await handler(parameters);
      
      const executionTime = Date.now() - startTime;
      
      const commandResult: CommandResult = {
        command,
        result,
        success: true,
        executionTime,
        context: this.context
      };

      console.log(`✅ CommandProcessor[${this.context}]: '${command}' completed in ${executionTime}ms`);
      
      return this.createResponse(true, commandResult);

    } catch (error: any) {
      const executionTime = Date.now() - startTime;
      
      const commandResult: CommandResult = {
        command,
        result: null,
        success: false,
        error: error.message,
        executionTime,
        context: this.context
      };

      console.error(`❌ CommandProcessor[${this.context}]: '${command}' failed:`, error.message);
      
      return this.createResponse(false, commandResult, error.message);
    }
  }

  /**
   * Register a command handler
   */
  registerCommand(command: string, handler: (params: any) => Promise<any>): void {
    this.commandRegistry.set(command, handler);
    console.log(`📝 CommandProcessor[${this.context}]: Registered '${command}'`);
  }

  /**
   * Initialize context-specific commands
   */
  private initializeCommands(): void {
    if (this.context === 'server') {
      this.initializeServerCommands();
    } else {
      this.initializeClientCommands();
    }
  }

  private initializeServerCommands(): void {
    // Server-specific commands
    this.registerCommand('screenshot', async (params) => {
      // Server screenshot logic would go here
      return {
        filename: params.filename || `screenshot-${Date.now()}.png`,
        path: '.continuum/jtag/screenshots/',
        success: true
      };
    });

    this.registerCommand('exec', async (params) => {
      // Server JavaScript execution
      const { exec } = require('child_process');
      return new Promise((resolve, reject) => {
        exec(params.code, (error: any, stdout: string, stderr: string) => {
          if (error) {
            reject(error);
          } else {
            resolve({ stdout, stderr });
          }
        });
      });
    });

    this.registerCommand('log', async (params) => {
      // Server logging
      console.log(`📝 Server Log [${params.component}]:`, params.message);
      return { logged: true, timestamp: new Date().toISOString() };
    });
  }

  private initializeClientCommands(): void {
    // Client-specific commands
    this.registerCommand('screenshot', async (params) => {
      // Browser screenshot using html2canvas or similar
      return {
        dataUrl: 'data:image/png;base64,fake-screenshot-data',
        width: window.innerWidth,
        height: window.innerHeight,
        timestamp: new Date().toISOString()
      };
    });

    this.registerCommand('exec', async (params) => {
      // Browser JavaScript execution
      try {
        const result = eval(params.code);
        return {
          result,
          context: 'browser',
          timestamp: new Date().toISOString()
        };
      } catch (error: any) {
        throw new Error(`Browser exec failed: ${error.message}`);
      }
    });

    this.registerCommand('log', async (params) => {
      // Browser logging
      console.log(`📝 Browser Log [${params.component}]:`, params.message);
      
      // Could also send to remote logging service
      return { 
        logged: true, 
        timestamp: new Date().toISOString(),
        context: 'browser'
      };
    });

    this.registerCommand('dom-query', async (params) => {
      // Browser-only DOM operations
      const element = document.querySelector(params.selector);
      return {
        found: !!element,
        tagName: element?.tagName,
        textContent: element?.textContent,
        attributes: element ? Array.from(element.attributes).map((attr: any) => ({ 
          name: attr.name, 
          value: attr.value 
        })) : []
      };
    });
  }

  /**
   * Get available commands for this context
   */
  getAvailableCommands(): string[] {
    return Array.from(this.commandRegistry.keys());
  }
}

// Export factory functions for different contexts
export function createServerCommandProcessor(): CommandProcessorDaemon {
  return new CommandProcessorDaemon('server');
}

export function createClientCommandProcessor(): CommandProcessorDaemon {
  return new CommandProcessorDaemon('client');
}