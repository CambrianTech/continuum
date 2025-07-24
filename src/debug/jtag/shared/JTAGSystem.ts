/**
 * JTAG Universal System - Complete Implementation
 * 
 * This is the main entry point that auto-wires the entire JTAG Universal Command Bus.
 * Provides the `await JTAGSystem.connect()` interface that sets up all environments,
 * routers, transports, and daemons automatically.
 */

import { JTAGModule } from './JTAGModule';
import type { JTAGContext, CommandParams, CommandResult } from './JTAGTypes';
import { SYSTEM_EVENTS } from '../shared/events/SystemEvents';
import type { JTAGRouter } from './JTAGRouter';
import type { DaemonBase } from './DaemonBase';
import type { DaemonEntry } from './DaemonBase';
import type { CommandDaemon } from '../daemons/command-daemon/shared/CommandDaemon';
import type { JTAGRouterConfig } from './JTAGRouterTypes';


/**
 * Version configuration for JTAG System
 */
export interface JTAGVersionConfig {
  readonly fallback: string;
  readonly enableLogging: boolean;
}

/**
 * Daemon configuration for JTAG System
 */
export interface JTAGDaemonConfig {
  readonly enableParallelInit: boolean;
  readonly initTimeout: number;
}

/**
 * Complete configuration interface for JTAG System
 */
export interface JTAGSystemConfig {
  readonly version?: Partial<JTAGVersionConfig>;
  readonly daemons?: Partial<JTAGDaemonConfig>;
  readonly router?: JTAGRouterConfig;
}

/**
 * Resolved configuration with all required fields
 */
export interface ResolvedJTAGSystemConfig {
  readonly version: JTAGVersionConfig;
  readonly daemons: JTAGDaemonConfig;
  readonly router: JTAGRouterConfig;
}

/**
 * Abstract JTAG System - Base class for environment-specific implementations
 */
export abstract class JTAGSystem extends JTAGModule {
  protected readonly router: JTAGRouter;
  public readonly daemons: Map<string, DaemonBase> = new Map();
  protected readonly config: ResolvedJTAGSystemConfig;

  constructor(context: JTAGContext, router: JTAGRouter, config: JTAGSystemConfig = {}) {
    super('jtag-system', context);
    this.router = router;
    
    // Apply default configuration with strong typing
    this.config = {
      version: {
        fallback: 'unknown-version',
        enableLogging: true,
        ...config.version
      },
      daemons: {
        enableParallelInit: true,
        initTimeout: 10000,
        ...config.daemons
      },
      router: config.router ?? {}
    } as const;
    
    // Log JTAG version on initialization
    if (this.config.version.enableLogging) {
      const version = this.getVersionString();
      console.log(`🎯 JTAG System v${version} initializing for ${context.environment} environment`);
    }
  }

  protected abstract get daemonEntries(): DaemonEntry[];

  protected abstract createDaemon(entry: DaemonEntry, context: JTAGContext, router: JTAGRouter): DaemonBase | null;

  protected getVersionString(): string {
    return this.config.version.fallback;
  }

  /**
   * Setup server-specific daemons using static structure
   */
  async setupDaemons(): Promise<void> {
    // Emit daemons loading event
    this.router.eventSystem.emit(SYSTEM_EVENTS.DAEMONS_LOADING, {
      context: this.context,
      timestamp: new Date().toISOString(),
      expectedDaemons: this.daemonEntries.map(d => d.name)
    });
    console.log(`🏗️ JTAG Server: Loading ${this.daemonEntries.length} daemons...`);

    const daemonPromises = this.daemonEntries.map(async (daemonEntry) => {
      try {
        const daemon = this.createDaemon(daemonEntry, this.context, this.router);
        
        if (daemon) {
          // Initialize daemon after construction is complete
          await daemon.initializeDaemon();
          
          this.register(daemonEntry.name, daemon);
          console.log(`📦 Registered server daemon: ${daemonEntry.name} (${daemonEntry.className})`);
          return daemon;
        }
        return null;
      } catch (error) {
        console.error(`❌ Failed to create server daemon ${daemonEntry.name}:`, error);
        return null;
      }
    });

    await Promise.all(daemonPromises);
    
    // Emit daemons loaded event
    this.router.eventSystem.emit(SYSTEM_EVENTS.DAEMONS_LOADED, {
      context: this.context,
      timestamp: new Date().toISOString(),
      loadedDaemons: Array.from(this.daemons.keys())
    });
    console.log(`🔌 JTAG Server System: Registered ${this.daemons.size} daemons statically`);
  }

  /**
   * Register a daemon with this system
   */
  register(name: string, daemon: DaemonBase): void {
    this.daemons.set(name, daemon);
    const version = this.getVersionString();
    console.log(`🎯 JTAG System v${version}: Registered daemon '${name}' (${daemon.constructor.name})`);
  }

  /**
   * Commands interface - delegate to CommandDaemon's elegantly typed interface
   */
  get commands(): Record<string, (params?: CommandParams) => Promise<CommandResult>> {
    const commandDaemon = this.daemons.get('CommandDaemon') as CommandDaemon;
    if (!commandDaemon) {
      throw new Error('CommandDaemon not available');
    }
    return commandDaemon.commandsInterface;
  }

  /**
   * Get system information
   */
  getSystemInfo(): { status: string; context: JTAGContext; version: string; daemons: string[] } {
    //TODO: implement actual status check
    return {
      status: 'connected', 
      context: this.context,
      version: this.getVersionString(),
      daemons: Array.from(this.daemons.keys())
    };
  }

  /**
   * Shutdown the system and cleanup resources
   */
  async shutdown(): Promise<void> {
    console.log(`🔄 JTAG System: Shutting down...`);

    // Cleanup daemons
    await Promise.all(Array.from(this.daemons.values()).map(daemon => daemon.shutdown()));

    // Cleanup router
    await this.router.shutdown();
    
    console.log(`✅ JTAG System: Shutdown complete`);
  }

   /**
   * Setup server-specific transports
   */
  async setupTransports(): Promise<void> {
    return this.router.setupCrossContextTransport();
  }
}