/**
 * System Initializer - Core OS initialization phases
 */

import { EventEmitter } from 'events';

export interface SystemInitializationState {
  modulesDiscovered: boolean;
  commandsLoaded: boolean;
  daemonsReady: boolean;
  discoveredCommands: string[];
  fileSystemReady: boolean;
}

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
  private isSystemReady = false;
  private initStartTime = new Date();
  private systemState: SystemInitializationState;

  constructor() {
    super();
    this.setupPhases();
    this.systemState = {
      modulesDiscovered: false,
      commandsLoaded: false,
      daemonsReady: false,
      discoveredCommands: [],
      fileSystemReady: true // Filesystem is always ready
    };
  }

  async initialize(): Promise<void> {
    console.log('🚀 CORE: Starting system initialization...');
    this.initStartTime = new Date();

    try {
      // Phase 1: Command Discovery
      await this.executePhase('command-discovery', async () => {
        const commands = await this.discoverCommands();
        this.systemState.discoveredCommands = commands;
        this.systemState.modulesDiscovered = true;
        return { commandsFound: commands.length };
      });

      // Phase 2: Command Loading
      await this.executePhase('command-loading', async () => {
        this.systemState.commandsLoaded = true;
        return { commandsLoaded: this.systemState.discoveredCommands.length };
      });

      // Phase 3: Daemon Registration
      await this.executePhase('daemon-registration', async () => {
        this.systemState.daemonsReady = true;
        return { daemonsReady: true };
      });

      this.isSystemReady = true;
      const totalTime = Date.now() - this.initStartTime.getTime();
      
      console.log(`✅ CORE: System initialization complete (${totalTime}ms)`);
      this.emit('system-ready', this.getSystemState());

    } catch (error) {
      console.error(`❌ CORE: System initialization failed: ${error.message}`);
      this.emit('system-error', error);
      throw error;
    }
  }

  getSystemState(): SystemReadyState {
    const totalTime = Date.now() - this.initStartTime.getTime();
    
    return {
      isReady: this.isSystemReady,
      phases: new Map(this.phases),
      commandsDiscovered: this.systemState.discoveredCommands.length,
      daemonsReady: this.systemState.daemonsReady ? 1 : 0,
      totalInitTime: totalTime
    };
  }

  getInitializationState(): SystemInitializationState {
    return { ...this.systemState };
  }

  isReady(): boolean {
    return this.isSystemReady;
  }

  private setupPhases(): void {
    const phaseNames = ['command-discovery', 'command-loading', 'daemon-registration'];
    
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
    
    console.log(`📋 CORE: Starting phase: ${phaseName}`);
    this.emit('phase-start', phaseName);

    try {
      const result = await executor();
      
      phase.status = 'completed';
      phase.endTime = new Date();
      phase.duration = phase.endTime.getTime() - phase.startTime!.getTime();
      
      console.log(`✅ CORE: Completed phase: ${phaseName} (${phase.duration}ms)`);
      this.emit('phase-complete', phaseName, result);
      
      return result;
      
    } catch (error) {
      phase.status = 'failed';
      phase.endTime = new Date();
      phase.error = error.message;
      
      console.error(`❌ CORE: Failed phase: ${phaseName} - ${error.message}`);
      this.emit('phase-error', phaseName, error);
      
      throw error;
    }
  }

  private async discoverCommands(): Promise<string[]> {
    // Mock command discovery - in real implementation, scan command directories
    const mockCommands = [
      'screenshot', 'chat', 'help', 'reload', 'restart', 
      'workspace', 'preferences', 'validation', 'test'
    ];
    
    // Simulate discovery time
    await new Promise(resolve => setTimeout(resolve, 100));
    
    console.log(`📦 CORE: Discovered ${mockCommands.length} command modules`);
    return mockCommands;
  }
}