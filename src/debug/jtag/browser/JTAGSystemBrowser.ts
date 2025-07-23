/**
 * JTAG System - Browser Implementation
 * 
 * Browser-specific JTAG system implementation with browser daemons and transports.
 */

import { JTAGSystem } from '../shared/JTAGSystem';
import { JTAGContext, JTAGEnvironment } from '../shared/JTAGTypes';
import { JTAGRouter } from '../shared/JTAGRouter';
import { JTAGBrowser } from './JTAGBrowser';

export class JTAGSystemBrowser extends JTAGSystem {
  private static instance: JTAGSystemBrowser | null = null;

  private constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Connect and auto-wire the browser JTAG system
   */
  static async connect(): Promise<JTAGSystemBrowser> {
    if (JTAGSystemBrowser.instance) {
      return JTAGSystemBrowser.instance;
    }

    // 1. Create browser context
    const context: JTAGContext = {
      uuid: `jtag_browser_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`,
      environment: 'browser' as JTAGEnvironment
    };

    console.log(`🔄 JTAG System: Connecting browser environment...`);

    // 2. Create universal router
    const router = new JTAGRouter(context);
    await router.initialize();

    // 3. Create browser system instance
    const system = new JTAGSystemBrowser(context, router);
    
    // 4. Use existing JTAGBrowser for daemon setup
    const jtagBrowser = new JTAGBrowser(context, router);
    await jtagBrowser.setupDaemons();
    
    // Copy daemons from JTAGBrowser
    system.daemons = jtagBrowser.daemons;

    // 5. Setup cross-context transport
    await system.setupTransports();

    JTAGSystemBrowser.instance = system;
    
    console.log(`✅ JTAG System: Connected browser successfully`);
    console.log(`   Context UUID: ${context.uuid}`);
    console.log(`   Daemons: ${Array.from(system.daemons.keys()).join(', ')}`);

    return system;
  }

  /**
   * Setup browser-specific transports
   */
  async setupTransports(): Promise<void> {
    await this.router.setupCrossContextTransport();
    console.log(`🔗 JTAG System: Cross-context transport configured`);
  }

  /**
   * Setup browser daemons - delegated to JTAGBrowser
   */
  async setupDaemons(): Promise<void> {
    // Handled in connect() method via JTAGBrowser
  }
}