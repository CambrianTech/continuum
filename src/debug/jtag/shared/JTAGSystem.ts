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
import type { CommandDaemonBase } from '../daemons/command-daemon/shared/CommandDaemonBase';
import type { JTAGRouter } from './JTAGRouter';

// Dynamic version detection
const getVersionString = (): string => {
  try {
    if (typeof window !== 'undefined') {
      // Browser environment - version embedded in build
      return '1.0.156-browser';
    } else {
      // Server environment - can read package.json
      const pkg = require('../package.json');
      return `${pkg.version}-server`;
    }
  } catch (error) {
    return 'unknown';
  }
};

/**
 * Abstract JTAG System - Base class for environment-specific implementations
 */
export abstract class JTAGSystem extends JTAGModule {
  protected router: JTAGRouter;
  public daemons: Map<string, any> = new Map();

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('jtag-system', context);
    this.router = router;
    
    // Log JTAG version on initialization
    const version = getVersionString();
    console.log(`🎯 JTAG System v${version} initializing for ${context.environment} environment`);
  }

  /**
   * Abstract setup methods - implemented by environment-specific subclasses
   */
  abstract setupDaemons(): Promise<void>;
  abstract setupTransports(): Promise<void>;

  /**
   * Register a daemon with this system
   */
  register(name: string, daemon: any): void {
    this.daemons.set(name, daemon);
    const version = getVersionString();
    console.log(`🎯 JTAG System v${version}: Registered daemon '${name}' (${daemon.constructor.name})`);
  }


  /**
   * Command interface - provides jtag.commands.screenshot() etc.
   */
  get commands() {
    return {
      screenshot: async (params: ScreenshotParams): Promise<ScreenshotResult> => {
        const commandDaemon = this.daemons.get('CommandDaemon') as CommandDaemonBase;
        if (!commandDaemon) {
          throw new Error('CommandDaemon not available');
        }
        return await commandDaemon.execute('screenshot', params);
      }
      // Add other commands here as needed
    };
  }

  /**
   * Direct daemon access - provides jtag.getDaemons()['CommandDaemon']
   */
  getDaemons() {
    return this.daemons;
  }

  /**
   * Get system information
   */
  getSystemInfo() {
    return {
      status: 'connected',
      context: this.context,
      version: '1.0.0',
      daemons: Array.from(this.daemons.keys())
    };
  }

  /**
   * Shutdown the system and cleanup resources
   */
  async shutdown(): Promise<void> {
    console.log(`🔄 JTAG System: Shutting down...`);
    
    // Cleanup daemons
    for (const [name, daemon] of Array.from(this.daemons.entries())) {
      if (daemon.shutdown) {
        await daemon.shutdown();
      }
    }
    this.daemons.clear();

    // Cleanup router
    await this.router.shutdown();
    
    console.log(`✅ JTAG System: Shutdown complete`);
  }
}