/**
 * JTAG Universal System - Complete Implementation
 * 
 * This is the main entry point that auto-wires the entire JTAG Universal Command Bus.
 * Provides the `await JTAGSystem.connect()` interface that sets up all environments,
 * routers, transports, and daemons automatically.
 */

import { JTAGBase, type CommandsInterface } from '@shared/JTAGBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { SYSTEM_EVENTS } from '@sharedEvents/SystemEvents';
import type { JTAGRouter } from '@shared/JTAGRouter';
import type { DaemonBase } from '@shared/DaemonBase';
import type { DaemonEntry } from '@shared/DaemonBase';
import { CommandDaemon } from '@daemonsCommandDaemon/shared/CommandDaemon';
import { ConsoleDaemon } from '@daemonsConsoleDaemon/shared/ConsoleDaemon';
import { SessionDaemon } from '@daemonsSessionDaemon/shared/SessionDaemon';
import type { JTAGRouterConfig } from '@shared/JTAGRouterTypes';
import type { UUID } from '@shared/CrossPlatformUUID';
import type { SessionCategory } from '@shared/SystemScopes';


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
 * Connection parameters for joining specific sessions
 */
export interface ConnectionParams {
  readonly sessionId?: UUID;
  readonly sessionCategory?: SessionCategory;
  readonly displayName?: string;
  readonly createIfNotExists?: boolean;
}

/**
 * Complete configuration interface for JTAG System
 */
export interface JTAGSystemConfig {
  readonly version?: Partial<JTAGVersionConfig>;
  readonly daemons?: Partial<JTAGDaemonConfig>;
  readonly router?: JTAGRouterConfig;
  readonly connection?: ConnectionParams;
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
export abstract class JTAGSystem extends JTAGBase {
  protected readonly router: JTAGRouter;
  protected readonly daemons: DaemonBase[] = [];
  protected readonly config: ResolvedJTAGSystemConfig;
  public sessionId = this.context.uuid;

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
          
          this.register(daemon);
          console.log(`📦 Registered server daemon: ${daemon.constructor.name}`);
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
      loadedDaemons: this.daemons.map(d => d.name)
    });
    console.log(`🔌 JTAG Server System: Registered ${this.daemons.length} daemons statically`);
    
    // After daemons are set up, connect to SessionDaemon
    await this.connectSession();
  }

  /**
   * Connect to SessionDaemon to get real sessionId and update ConsoleDaemon
   */
  protected async connectSession(): Promise<void> {
    this.sessionId = this.context.uuid;

    try {      
      this.updateConsoleDaemonSessionId();
      console.log(`✅ ${this.toString()}: Session connected - ${this.sessionId}`);
    } catch (error) {
      console.error(`❌ ${this.toString()}: Error connecting session:`, error);
      // Fallback to context.uuid
      if (this.sessionId !== this.context.uuid) {
        this.sessionId = this.context.uuid;
        this.updateConsoleDaemonSessionId();
      }
    }
  }

  /**
   * Update ConsoleDaemon with current sessionId for proper dual-scope logging
   */
  protected updateConsoleDaemonSessionId(): void {
    const consoleDaemon = this.daemons.find(d => d instanceof ConsoleDaemon);
    if (consoleDaemon && this.sessionId) {
      consoleDaemon.setCurrentSessionId(this.sessionId);
      console.log(`🏷️ ${this.toString()}: Updated ConsoleDaemon sessionId to ${this.sessionId}`);
    }
  }

  /**
   * Register a daemon with this system
   */
  register(daemon: DaemonBase): void {
    this.daemons.push(daemon);
    const version = this.getVersionString();
    console.log(`🎯 JTAG System v${version}: Registered daemon '${daemon.constructor.name}'`);
  }

  /**
   * Implementation of abstract method from JTAGBase
   * Provides command source from CommandDaemon
   */
  protected getCommandsInterface(): CommandsInterface {
    const commandDaemon = this.daemons.find(d => d instanceof CommandDaemon);
    if (!commandDaemon) {
      throw new Error('CommandDaemon not available');
    }
    return commandDaemon.commands;
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
      daemons: this.daemons.map(d => d.name)
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