/**
 * System Initializer - Manages the initialization dependency chain
 * Ensures commands aren't executed until all modules are discovered and ready
 */

import { EventEmitter } from 'events';
import { readdir, stat } from 'fs/promises';
import { join } from 'path';

export interface InitializationPhase {
  name: string;
  status: 'pending' | 'in_progress' | 'completed' | 'failed';
  startTime?: Date;
  endTime?: Date;
  duration?: number;
  error?: string;
}

export interface SystemReadyState {
  isReady: boolean;
  phases: Map<string, InitializationPhase>;
  commandsDiscovered: number;
  daemonsReady: number;
  totalInitTime: number;
}

export class SystemInitializer extends EventEmitter {
  private phases = new Map<string, InitializationPhase>();
  private queuedCommands: Array<{
    command: string;
    params: any;
    resolve: (result: any) => void;
    reject: (error: Error) => void;
    timestamp: Date;
  }> = [];
  
  private isSystemReady = false;
  private initStartTime = new Date();

  constructor() {
    super();
    this.setupInitializationPhases();
  }

  /**
   * Initialize the entire system in proper dependency order
   */
  async initialize(): Promise<void> {
    this.log('🚀 Starting Continuum system initialization...');
    this.initStartTime = new Date();

    try {
      // Phase 1: Command Discovery
      await this.executePhase('command-discovery', async () => {
        const commands = await this.discoverCommands();
        this.log(`📦 Discovered ${commands.length} command modules`);
        return { commandsFound: commands.length };
      });

      // Phase 2: Daemon Registration  
      await this.executePhase('daemon-registration', async () => {
        const daemons = await this.registerDaemons();
        this.log(`⚙️ Registered ${daemons.length} daemon processes`);
        return { daemonsRegistered: daemons.length };
      });

      // Phase 3: Command System Ready
      await this.executePhase('command-system-ready', async () => {
        this.log('🎯 Command system ready for execution');
        return { ready: true };
      });

      // Phase 4: Process Queued Commands
      await this.executePhase('process-queue', async () => {
        const queueSize = this.queuedCommands.length;
        if (queueSize > 0) {
          this.log(`🔄 Processing ${queueSize} queued commands...`);
          await this.processQueuedCommands();
        }
        return { processedCommands: queueSize };
      });

      this.isSystemReady = true;
      const totalTime = Date.now() - this.initStartTime.getTime();
      
      this.log(`✅ System initialization complete (${totalTime}ms)`);
      this.emit('system-ready', this.getReadyState());

    } catch (error) {
      this.log(`❌ System initialization failed: ${error.message}`, 'error');
      this.emit('system-error', error);
      throw error;
    }
  }

  /**
   * Execute a command - queue if system not ready, execute immediately if ready
   */
  async executeCommand(command: string, params: any): Promise<any> {
    if (!this.isSystemReady) {
      this.log(`🔄 Queueing command: ${command} (system not ready)`);
      
      return new Promise((resolve, reject) => {
        this.queuedCommands.push({
          command,
          params,
          resolve,
          reject,
          timestamp: new Date()
        });
      });
    }

    // System is ready - execute immediately
    this.log(`⚡ Executing command: ${command}`);
    return this.performCommandExecution(command, params);
  }

  /**
   * Get current system ready state
   */
  getReadyState(): SystemReadyState {
    const totalTime = Date.now() - this.initStartTime.getTime();
    
    return {
      isReady: this.isSystemReady,
      phases: new Map(this.phases),
      commandsDiscovered: this.getPhaseResult('command-discovery')?.commandsFound || 0,
      daemonsReady: this.getPhaseResult('daemon-registration')?.daemonsRegistered || 0,
      totalInitTime: totalTime
    };
  }

  /**
   * Check if system is ready
   */
  isReady(): boolean {
    return this.isSystemReady;
  }

  // Private implementation methods
  private setupInitializationPhases(): void {
    const phaseNames = [
      'command-discovery',
      'daemon-registration', 
      'command-system-ready',
      'process-queue'
    ];

    phaseNames.forEach(name => {
      this.phases.set(name, {
        name,
        status: 'pending'
      });
    });
  }

  private async executePhase(phaseName: string, executor: () => Promise<any>): Promise<any> {
    const phase = this.phases.get(phaseName)!;
    
    phase.status = 'in_progress';
    phase.startTime = new Date();
    
    this.log(`📋 Starting phase: ${phaseName}`);
    this.emit('phase-start', phaseName);

    try {
      const result = await executor();
      
      phase.status = 'completed';
      phase.endTime = new Date();
      phase.duration = phase.endTime.getTime() - phase.startTime!.getTime();
      
      this.log(`✅ Completed phase: ${phaseName} (${phase.duration}ms)`);
      this.emit('phase-complete', phaseName, result);
      
      return result;
      
    } catch (error) {
      phase.status = 'failed';
      phase.endTime = new Date();
      phase.error = error.message;
      
      this.log(`❌ Failed phase: ${phaseName} - ${error.message}`, 'error');
      this.emit('phase-error', phaseName, error);
      
      throw error;
    }
  }

  private async discoverCommands(): Promise<string[]> {
    const commandsDir = 'src/commands';
    const commands: string[] = [];
    
    try {
      await this.scanCommandDirectory(commandsDir, commands);
    } catch (error) {
      this.log(`⚠️ Error scanning commands directory: ${error.message}`, 'warn');
    }
    
    return commands;
  }

  private async scanCommandDirectory(dirPath: string, commands: string[]): Promise<void> {
    try {
      const entries = await readdir(dirPath);
      
      for (const entry of entries) {
        const entryPath = join(dirPath, entry);
        const stats = await stat(entryPath);
        
        if (stats.isDirectory()) {
          // Check for command package structure
          const packagePath = join(entryPath, 'package.json');
          try {
            await stat(packagePath);
            commands.push(entry);
            this.log(`📦 Found command: ${entry}`);
          } catch {
            // Not a command package, recurse into subdirectory
            await this.scanCommandDirectory(entryPath, commands);
          }
        }
      }
    } catch (error) {
      this.log(`⚠️ Error scanning directory ${dirPath}: ${error.message}`, 'warn');
    }
  }

  private async registerDaemons(): Promise<string[]> {
    // For now, return mock daemons
    // In full implementation, this would register actual daemon processes
    const daemons = [
      'command-processor',
      'websocket-server', 
      'renderer',
      'browser-manager'
    ];
    
    return daemons;
  }

  private async processQueuedCommands(): Promise<void> {
    const commands = [...this.queuedCommands];
    this.queuedCommands = [];
    
    for (const queuedCommand of commands) {
      try {
        const result = await this.performCommandExecution(
          queuedCommand.command, 
          queuedCommand.params
        );
        queuedCommand.resolve(result);
      } catch (error) {
        queuedCommand.reject(error);
      }
    }
  }

  private async performCommandExecution(command: string, params: any): Promise<any> {
    // Mock command execution for now
    // In full implementation, this would route to actual command processors
    
    this.log(`⚡ DAEMON: Executing ${command}`, 'daemon');
    
    // Simulate command processing time
    await new Promise(resolve => setTimeout(resolve, 10));
    
    // Mock successful response
    const response = {
      success: true,
      data: {
        command,
        params,
        timestamp: new Date().toISOString(),
        processedBy: 'command-processor-daemon',
        version: '0.2.2204'
      }
    };
    
    this.log(`✅ DAEMON: Command ${command} completed successfully`, 'daemon');
    return response;
  }

  private getPhaseResult(phaseName: string): any {
    const phase = this.phases.get(phaseName);
    return phase?.status === 'completed' ? (phase as any).result : null;
  }

  private log(message: string, level: 'info' | 'warn' | 'error' | 'daemon' = 'info'): void {
    const timestamp = new Date().toISOString();
    const prefix = level === 'daemon' ? '[DAEMON]' : '[CORE]';
    
    switch (level) {
      case 'error':
        console.error(`${timestamp} ${prefix} ❌ ${message}`);
        break;
      case 'warn':
        console.warn(`${timestamp} ${prefix} ⚠️ ${message}`);
        break;
      case 'daemon':
        console.log(`${timestamp} ${prefix} 🔧 ${message}`);
        break;
      default:
        console.log(`${timestamp} ${prefix} ${message}`);
    }
  }
}

export default SystemInitializer;