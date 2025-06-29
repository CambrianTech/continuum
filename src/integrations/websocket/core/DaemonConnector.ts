/**
 * Daemon Connector - Simple connection to existing TypeScript command system
 */

import { EventEmitter } from 'events';
import { DaemonConnection, CommandResult, DaemonConfig } from '../types';

export class DaemonConnector extends EventEmitter {
  private connection: DaemonConnection;
  private _config: Required<DaemonConfig>;
  private commandProcessor: any = null;

  constructor(config: DaemonConfig = {}) {
    super();
    
    this._config = {
      autoConnect: config.autoConnect ?? true,
      enableFallback: config.enableFallback ?? false,
      retryAttempts: config.retryAttempts ?? 3,
      retryInterval: config.retryInterval ?? 5000
    };

    this.connection = {
      connected: false,
      connectionAttempts: 0
    };
  }

  async connect(): Promise<boolean> {
    console.log('🔌 Connecting to TypeScript command system...');
    
    try {
      // Import the existing SelfTestCommand directly (proven working)
      const { SelfTestCommand } = await import('../../../commands/development/selftest/SelfTestCommand');
      
      // Create a simple command processor that uses existing TypeScript commands
      this.commandProcessor = {
        initialized: true,
        executeCommand: async (command: string, params: any, context: any): Promise<CommandResult> => {
          console.log(`🚀 Routing ${command} to TypeScript command system`);
          
          switch (command) {
            case 'selftest':
              const result = await SelfTestCommand.execute(params, context);
              return {
                success: result.success,
                processor: 'typescript-command-system',
                message: result.message,
                ...(result.data !== undefined && { data: result.data }),
                ...(result.error !== undefined && { error: result.error })
              };
              
            default:
              return {
                success: false,
                error: `Command ${command} not found in TypeScript system`,
                processor: 'typescript-command-system'
              };
          }
        },
        getCommands: () => ['selftest'], // Expandable as we add more TypeScript commands
        getDefinition: (command: string) => {
          if (command === 'selftest') {
            return SelfTestCommand.getDefinition();
          }
          return null;
        }
      };

      this.connection = {
        connected: true,
        commandProcessor: this.commandProcessor,
        lastConnectAttempt: new Date(),
        connectionAttempts: this.connection.connectionAttempts + 1
      };

      console.log('✅ Connected to TypeScript command system');
      this.emit('connected');
      return true;

    } catch (error) {
      console.error('❌ Failed to connect to TypeScript command system:', error);
      this.connection = {
        ...this.connection,
        connectionAttempts: this.connection.connectionAttempts + 1
      };
      this.emit('error', error);
      return false;
    }
  }

  async disconnect(): Promise<void> {
    if (this.connection.connected) {
      this.connection = {
        ...this.connection,
        connected: false
      };
      this.commandProcessor = null;
      console.log('🔌 Disconnected from TypeScript command system');
      this.emit('disconnected');
    }
  }

  isConnected(): boolean {
    return this.connection.connected;
  }

  async executeCommand(command: string, params: any, context: any): Promise<CommandResult> {
    if (!this.connection.connected || !this.commandProcessor) {
      return {
        success: false,
        error: 'Not connected to TypeScript command system',
        processor: 'daemon-connector-disconnected'
      };
    }

    try {
      return await this.commandProcessor.executeCommand(command, params, context);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage,
        processor: 'daemon-connector-error'
      };
    }
  }

  getAvailableCommands(): string[] {
    if (!this.connection.connected || !this.commandProcessor) {
      return [];
    }
    
    return this.commandProcessor.getCommands();
  }

  getCommandDefinition(command: string) {
    if (!this.connection.connected || !this.commandProcessor) {
      return null;
    }
    
    return this.commandProcessor.getDefinition(command);
  }
}