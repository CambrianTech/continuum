/**
 * Bootstrap Commands - Root-level commands that have special initialization requirements
 * These commands have different availability based on system initialization state
 */

export interface BootstrapCommand {
  name: string;
  category: 'immediate' | 'post-discovery' | 'always-available';
  description: string;
  canExecute: (systemState: SystemInitializationState) => boolean;
  executor: (params: any, systemState: SystemInitializationState) => Promise<any>;
}

export interface SystemInitializationState {
  modulesDiscovered: boolean;
  commandsLoaded: boolean;
  daemonsReady: boolean;
  discoveredCommands: string[];
  fileSystemReady: boolean;
}

export class BootstrapCommandRegistry {
  private commands = new Map<string, BootstrapCommand>();

  constructor() {
    this.registerBootstrapCommands();
  }

  /**
   * Check if a command can execute in the current system state
   */
  canExecuteCommand(commandName: string, systemState: SystemInitializationState): boolean {
    const command = this.commands.get(commandName);
    if (!command) {
      return false; // Unknown command - defer to post-discovery
    }
    
    return command.canExecute(systemState);
  }

  /**
   * Execute a bootstrap command
   */
  async executeCommand(commandName: string, params: any, systemState: SystemInitializationState): Promise<any> {
    const command = this.commands.get(commandName);
    if (!command) {
      throw new Error(`Bootstrap command not found: ${commandName}`);
    }

    if (!command.canExecute(systemState)) {
      throw new Error(`Command ${commandName} cannot execute in current system state`);
    }

    console.log(`🔧 BOOTSTRAP: Executing ${commandName} (${command.category})`);
    return await command.executor(params, systemState);
  }

  /**
   * Get all available commands in current system state
   */
  getAvailableCommands(systemState: SystemInitializationState): string[] {
    const available: string[] = [];
    
    for (const [name, command] of this.commands) {
      if (command.canExecute(systemState)) {
        available.push(name);
      }
    }
    
    return available;
  }

  /**
   * Register all bootstrap commands
   */
  private registerBootstrapCommands(): void {
    // IMMEDIATE COMMANDS - Available before any discovery
    this.registerCommand({
      name: 'info',
      category: 'immediate',
      description: 'System information (available immediately)',
      canExecute: () => true, // Always available
      executor: async (params) => {
        console.log(`⚡ DAEMON: Executing immediate command: info`);
        return {
          success: true,
          data: {
            version: this.getSystemVersion(),
            system: {
              platform: process.platform,
              nodeVersion: process.version,
              uptime: this.formatUptime(process.uptime())
            },
            server: {
              pid: process.pid,
              workingDirectory: process.cwd(),
              uptime: this.formatUptime(process.uptime())
            },
            timestamp: new Date().toISOString(),
            processedBy: 'bootstrap-command-processor'
          }
        };
      }
    });

    this.registerCommand({
      name: 'status',
      category: 'immediate', 
      description: 'System status (available immediately)',
      canExecute: () => true,
      executor: async (params, systemState) => {
        console.log(`⚡ DAEMON: Executing immediate command: status`);
        return {
          success: true,
          data: {
            systemReady: systemState.modulesDiscovered && systemState.commandsLoaded,
            modulesDiscovered: systemState.modulesDiscovered,
            commandsLoaded: systemState.commandsLoaded,
            daemonsReady: systemState.daemonsReady,
            discoveredCommandsCount: systemState.discoveredCommands.length,
            timestamp: new Date().toISOString(),
            processedBy: 'bootstrap-command-processor'
          }
        };
      }
    });

    // FILE COMMANDS - Always available (core filesystem operations)
    this.registerCommand({
      name: 'filesave',
      category: 'always-available',
      description: 'Save file (always available)',
      canExecute: (systemState) => systemState.fileSystemReady,
      executor: async (params) => {
        console.log(`⚡ DAEMON: Executing file command: filesave`);
        // Mock file save implementation
        return {
          success: true,
          data: {
            filename: params.filename || 'untitled.txt',
            path: '/tmp/' + (params.filename || 'untitled.txt'),
            size: params.content?.length || 0,
            timestamp: new Date().toISOString(),
            processedBy: 'bootstrap-file-processor'
          }
        };
      }
    });

    // POST-DISCOVERY COMMANDS - Need full module discovery
    this.registerCommand({
      name: 'list',
      category: 'post-discovery',
      description: 'List all commands (requires module discovery)',
      canExecute: (systemState) => systemState.modulesDiscovered && systemState.commandsLoaded,
      executor: async (params, systemState) => {
        console.log(`⚡ DAEMON: Executing post-discovery command: list`);
        
        if (!systemState.modulesDiscovered) {
          throw new Error('Cannot list commands: module discovery not complete');
        }

        // Combine bootstrap commands with discovered commands
        const bootstrapCommands = Array.from(this.commands.keys());
        const allCommands = [...new Set([...bootstrapCommands, ...systemState.discoveredCommands])];
        
        return {
          success: true,
          data: {
            commands: allCommands.sort(),
            bootstrapCommands: bootstrapCommands.sort(),
            discoveredCommands: systemState.discoveredCommands.sort(),
            totalCommands: allCommands.length,
            systemReady: true,
            timestamp: new Date().toISOString(),
            processedBy: 'bootstrap-command-processor'
          }
        };
      }
    });

    this.registerCommand({
      name: 'help',
      category: 'post-discovery',
      description: 'Command help (requires module discovery)',
      canExecute: (systemState) => systemState.modulesDiscovered,
      executor: async (params, systemState) => {
        console.log(`⚡ DAEMON: Executing post-discovery command: help`);
        
        const availableCommands = this.getAvailableCommands(systemState);
        const targetCommand = params.command;
        
        if (targetCommand) {
          // Help for specific command
          if (!availableCommands.includes(targetCommand)) {
            return {
              success: false,
              error: `Command not found: ${targetCommand}`
            };
          }
          
          const command = this.commands.get(targetCommand);
          return {
            success: true,
            data: {
              command: targetCommand,
              description: command?.description || 'No description available',
              category: command?.category || 'unknown',
              available: true,
              timestamp: new Date().toISOString(),
              processedBy: 'bootstrap-command-processor'
            }
          };
        } else {
          // General help
          return {
            success: true,
            data: {
              availableCommands: availableCommands.sort(),
              systemState: {
                modulesDiscovered: systemState.modulesDiscovered,
                commandsLoaded: systemState.commandsLoaded,
                totalCommands: availableCommands.length
              },
              usage: 'Use "help <command>" for specific command help',
              timestamp: new Date().toISOString(),
              processedBy: 'bootstrap-command-processor'
            }
          };
        }
      }
    });
  }

  private registerCommand(command: BootstrapCommand): void {
    this.commands.set(command.name, command);
    console.log(`📦 Registered bootstrap command: ${command.name} (${command.category})`);
  }

  private getSystemVersion(): string {
    try {
      const packageJson = require('../../package.json');
      return packageJson.version || '0.0.0';
    } catch {
      return '0.0.0';
    }
  }

  private formatUptime(seconds: number): string {
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    return `${hours}h ${minutes}m`;
  }
}

export default BootstrapCommandRegistry;