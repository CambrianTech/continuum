/**
 * JTAG System - Browser Implementation
 * 
 * Browser-specific JTAG system implementation with browser daemons and transports.
 * Follows the symmetric daemon architecture pattern established in CommandDaemon.
 */

import { JTAGSystem } from '../shared/JTAGSystem';
import type { JTAGContext } from '../shared/JTAGTypes';
import { JTAG_ENVIRONMENTS } from '../shared/JTAGTypes';
import { JTAGRouter } from '../shared/JTAGRouter';
import { SYSTEM_EVENTS } from '../shared/events/SystemEvents';
import type { DaemonBase, DaemonEntry } from '../shared/DaemonBase';
import { BROWSER_DAEMONS } from './structure';

export class JTAGSystemBrowser extends JTAGSystem {
  protected override get daemonEntries(): DaemonEntry[] { return BROWSER_DAEMONS; }
  
  protected override createDaemon(entry: DaemonEntry, context: JTAGContext, router: JTAGRouter): DaemonBase | null {
    return new entry.daemonClass(context, router);
  }

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
      environment: JTAG_ENVIRONMENTS.BROWSER
    };

    console.log(`🔄 JTAG System: Connecting browser environment...`);

    // 2. Create universal router
    const router = new JTAGRouter(context);
    
    // Emit initializing event
    router.eventSystem.emit(SYSTEM_EVENTS.INITIALIZING, {
      context,
      timestamp: new Date().toISOString()
    });
    console.log(`🎬 JTAG System: Initializing browser environment`);
    
    await router.initialize();

    // 3. Create browser system instance
    const system = new JTAGSystemBrowser(context, router);
    
    // 4. Setup daemons directly (no delegation needed)
    await system.setupDaemons();

    // 5. Setup cross-context transport
    await system.setupTransports();
    
    // Emit transport ready event
    router.eventSystem.emit(SYSTEM_EVENTS.TRANSPORT_READY, {
      context,
      timestamp: new Date().toISOString(),
      transportType: 'websocket-client'
    });
    console.log(`🔗 JTAG System: Transport ready event emitted`);

    JTAGSystemBrowser.instance = system;
    
    console.log(`✅ JTAG System: Connected browser successfully`);
    console.log(`   Context UUID: ${context.uuid}`);
    console.log(`   Daemons: ${Array.from(system.daemons.keys()).join(', ')}`);

    // Emit system ready event after full initialization
    router.eventSystem.emit(SYSTEM_EVENTS.READY, {
      version: '1.0.0',
      context,
      timestamp: new Date().toISOString(),
      components: Array.from(system.daemons.keys())
    });
    console.log(`🎉 JTAG System: System ready event emitted`);

    return system;
  }
}