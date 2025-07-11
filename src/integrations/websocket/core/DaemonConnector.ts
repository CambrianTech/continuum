/**
 * Daemon Connector - Universal command execution via UniversalCommandRegistry
 */

import { EventEmitter } from 'events';
import { DaemonConnection, DaemonConfig } from '../types';
import { CommandResult } from '../../../types/shared/CommandTypes';
import { UniversalCommandRegistry, getGlobalCommandRegistry } from '../../../services/UniversalCommandRegistry';

export class DaemonConnector extends EventEmitter {
  private connection: DaemonConnection;
  private commandRegistry: UniversalCommandRegistry;

  constructor(_config: DaemonConfig = {}) {
    super();

    this.connection = {
      connected: false,
      connectionAttempts: 0
    };
    
    // Use the global command registry for universal command access
    this.commandRegistry = getGlobalCommandRegistry();
  }
  
  async connect(): Promise<boolean> {
    console.log('🔌 Connecting to Universal Command Registry...');
    
    try {
      // Initialize the command registry
      await this.commandRegistry.initialize();
      
      // Cache available commands for synchronous access
      this.cachedCommands = await this.commandRegistry.getAvailableCommands();

      this.connection = {
        connected: true,
        lastConnectAttempt: new Date(),
        connectionAttempts: this.connection.connectionAttempts + 1
      };

      console.log(`✅ Connected to Universal Command Registry with ${this.cachedCommands.length} commands discovered`);
      this.emit('connected');
      return true;

    } catch (error) {
      console.error('❌ Failed to connect to Universal Command Registry:', error);
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
      console.log('🔌 Disconnected from Universal Command Registry');
      this.emit('disconnected');
    }
  }

  isConnected(): boolean {
    return this.connection.connected;
  }

  async executeCommand(command: string, params: any, context: any): Promise<CommandResult> {
    if (!this.connection.connected) {
      return {
        success: false,
        error: 'Not connected to Universal Command Registry'
      };
    }

    try {
      // Use the UniversalCommandRegistry for command execution
      const result = await this.commandRegistry.executeCommand(command, params, context, {
        validateParameters: true,
        timeout: 30000
      });
      
      return result; // Return the result directly from the registry
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ [DAEMON CONNECTOR] Command execution failed: ${errorMessage}`);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  getAvailableCommands(): string[] {
    if (!this.connection.connected) {
      return [];
    }
    
    // Synchronous access to cached commands
    // For immediate access, we'll need to populate this during connect()
    return this.cachedCommands || [];
  }

  async getAvailableCommandsAsync(): Promise<string[]> {
    if (!this.connection.connected) {
      return [];
    }
    
    return await this.commandRegistry.getAvailableCommands();
  }

  async getCommandDefinition(command: string) {
    if (!this.connection.connected) {
      return null;
    }
    
    return await this.commandRegistry.getCommandDefinition(command);
  }
  
  private cachedCommands: string[] = [];
}