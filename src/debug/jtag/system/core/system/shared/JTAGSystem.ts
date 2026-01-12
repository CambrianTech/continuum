/**
 * JTAG Universal System - Complete Implementation
 * 
 * This is the main entry point that auto-wires the entire JTAG Universal Command Bus.
 * Provides the `await JTAGSystem.connect()` interface that sets up all environments,
 * routers, transports, and daemons automatically.
 */

import { JTAGBase, type CommandsInterface } from '../../shared/JTAGBase';
import type { JTAGContext } from '../../types/JTAGTypes';
import { SYSTEM_SCOPES } from '../../types/SystemScopes';
import { SYSTEM_EVENTS } from '../../../events';
import type { JTAGRouter } from '../../router/shared/JTAGRouter';
import type { DaemonBase } from '../../../../daemons/command-daemon/shared/DaemonBase';
import type { DaemonEntry } from '../../../../daemons/command-daemon/shared/DaemonBase';
import { CommandDaemon } from '../../../../daemons/command-daemon/shared/CommandDaemon';
import { ConsoleDaemon } from '../../../../daemons/console-daemon/shared/ConsoleDaemon';
import type { JTAGRouterConfig } from '../../router/shared/JTAGRouterTypes';
import type { UUID } from '../../types/CrossPlatformUUID';
import type { SessionCategory } from '../../../../daemons/session-daemon/shared/SessionTypes';
import { isEventSubscriptionProvider, type IEventSubscriptionProvider } from '../../../events/shared/IEventSubscriptionProvider';
import { DaemonOrchestrator, type DaemonInitMetrics } from './DaemonOrchestrator';


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

  /**
   * Public access to daemons for external clients
   */
  public get systemDaemons(): DaemonBase[] {
    return this.daemons;
  }

  /**
   * Get CommandDaemon for direct local command execution
   * Returns null if CommandDaemon is not available
   * CRITICAL for server-side autonomous agents (PersonaUser, etc.)
   */
  public getCommandDaemon(): DaemonBase | null {
    const commandDaemon = this.daemons.find(daemon => daemon.subpath === 'commands');
    return commandDaemon || null;
  }

  /**
   * Get EventsDaemon for event subscription management
   * Returns null if EventsDaemon is not available
   */
  public getEventsDaemon(): IEventSubscriptionProvider | null {
    const eventsDaemon = this.daemons.find(daemon => daemon.subpath === 'events');
    if (!eventsDaemon) {
      return null;
    }
    return isEventSubscriptionProvider(eventsDaemon) ? eventsDaemon : null;
  }

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
      router: config.router ?? { sessionId: SYSTEM_SCOPES.SYSTEM }
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
   * Provide router access for scoped event system
   */
  protected getRouter(): JTAGRouter {
    return this.router;
  }

  //This SHOULD BE the only initialize/setup method in ANY JTAGBase class, make the others protected and call from here
  protected override async initialize(): Promise<void> {
    // // Initialize router with provided configuration
    // await this.router.initialize(this.context, this.config.router);

    // // Setup transports for cross-context communication
    // await this.setupTransports();

    // // Setup daemons based on the static structure
    // await this.setupDaemons();

    // // Initialize session from SessionDaemon if available
    // await this.initializeSessionFromDaemon();
      
  }

  /** Last daemon init metrics (for observability) */
  protected lastDaemonMetrics: DaemonInitMetrics[] = [];

  /**
   * Setup server-specific daemons using static structure
   * Uses DaemonOrchestrator for wave-based parallel startup
   */
  async setupDaemons(): Promise<void> {
    // Emit daemons loading event
    this.router.eventManager.events.emit(SYSTEM_EVENTS.DAEMONS_LOADING, {
      context: this.context,
      timestamp: new Date().toISOString(),
      expectedDaemons: this.daemonEntries.map(d => d.name)
    });

    // Use DaemonOrchestrator for dependency-aware startup
    const orchestrator = new DaemonOrchestrator(
      this.daemonEntries,
      this.context,
      this.router,
      (entry, ctx, router) => this.createDaemon(entry, ctx, router)
    );

    const { daemons, metrics } = await orchestrator.startAll();
    this.lastDaemonMetrics = metrics;

    // Register all daemons
    for (const daemon of daemons) {
      this.register(daemon);

      // Register CommandDaemon to globalThis for server-side Commands.execute() routing
      if (daemon.name === 'command-daemon' && typeof process !== 'undefined') {
        (globalThis as any).__JTAG_COMMAND_DAEMON__ = daemon;
      }
    }

    // Emit daemons loaded event
    this.router.eventManager.events.emit(SYSTEM_EVENTS.DAEMONS_LOADED, {
      context: this.context,
      timestamp: new Date().toISOString(),
      loadedDaemons: this.daemons.map(d => d.name),
      metrics: metrics
    });

    // After daemons are set up, connect to SessionDaemon
    await this.connectSession();
  }

  /**
   * Get last daemon initialization metrics
   */
  getDaemonMetrics(): DaemonInitMetrics[] {
    return this.lastDaemonMetrics;
  }

  /**
   * Connect to SessionDaemon to get real sessionId and update ConsoleDaemon
   */
  protected async connectSession(): Promise<void> {
    // Get sessionId from router config (the proper source)
    this.sessionId = this.config.router.sessionId ?? SYSTEM_SCOPES.SYSTEM;

    try {      
      this.updateConsoleDaemonSessionId();
      
      // Initialize scoped event system now that router and session are ready
      this.initializeScopedEvents();
      
      console.log(`✅ ${this.toString()}: Session connected - ${this.sessionId} (context: ${this.context.uuid})`);
    } catch (error) {
      console.error(`❌ ${this.toString()}: Error connecting session:`, error);
      // Fallback to system scope (not context.uuid!)
      if (this.sessionId !== SYSTEM_SCOPES.SYSTEM) {
        this.sessionId = SYSTEM_SCOPES.SYSTEM;
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
      // Use session provider pattern - daemon gets session ID from system
      consoleDaemon.setSessionIdProvider(() => this.sessionId);
    }
  }

  /**
   * Register a daemon with this system
   */
  register(daemon: DaemonBase): void {
    this.daemons.push(daemon);
  }

  /**
   * Implementation of abstract method from JTAGBase
   * Provides command source from CommandDaemon
   *
   * NOTE: This method checks globalThis FIRST because CommandDaemon registers
   * itself there during its initialize() phase. This is critical because:
   * - UserDaemon's initializeDeferred() needs CommandDaemon
   * - But this.daemons is only populated AFTER orchestrator.startAll() returns
   * - globalThis provides early access during daemon initialization
   */
  public getCommandsInterface(): CommandsInterface {
    // Check globalThis first - available during daemon initialization
    // CommandDaemonServer registers itself here in its initialize() method
    if (typeof process !== 'undefined' && (globalThis as any).__JTAG_COMMAND_DAEMON__) {
      return (globalThis as any).__JTAG_COMMAND_DAEMON__.commands;
    }

    // Fall back to checking daemons array (populated after orchestrator returns)
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