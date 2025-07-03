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
        const commandInfo = commands.get(command);
        if (!commandInfo) {
          return {
            success: false,
            error: `Command ${command} not found in discovered commands. Available: [${Array.from(commands.keys()).join(', ')}] (${commands.size} total)`,
            processor: 'dynamic-command-discovery'
          };
        }
        
        try {
          // Use dynamic import for tsx-loader
          // @ts-ignore - tsx module resolution issue
          const tsxModule = await import('tsx/cjs/api');
          const unregister = tsxModule.register();
          
          try {
            // Import TypeScript file directly using file:// URL for absolute paths
            const fileUrl = `file://${commandInfo.originalTsPath}`;
            const commandModule = await import(fileUrl);
            
            const CommandClass = commandModule[commandInfo.className];
            
            if (!CommandClass || !CommandClass.execute) {
              return {
                success: false,
                error: `Command ${command} does not have execute method. Module exports: [${Object.keys(commandModule).join(', ')}]`,
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
          } finally {
            unregister();
          }
          
        } catch (error) {
          const errorMessage = error instanceof Error ? error.message : String(error);
          console.error(`❌ [DYNAMIC COMMAND] Command execution failed: ${errorMessage}`);
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
          // Use dynamic import for tsx-loader
          // @ts-ignore - tsx module resolution issue
          const tsxModule = await import('tsx/cjs/api');
          const unregister = tsxModule.register();
          
          try {
            const fileUrl = `file://${commandInfo.originalTsPath}`;
            const commandModule = await import(fileUrl);
            const CommandClass = commandModule[commandInfo.className];
            return CommandClass?.getDefinition ? CommandClass.getDefinition() : null;
          } finally {
            unregister();
          }
        } catch (error) {
          console.warn(`Failed to get definition for ${command}:`, error instanceof Error ? error.message : String(error));
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
          
          // Check if this is a command module - support multiple formats
          const commandNames = [];
          
          // Support different package.json formats
          if (packageJson.continuum?.commandName) {
            commandNames.push(packageJson.continuum.commandName);
          }
          if (packageJson.continuum?.core) {
            commandNames.push(packageJson.continuum.core);
          }
          if (packageJson.continuum?.command) {
            commandNames.push(packageJson.continuum.command);
          }
          if (packageJson.continuum?.commands && Array.isArray(packageJson.continuum.commands)) {
            commandNames.push(...packageJson.continuum.commands);
          }
          
          // Find the TypeScript implementation
          const files = await fs.readdir(dirPath);
          const commandFiles = files.filter(file => 
            file.includes('Command.ts') && !file.includes('.test.ts')
          );
          
          if (commandNames.length > 0 && commandFiles.length > 0) {
            const moduleName = packageJson.name;
            
            // For directories with multiple command files, try to match them to command names
            for (const commandFile of commandFiles) {
              const className = commandFile.replace('.ts', '');
              const tsPath = path.resolve(dirPath, commandFile);
              
              // Try to determine which command name this file implements
              let bestMatchCommandName = commandNames[0]; // default to first command name
              let bestMatchScore = 0;
              
              // Look for better matches based on file name - prioritize exact matches
              for (const cmdName of commandNames) {
                const cmdFileBase = cmdName.replace(/-/g, '').toLowerCase();
                const classFileBase = className.replace(/Command$/, '').toLowerCase();
                let score = 0;
                
                // Direct match: session-create -> SessionCreate (highest priority)
                if (classFileBase === cmdFileBase) {
                  score = 100;
                }
                // Special case: SessionCreateCommand should match session-create
                else if (className.toLowerCase().includes(cmdName.replace(/-/g, '').toLowerCase())) {
                  score = 90;
                }
                // Contains match: SessionCreateCommand -> session-create
                else if (classFileBase.includes(cmdFileBase) || cmdFileBase.includes(classFileBase)) {
                  score = 50;
                }
                
                // Use the best match
                if (score > bestMatchScore) {
                  bestMatchScore = score;
                  bestMatchCommandName = cmdName;
                }
              }
              
              commands.set(bestMatchCommandName, {
                name: bestMatchCommandName,
                className: className,
                module: moduleName,
                directory: dirPath,
                originalTsPath: tsPath,
                // For debugging
                packagePath,
                packageJson
              });
              
              console.log(`  ✅ Found command: ${bestMatchCommandName} -> ${className} at ${tsPath}`);
            }
          }
        } catch (error) {
          console.warn(`⚠️ Failed to process ${dirPath}:`, error);
        }
      }
      
      console.log(`✅ Command discovery complete: ${commands.size} commands found`);
      if (commands.size > 0) {
        console.log('📋 Discovered commands:');
        for (const [name, info] of commands) {
          console.log(`  - ${name}: ${info.className} (${info.directory})`);
        }
      }
      return commands;
      
    } catch (error) {
      console.error('❌ Command discovery failed:', error);
      return new Map();
    }
  }

  /**
   * Recursively find all directories that might contain commands
   */
  private async findCommandDirectories(baseDir: string, maxDepth: number = 3, currentDepth: number = 0): Promise<string[]> {
    const directories: string[] = [];
    const fs = await import('fs/promises');
    const path = await import('path');
    
    // Prevent infinite loops with depth limit
    if (currentDepth >= maxDepth) {
      return directories;
    }
    
    try {
      const entries = await fs.readdir(baseDir, { withFileTypes: true });
      
      for (const entry of entries) {
        if (entry.isDirectory()) {
          // Skip common problematic directories
          if (entry.name.startsWith('.') || 
              entry.name === 'node_modules' || 
              entry.name === 'dist' || 
              entry.name === 'build') {
            continue;
          }
          
          const fullPath = path.join(baseDir, entry.name);
          directories.push(fullPath);
          
          // Recursively search subdirectories with depth tracking
          const subDirs = await this.findCommandDirectories(fullPath, maxDepth, currentDepth + 1);
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
      const result = await this.commandProcessor.executeCommand(command, params, context);
      return result;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ [DAEMON CONNECTOR] Command execution failed: ${errorMessage}`);
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