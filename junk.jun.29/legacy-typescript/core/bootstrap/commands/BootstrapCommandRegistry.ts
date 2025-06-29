/**
 * Bootstrap Command Registry
 * Manages root-level commands with initialization dependencies
 */

import { SystemInitializationState } from '../initialization/SystemInitializer.js';

export interface BootstrapCommand {
  name: string;
  category: 'immediate' | 'post-discovery' | 'always-available';
  description: string;
  canExecute: (systemState: SystemInitializationState) => boolean;
  executor: (params: any, systemState: SystemInitializationState) => Promise<any>;
}

export class BootstrapCommandRegistry {
  private commands = new Map<string, BootstrapCommand>();

  constructor() {
    console.log('📦 CORE: Initializing Bootstrap Command Registry...');
    this.registerCoreCommands();
  }

  canExecuteCommand(commandName: string, systemState: SystemInitializationState): boolean {
    const command = this.commands.get(commandName);
    if (!command) {
      return false;
    }
    return command.canExecute(systemState);
  }

  async executeCommand(commandName: string, params: any, systemState: SystemInitializationState): Promise<any> {
    const command = this.commands.get(commandName);
    if (!command) {
      throw new Error(`Bootstrap command not found: ${commandName}`);
    }

    if (!command.canExecute(systemState)) {
      throw new Error(`Command ${commandName} cannot execute in current system state`);
    }

    console.log(`🔧 DAEMON: Executing ${commandName} (${command.category})`);
    const result = await command.executor(params, systemState);
    console.log(`✅ DAEMON: Completed ${commandName}`);
    
    return result;
  }

  getAvailableCommands(systemState: SystemInitializationState): string[] {
    const available: string[] = [];
    
    this.commands.forEach((command, name) => {
      if (command.canExecute(systemState)) {
        available.push(name);
      }
    });
    
    return available;
  }

  private registerCoreCommands(): void {
    // IMMEDIATE: Available before discovery
    this.register({
      name: 'info',
      category: 'immediate',
      description: 'System information (available immediately)',
      canExecute: () => true,
      executor: async (params) => ({
        success: true,
        data: {
          version: this.getVersion(),
          system: this.getSystemInfo(),
          server: this.getServerInfo(),
          timestamp: new Date().toISOString(),
          processedBy: 'bootstrap-command-processor'
        }
      })
    });

    this.register({
      name: 'status', 
      category: 'immediate',
      description: 'System status (available immediately)',
      canExecute: () => true,
      executor: async (params, systemState) => ({
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
      })
    });

    // POST-DISCOVERY: Need module discovery complete
    this.register({
      name: 'list',
      category: 'post-discovery', 
      description: 'List all commands (requires module discovery)',
      canExecute: (systemState) => systemState.modulesDiscovered && systemState.commandsLoaded,
      executor: async (params, systemState) => {
        const bootstrapCommands = Array.from(this.commands.keys());
        const combinedCommands = bootstrapCommands.concat(systemState.discoveredCommands);
        const allCommands = combinedCommands.filter((command, index) => combinedCommands.indexOf(command) === index);
        
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

    this.register({
      name: 'help',
      category: 'post-discovery',
      description: 'Command help (requires module discovery)', 
      canExecute: (systemState) => systemState.modulesDiscovered,
      executor: async (params, systemState) => {
        console.debug('🔗 BOOTSTRAP: help command calling list command for command inventory');
        
        // Call list command to get comprehensive command data
        const listCommand = this.commands.get('list');
        if (!listCommand) {
          throw new Error('List command not available for help system');
        }
        
        const listResult = await listCommand.executor({}, systemState);
        const availableCommands = listResult.data.commands;
        const targetCommand = params.command;
        
        if (targetCommand) {
          const command = this.commands.get(targetCommand);
          console.debug(`🔍 BOOTSTRAP: help providing details for specific command: ${targetCommand}`);
          
          return {
            success: true,
            data: {
              command: targetCommand,
              description: command?.description || 'No description available',
              category: command?.category || 'unknown',
              available: availableCommands.includes(targetCommand),
              commandSource: listResult.data.bootstrapCommands.includes(targetCommand) ? 'bootstrap' : 'discovered',
              timestamp: new Date().toISOString(),
              processedBy: 'bootstrap-command-processor'
            }
          };
        } else {
          console.debug(`📋 BOOTSTRAP: help providing general command listing (${availableCommands.length} commands)`);
          
          return {
            success: true,
            data: {
              availableCommands: availableCommands.sort(),
              commandBreakdown: {
                bootstrap: listResult.data.bootstrapCommands,
                discovered: listResult.data.discoveredCommands,
                total: listResult.data.totalCommands
              },
              systemState: {
                modulesDiscovered: systemState.modulesDiscovered,
                commandsLoaded: systemState.commandsLoaded,
                systemReady: listResult.data.systemReady
              },
              usage: 'Use "help <command>" for specific command help',
              timestamp: new Date().toISOString(),
              processedBy: 'bootstrap-command-processor',
              basedOnListResult: true
            }
          };
        }
      }
    });

    // ALWAYS AVAILABLE: Core file operations
    this.register({
      name: 'filesave',
      category: 'always-available',
      description: 'Save file (always available)',
      canExecute: (systemState) => systemState.fileSystemReady,
      executor: async (params) => ({
        success: true,
        data: {
          filename: params.filename || 'untitled.txt',
          path: '/tmp/' + (params.filename || 'untitled.txt'),
          size: params.content?.length || 0,
          timestamp: new Date().toISOString(),
          processedBy: 'bootstrap-file-processor'
        }
      })
    });
  }

  private register(command: BootstrapCommand): void {
    this.commands.set(command.name, command);
    console.log(`📦 CORE: Registered bootstrap command: ${command.name} (${command.category})`);
  }

  private getVersion(): string {
    try {
      const packageJson = require('../../../package.json');
      return packageJson.version || '0.0.0';
    } catch {
      return '0.0.0';
    }
  }

  private getSystemInfo(): any {
    return {
      platform: process.platform,
      nodeVersion: process.version,
      uptime: this.formatUptime(process.uptime()),
      architecture: process.arch
    };
  }

  private getServerInfo(): any {
    return {
      pid: process.pid,
      workingDirectory: process.cwd(),
      uptime: this.formatUptime(process.uptime())
    };
  }

  private formatUptime(seconds: number): string {
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    return `${hours}h ${minutes}m`;
  }
}