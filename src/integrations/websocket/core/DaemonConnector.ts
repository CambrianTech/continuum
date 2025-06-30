/**
 * Daemon Connector - Simple connection to existing TypeScript command system
 */

import { EventEmitter } from 'events';
import { DaemonConnection, CommandResult, DaemonConfig } from '../types';

export class DaemonConnector extends EventEmitter {
  private connection: DaemonConnection;
  private commandProcessor: any = null;

  constructor(_config: DaemonConfig = {}) {
    super();

    this.connection = {
      connected: false,
      connectionAttempts: 0
    };
  }

  async connect(): Promise<boolean> {
    console.log('🔌 Connecting to Command Processor daemon via IPC...');
    
    try {
      // TODO: Replace with actual IPC to running Command Processor daemon
      // For now, implement dynamic command discovery from filesystem
      const commandProcessor = await this.createDynamicCommandProcessor();
      
      this.commandProcessor = commandProcessor;

      this.connection = {
        connected: true,
        commandProcessor: this.commandProcessor,
        lastConnectAttempt: new Date(),
        connectionAttempts: this.connection.connectionAttempts + 1
      };

      console.log('✅ Connected to Command Processor with dynamic command discovery');
      this.emit('connected');
      return true;

    } catch (error) {
      console.error('❌ Failed to connect to Command Processor daemon:', error);
      this.connection = {
        ...this.connection,
        connectionAttempts: this.connection.connectionAttempts + 1
      };
      this.emit('error', error);
      return false;
    }
  }

  /**
   * Create dynamic command processor that discovers commands from filesystem
   * TODO: Replace with actual IPC to running Command Processor daemon
   */
  private async createDynamicCommandProcessor() {
    const commands = await this.discoverCommands();
    
    return {
      initialized: true,
      executeCommand: async (command: string, params: any, context: any): Promise<CommandResult> => {
        console.log(`🚀 Routing ${command} to discovered command system`);
        
        const commandInfo = commands.get(command);
        if (!commandInfo) {
          return {
            success: false,
            error: `Command ${command} not found in discovered commands`,
            processor: 'dynamic-command-discovery'
          };
        }
        
        try {
          // Dynamic import of the command
          const commandModule = await import(commandInfo.path);
          const CommandClass = commandModule[commandInfo.className];
          
          if (!CommandClass || !CommandClass.execute) {
            return {
              success: false,
              error: `Command ${command} does not have execute method`,
              processor: 'dynamic-command-discovery'
            };
          }
          
          const result = await CommandClass.execute(params, context);
          return {
            success: result.success,
            processor: 'dynamic-command-discovery',
            ...(result.data !== undefined && { data: result.data }),
            ...(result.error !== undefined && { error: result.error })
          };
          
        } catch (error) {
          const errorMessage = error instanceof Error ? error.message : String(error);
          return {
            success: false,
            error: `Failed to execute ${command}: ${errorMessage}`,
            processor: 'dynamic-command-discovery'
          };
        }
      },
      getCommands: () => Array.from(commands.keys()),
      getDefinition: async (command: string) => {
        const commandInfo = commands.get(command);
        if (!commandInfo) return null;
        
        try {
          const commandModule = await import(commandInfo.path);
          const CommandClass = commandModule[commandInfo.className];
          return CommandClass.getDefinition ? CommandClass.getDefinition() : null;
        } catch {
          return null;
        }
      }
    };
  }

  /**
   * Discover commands from filesystem using package.json discovery
   * This mirrors how the real Command Processor daemon should work
   */
  private async discoverCommands(): Promise<Map<string, any>> {
    const commands = new Map();
    
    try {
      const fs = await import('fs/promises');
      const path = await import('path');
      
      // Recursively find all package.json files in src/commands
      const commandDirs = await this.findCommandDirectories('src/commands');
      
      for (const dirPath of commandDirs) {
        try {
          const packagePath = path.join(dirPath, 'package.json');
          
          // Check if package.json exists
          try {
            await fs.access(packagePath);
          } catch {
            continue; // Skip if no package.json
          }
          
          // Read package.json to get command info
          const packageContent = await fs.readFile(packagePath, 'utf-8');
          const packageJson = JSON.parse(packageContent);
          
          // Check if this is a command module
          const commandName = packageJson.continuum?.commandName || packageJson.continuum?.core;
          if (commandName) {
            const moduleName = packageJson.name;
            
            // Find the TypeScript implementation
            const files = await fs.readdir(dirPath);
            const commandFile = files.find(file => 
              file.includes('Command.ts') && !file.includes('.test.ts')
            );
            
            if (commandFile) {
              const commandPath = path.resolve(dirPath, commandFile);
              const className = commandFile.replace('.ts', '');
              
              commands.set(commandName, {
                name: commandName,
                path: commandPath,
                className: className,
                module: moduleName,
                directory: dirPath
              });
              
              console.log(`📋 Discovered command: ${commandName} → ${className}`);
            }
          }
        } catch (error) {
          console.warn(`⚠️ Failed to process ${dirPath}:`, error);
        }
      }
      
      console.log(`✅ Command discovery complete: ${commands.size} commands found`);
      return commands;
      
    } catch (error) {
      console.error('❌ Command discovery failed:', error);
      return new Map();
    }
  }

  /**
   * Recursively find all directories that might contain commands
   */
  private async findCommandDirectories(baseDir: string): Promise<string[]> {
    const directories: string[] = [];
    const fs = await import('fs/promises');
    const path = await import('path');
    
    try {
      const entries = await fs.readdir(baseDir, { withFileTypes: true });
      
      for (const entry of entries) {
        if (entry.isDirectory()) {
          const fullPath = path.join(baseDir, entry.name);
          directories.push(fullPath);
          
          // Recursively search subdirectories
          const subDirs = await this.findCommandDirectories(fullPath);
          directories.push(...subDirs);
        }
      }
    } catch (error) {
      // Directory doesn't exist or can't be read
    }
    
    return directories;
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