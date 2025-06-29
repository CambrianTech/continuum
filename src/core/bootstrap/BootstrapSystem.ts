/**
 * Core Bootstrap System
 * Root-level OS commands and initialization dependency management
 */

import { EventEmitter } from 'events';
import { BootstrapCommandRegistry } from './commands/BootstrapCommandRegistry.js';
import { SystemInitializer, SystemReadyState } from './initialization/SystemInitializer.js';
import { CommandQueue } from './queue/CommandQueue.js';

export interface BootstrapSystemState {
  phase: 'starting' | 'discovering' | 'ready' | 'error';
  systemReady: boolean;
  commandsAvailable: string[];
  queuedCommands: number;
  initializationTime: number;
}

export class BootstrapSystem extends EventEmitter {
  private commandRegistry: BootstrapCommandRegistry;
  private systemInitializer: SystemInitializer;
  private commandQueue: CommandQueue;
  private startTime = new Date();

  constructor() {
    super();
    
    console.log('🚀 Initializing Core Bootstrap System...');
    
    this.commandRegistry = new BootstrapCommandRegistry();
    this.systemInitializer = new SystemInitializer();
    this.commandQueue = new CommandQueue();
    
    this.setupEventHandlers();
  }

  /**
   * Start the bootstrap system
   */
  async start(): Promise<void> {
    console.log('🔧 CORE: Starting bootstrap system...');
    this.startTime = new Date();
    
    try {
      // Initialize system in phases
      await this.systemInitializer.initialize();
      
      // Process any queued commands
      await this.commandQueue.processAll(this.executeBootstrapCommand.bind(this));
      
      console.log('✅ CORE: Bootstrap system ready');
      this.emit('bootstrap-ready', this.getSystemState());
      
    } catch (error) {
      console.error('❌ CORE: Bootstrap system failed:', error);
      this.emit('bootstrap-error', error);
      throw error;
    }
  }

  /**
   * Execute a command through the bootstrap system
   */
  async executeCommand(command: string, params: any): Promise<any> {
    const initState = this.systemInitializer.getInitializationState();
    
    console.debug(`📥 SERVER: Bootstrap received command: ${command}`, { 
      params, 
      systemReady: this.systemInitializer.isReady(),
      queueSize: this.commandQueue.size() 
    });
    
    // Check if system is ready for this command
    if (!this.commandRegistry.canExecuteCommand(command, initState)) {
      console.debug(`⏳ SERVER: Queueing command (modules not discovered): ${command}`, {
        systemPhase: this.determinePhase(initState),
        modulesDiscovered: initState.modulesDiscovered
      });
      
      return this.commandQueue.add(command, params);
    }
    
    // Execute immediately
    return this.executeBootstrapCommand(command, params);
  }

  /**
   * Get current system state
   */
  getSystemState(): BootstrapSystemState {
    const initState = this.systemInitializer.getInitializationState();
    const uptime = Date.now() - this.startTime.getTime();
    
    return {
      phase: this.determinePhase(initState),
      systemReady: this.systemInitializer.isReady(),
      commandsAvailable: this.commandRegistry.getAvailableCommands(initState),
      queuedCommands: this.commandQueue.size(),
      initializationTime: uptime
    };
  }

  /**
   * Check if system is ready
   */
  isReady(): boolean {
    return this.systemInitializer.isReady();
  }

  // Private methods
  private async executeBootstrapCommand(command: string, params: any): Promise<any> {
    const initState = this.systemInitializer.getInitializationState();
    
    console.debug(`⚡ SERVER: Executing bootstrap command: ${command}`, {
      category: this.getCategoryForCommand(command),
      systemReady: this.systemInitializer.isReady(),
      processingStartTime: new Date().toISOString()
    });
    
    try {
      const result = await this.commandRegistry.executeCommand(command, params, initState);
      
      console.debug(`✅ SERVER: Bootstrap command completed: ${command}`, {
        success: result.success,
        dataKeys: Object.keys(result.data || {}),
        processedBy: result.data?.processedBy
      });
      
      return result;
      
    } catch (error) {
      console.error(`❌ SERVER: Bootstrap command failed: ${command}`, {
        error: (error as Error).message,
        systemState: this.determinePhase(initState)
      });
      throw error;
    }
  }

  private setupEventHandlers(): void {
    this.systemInitializer.on('system-ready', (readyState: SystemReadyState) => {
      console.log('🎯 CORE: System initialization complete');
      this.emit('system-ready', readyState);
    });

    this.systemInitializer.on('phase-complete', (phaseName: string) => {
      console.log(`📋 CORE: Initialization phase completed: ${phaseName}`);
      this.emit('phase-complete', phaseName);
    });

    this.commandQueue.on('command-processed', (command: string, result: any) => {
      console.log(`🔄 CORE: Queued command processed: ${command}`);
      this.emit('command-processed', { command, result });
    });
  }

  private determinePhase(initState: any): BootstrapSystemState['phase'] {
    if (!this.systemInitializer.isReady()) {
      if (initState.modulesDiscovered) {
        return 'discovering';
      }
      return 'starting';
    }
    return 'ready';
  }

  private getCategoryForCommand(command: string): string {
    const immediateCommands = ['info', 'status'];
    const postDiscoveryCommands = ['list', 'help'];
    const alwaysAvailableCommands = ['filesave', 'fileread'];
    
    if (immediateCommands.includes(command)) return 'immediate';
    if (postDiscoveryCommands.includes(command)) return 'post-discovery';
    if (alwaysAvailableCommands.includes(command)) return 'always-available';
    return 'unknown';
  }
}

export default BootstrapSystem;