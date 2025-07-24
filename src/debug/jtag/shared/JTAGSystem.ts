/**
 * JTAG Universal System - Complete Implementation
 * 
 * This is the main entry point that auto-wires the entire JTAG Universal Command Bus.
 * Provides the `await JTAGSystem.connect()` interface that sets up all environments,
 * routers, transports, and daemons automatically.
 */

import { JTAGModule } from './JTAGModule';
import type { JTAGContext } from './JTAGTypes';
import type { ScreenshotParams, ScreenshotResult } from '../daemons/command-daemon/commands/screenshot/shared/ScreenshotTypes';
import { SystemEvents } from '../shared/events/SystemEvents';
/**
 * Strong-typed commands interface
 */
export interface JTAGCommands {
  screenshot(params: ScreenshotParams): Promise<ScreenshotResult>;
}
import type { JTAGRouter } from './JTAGRouter';
import type { DaemonBase } from './DaemonBase';
import type { DaemonEntry } from './DaemonBase';
import type { CommandDaemon } from '../daemons/command-daemon/shared/CommandDaemon';

// Import package.json statically for server environment
import pkg from '../package.json';

// Dynamic version detection
const getVersionString = (): string => {
  try {
    if (typeof window !== 'undefined') {
      // Browser environment - version embedded in build
      return '1.0.156-browser';
    } else {
      // Server environment - can read package.json
      return `${pkg.version}-server`;
    }
  } catch (error) {
    return (error as Error)?.message || 'unknown-version';
  }
};

/**
 * Abstract JTAG System - Base class for environment-specific implementations
 */
export abstract class JTAGSystem extends JTAGModule {
  protected router: JTAGRouter;
  public daemons: Map<string, DaemonBase> = new Map();

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('jtag-system', context);
    this.router = router;
    
    // Log JTAG version on initialization
    const version = getVersionString();
    console.log(`🎯 JTAG System v${version} initializing for ${context.environment} environment`);
  }

  protected abstract get daemonEntries(): DaemonEntry[];

  protected abstract createDaemon(entry: DaemonEntry, context: JTAGContext, router: JTAGRouter): DaemonBase | null;

  /**
   * Setup server-specific daemons using static structure
   */
  async setupDaemons(): Promise<void> {
    // Emit daemons loading event
    this.router.eventSystem.emit(SystemEvents.DAEMONS_LOADING, {
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
      } catch (error: any) {
        console.error(`❌ Failed to create server daemon ${daemonEntry.name}:`, error.message);
        return null;
      }
    });

    await Promise.all(daemonPromises);
    
    // Emit daemons loaded event
    this.router.eventSystem.emit(SystemEvents.DAEMONS_LOADED, {
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
    const version = getVersionString();
    console.log(`🎯 JTAG System v${version}: Registered daemon '${name}' (${daemon.constructor.name})`);
  }


  /**
   * Commands interface with strong typing
   */
  get commands(): JTAGCommands {
    const commandDaemon = this.daemons.get('CommandDaemon') as CommandDaemon;
    if (!commandDaemon) {
      throw new Error('CommandDaemon not available');
    }

    return {
      screenshot: async (params: ScreenshotParams): Promise<ScreenshotResult> => {
        console.log(`📨 JTAG System: Routing screenshot command through messaging system`);
        return await commandDaemon.execute('screenshot', params) as ScreenshotResult;
      }
    };
  }

  /**
   * Direct daemon access - provides jtag.getDaemons()['CommandDaemon']
   */
  getDaemons(): Map<string, DaemonBase> {
    return this.daemons;
  }

  /**
   * Get system information
   */
  getSystemInfo(): { status: string; context: JTAGContext; version: string; daemons: string[] } {
    //TODO: implement actual status check
    return {
      status: 'connected', 
      context: this.context,
      version: getVersionString(),
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