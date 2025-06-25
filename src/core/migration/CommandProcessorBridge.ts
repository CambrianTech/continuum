/**
 * Command Processor Bridge - Migration interface between legacy routing and modern daemon
 * 
 * PROBLEM: Current routing sends "migration {...}" to PlannerAI instead of MigrationCommand
 * SOLUTION: Bridge that intercepts and properly routes commands during migration period
 * 
 * PRESERVES: All existing Lambda architecture and mesh capabilities
 * ADDS: TypeScript daemon routing with backward compatibility
 */

import { CommandProcessorDaemon, CommandRequest, ExecutionContext } from '../../daemons/command-processor/CommandProcessorDaemon.js';

export interface RouteDecision {
  useModern: boolean;
  reason: string;
  confidence: number;
}

export interface LegacyCommand {
  name: string;
  params: string;
  encoding?: string;
  source: string;
}

/**
 * Bridge for gradual migration from legacy command routing to modern daemon
 */
export class CommandProcessorBridge {
  private modernDaemon: CommandProcessorDaemon;
  private legacyFallback: any; // Legacy CommandProcessor.cjs
  private migrationEnabled = false;
  private migrationPercentage = 0; // 0-100% traffic to modern system
  
  // Command-specific migration status
  private commandMigrationStatus = new Map<string, boolean>();

  constructor(modernDaemon: CommandProcessorDaemon, legacyFallback: any) {
    this.modernDaemon = modernDaemon;
    this.legacyFallback = legacyFallback;
    this.initializeMigrationStatus();
  }

  /**
   * Main routing decision: should this command use modern or legacy system?
   */
  async routeCommand(task: string, source: string = 'unknown'): Promise<any> {
    console.log(`🌉 BRIDGE: Routing command task: "${task.substring(0, 100)}..."`);
    
    const command = this.parseCommandFromTask(task);
    if (!command) {
      console.log(`🌉 BRIDGE: No command detected, routing to legacy AI system`);
      return await this.routeToLegacy(task, source);
    }

    const decision = await this.makeRoutingDecision(command, source);
    console.log(`🌉 BRIDGE: Routing decision for ${command.name}: ${decision.useModern ? 'MODERN' : 'LEGACY'} (${decision.reason})`);

    if (decision.useModern) {
      return await this.routeToModern(command);
    } else {
      return await this.routeToLegacy(task, source);
    }
  }

  /**
   * Parse command from task string (handles various formats)
   */
  private parseCommandFromTask(task: string): LegacyCommand | null {
    const taskTrimmed = task.trim();

    // Handle [CMD:NAME] format
    const cmdMatch = taskTrimmed.match(/^\[CMD:(\w+)\]\s*(.*)/);
    if (cmdMatch) {
      return {
        name: cmdMatch[1],
        params: cmdMatch[2] || '{}',
        source: 'cmd-format'
      };
    }

    // Handle direct command format: "commandName {...}"
    const directMatch = taskTrimmed.match(/^(\w+)\s*(.*)/);
    if (directMatch) {
      const commandName = directMatch[1].toUpperCase();
      
      // Check if this looks like a command (common command names)
      const knownCommands = [
        'MIGRATION', 'SCREENSHOT', 'HELP', 'AGENTS', 'WORKSPACE', 
        'BROWSER_JS', 'BROWSERJS', 'EXEC', 'FILE_READ', 'FILE_WRITE',
        'DIAGNOSTICS', 'RESTART', 'STATUS', 'LIST', 'TEST'
      ];
      
      if (knownCommands.includes(commandName) || this.isRegisteredCommand(commandName)) {
        return {
          name: commandName,
          params: directMatch[2] || '{}',
          source: 'direct-format'
        };
      }
    }

    return null;
  }

  /**
   * Check if command name is registered in either system
   */
  private isRegisteredCommand(commandName: string): boolean {
    // Check legacy system
    if (this.legacyFallback && this.legacyFallback.commandRegistry) {
      const legacyCommand = this.legacyFallback.commandRegistry.getCommand(commandName);
      if (legacyCommand) return true;
    }

    if (this.legacyFallback && this.legacyFallback.commands) {
      if (this.legacyFallback.commands.has(commandName)) return true;
    }

    // Check modern system (when available)
    // Implementation would check modern daemon command registry

    return false;
  }

  /**
   * Make routing decision based on migration strategy
   */
  private async makeRoutingDecision(command: LegacyCommand, source: string): Promise<RouteDecision> {
    // If migration is disabled, always use legacy
    if (!this.migrationEnabled) {
      return {
        useModern: false,
        reason: 'Migration disabled',
        confidence: 1.0
      };
    }

    // Check command-specific migration status
    const commandMigrated = this.commandMigrationStatus.get(command.name) || false;
    if (!commandMigrated) {
      return {
        useModern: false,
        reason: `Command ${command.name} not yet migrated`,
        confidence: 1.0
      };
    }

    // Traffic splitting for migrated commands
    const random = Math.random() * 100;
    if (random < this.migrationPercentage) {
      return {
        useModern: true,
        reason: `Traffic splitting: ${this.migrationPercentage}% to modern`,
        confidence: 0.8
      };
    } else {
      return {
        useModern: false,
        reason: `Traffic splitting: ${100 - this.migrationPercentage}% to legacy`,
        confidence: 0.8
      };
    }
  }

  /**
   * Route to modern TypeScript daemon
   */
  private async routeToModern(command: LegacyCommand): Promise<any> {
    console.log(`🚀 BRIDGE: Routing to modern daemon: ${command.name}`);
    
    try {
      // Convert legacy command to modern request format
      const request: CommandRequest = {
        command: command.name,
        params: this.parseParams(command.params),
        encoding: command.encoding || 'utf-8',
        context: this.createExecutionContext(command.source)
      };

      // Execute via modern daemon
      const response = await this.modernDaemon.handleMessage({
        id: `bridge-${Date.now()}`,
        from: 'command-processor-bridge',
        to: 'command-processor',
        type: 'execute-command',
        data: request,
        timestamp: new Date()
      });

      if (response.success) {
        console.log(`✅ BRIDGE: Modern execution successful for ${command.name}`);
        return {
          result: response.data,
          role: 'BusCommand',
          type: 'modern_command_execution',
          source: 'modern-daemon'
        };
      } else {
        console.log(`❌ BRIDGE: Modern execution failed for ${command.name}, falling back to legacy`);
        return await this.routeToLegacyCommand(command);
      }

    } catch (error) {
      console.error(`❌ BRIDGE: Modern daemon error for ${command.name}:`, error);
      console.log(`🔄 BRIDGE: Falling back to legacy system`);
      return await this.routeToLegacyCommand(command);
    }
  }

  /**
   * Route to legacy system (preserves existing behavior)
   */
  private async routeToLegacy(task: string, source: string): Promise<any> {
    console.log(`⚠️ BRIDGE: Routing to legacy system`);
    
    // This would call the legacy intelligent routing system
    // For now, return a placeholder that mimics legacy behavior
    return {
      result: 'Legacy routing not yet implemented',
      role: 'LegacyCommand',
      type: 'legacy_command_execution'
    };
  }

  /**
   * Route specific command to legacy system
   */
  private async routeToLegacyCommand(command: LegacyCommand): Promise<any> {
    console.log(`⚠️ BRIDGE: Routing command to legacy system: ${command.name}`);
    
    try {
      if (this.legacyFallback && this.legacyFallback.executeCommand) {
        const result = await this.legacyFallback.executeCommand(
          command.name, 
          command.params, 
          command.encoding || 'utf-8'
        );
        
        return {
          result: {
            command: command.name,
            params: command.params,
            result: result
          },
          role: 'BusCommand',
          type: 'legacy_command_execution'
        };
      } else {
        throw new Error('Legacy fallback not available');
      }
    } catch (error) {
      console.error(`❌ BRIDGE: Legacy execution failed for ${command.name}:`, error);
      return {
        result: {
          command: command.name,
          error: error instanceof Error ? error.message : String(error)
        },
        role: 'BusCommand',
        type: 'command_execution_failed'
      };
    }
  }

  /**
   * Migration control methods
   */
  enableMigration(percentage: number = 10): void {
    this.migrationEnabled = true;
    this.migrationPercentage = Math.max(0, Math.min(100, percentage));
    console.log(`🌉 BRIDGE: Migration enabled at ${this.migrationPercentage}%`);
  }

  disableMigration(): void {
    this.migrationEnabled = false;
    this.migrationPercentage = 0;
    console.log(`🌉 BRIDGE: Migration disabled`);
  }

  setMigrationPercentage(percentage: number): void {
    this.migrationPercentage = Math.max(0, Math.min(100, percentage));
    console.log(`🌉 BRIDGE: Migration percentage set to ${this.migrationPercentage}%`);
  }

  migrateCommand(commandName: string): void {
    this.commandMigrationStatus.set(commandName.toUpperCase(), true);
    console.log(`🌉 BRIDGE: Command ${commandName} marked as migrated`);
  }

  rollbackCommand(commandName: string): void {
    this.commandMigrationStatus.set(commandName.toUpperCase(), false);
    console.log(`🌉 BRIDGE: Command ${commandName} rolled back to legacy`);
  }

  getMigrationStatus(): any {
    return {
      enabled: this.migrationEnabled,
      percentage: this.migrationPercentage,
      migratedCommands: Array.from(this.commandMigrationStatus.entries())
        .filter(([_, migrated]) => migrated)
        .map(([command, _]) => command),
      totalCommands: this.commandMigrationStatus.size
    };
  }

  // Helper methods
  private parseParams(params: string): string | object {
    if (!params || params.trim() === '') return '{}';
    
    try {
      return JSON.parse(params);
    } catch {
      return params; // Return as string if not valid JSON
    }
  }

  private createExecutionContext(source: string): ExecutionContext {
    return {
      source: this.mapSource(source),
      priority: 'normal',
      traceId: `bridge-${Date.now()}`
    };
  }

  private mapSource(source: string): ExecutionContext['source'] {
    switch (source) {
      case 'cmd-format':
      case 'direct-format':
        return 'portal';
      default:
        return 'api';
    }
  }

  private initializeMigrationStatus(): void {
    // Mark specific commands as migration-ready
    const migrationReadyCommands = [
      'MIGRATION', 'HELP', 'STATUS', 'LIST', 'WORKSPACE'
    ];
    
    migrationReadyCommands.forEach(command => {
      this.commandMigrationStatus.set(command, false); // Start with legacy
    });
  }
}