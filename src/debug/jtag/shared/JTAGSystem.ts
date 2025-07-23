/**
 * JTAG Universal System - Complete Implementation
 * 
 * This is the main entry point that auto-wires the entire JTAG Universal Command Bus.
 * Provides the `await JTAGSystem.connect()` interface that sets up all environments,
 * routers, transports, and daemons automatically.
 */

import { JTAGModule } from './JTAGModule';
import { JTAGContext, JTAGEnvironment, JTAG_ENVIRONMENTS } from './JTAGTypes';
import { ScreenshotParams, ScreenshotResult } from '../daemons/command-daemon/commands/screenshot/shared/ScreenshotTypes';
import { CommandDaemonBase } from '../daemons/command-daemon/shared/CommandDaemonBase';
import { JTAGRouter } from './JTAGRouter';

/**
 * Main JTAG System - Universal Command Bus
 */
export class JTAGSystem extends JTAGModule {
  protected router: JTAGRouter;
  protected daemons: Map<string, any> = new Map();
  private static instance: JTAGSystem | null = null;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('jtag-system', context);
    this.router = router;
  }

  /**
   * Connect and auto-wire the complete JTAG system
   * This is the main entry point: let jtag = await JTAGSystem.connect()
   */
  static async connect(): Promise<JTAGSystem> {
    if (JTAGSystem.instance) {
      return JTAGSystem.instance;
    }

    // 1. Detect environment and create context
    const environment: JTAGEnvironment = typeof window === 'undefined' ? 'server' : 'browser';
    const context: JTAGContext = {
      uuid: `jtag_${environment}_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`,
      environment
    };

    console.log(`🔄 JTAG System: Connecting ${environment} environment...`);

    // 2. Create universal router
    const router = new JTAGRouter(context);
    await router.initialize();

    // 3. Create environment-specific system instance
    let system: JTAGSystem;
    if (environment === 'server') {
      const { JTAGServer } = await import('./server/JTAGServer');
      system = new JTAGServer(context, router);
    } else {
      const { JTAGBrowser } = await import('./browser/JTAGBrowser');
      system = new JTAGBrowser(context, router);
    }

    // 4. Auto-wire daemons
    await system.setupDaemons();

    // 5. Setup cross-context transport
    await system.setupTransports();

    JTAGSystem.instance = system;
    
    console.log(`✅ JTAG System: Connected ${environment} successfully`);
    console.log(`   Context UUID: ${context.uuid}`);
    console.log(`   Daemons: ${Array.from(system.daemons.keys()).join(', ')}`);

    return system;
  }

  /**
   * Setup daemons - implemented by subclasses
   */
  protected async setupDaemons(): Promise<void> {
    // Base implementation - can be overridden by subclasses
    console.log(`🔧 JTAG System: Base daemon setup complete`);
  }

  /**
   * Register a daemon with this system
   */
  register(name: string, daemon: any): void {
    this.daemons.set(name, daemon);
    console.log(`🎯 ${this.toString()}: Registered daemon '${name}'`);
  }

  /**
   * Setup cross-context transport connections
   */
  protected async setupTransports(): Promise<void> {
    await this.router.setupCrossContextTransport();
    console.log(`🔗 JTAG System: Cross-context transport configured`);
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

    JTAGSystem.instance = null;
    
    console.log(`✅ JTAG System: Shutdown complete`);
  }
}