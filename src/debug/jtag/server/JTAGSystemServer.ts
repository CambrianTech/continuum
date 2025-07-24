/**
 * JTAG System - Server Implementation
 * 
 * Server-specific JTAG system implementation with server daemons and transports.
 * Follows the symmetric daemon architecture pattern established in CommandDaemon.
 */

import { JTAGSystem } from '../shared/JTAGSystem';
import type { JTAGContext } from '../shared/JTAGTypes';
import { JTAG_ENVIRONMENTS } from '../shared/JTAGTypes';
import { JTAGRouter } from '../shared/JTAGRouter';
import { SYSTEM_EVENTS } from '../shared/events/SystemEvents';
import type { DaemonBase, DaemonEntry } from '../shared/DaemonBase';
import { SERVER_DAEMONS } from './structure';

export class JTAGSystemServer extends JTAGSystem {
  protected override get daemonEntries(): DaemonEntry[] { return SERVER_DAEMONS; }
  
  protected override createDaemon(entry: DaemonEntry, context: JTAGContext, router: JTAGRouter): DaemonBase | null {
    return new entry.daemonClass(context, router);
  }
  private static instance: JTAGSystemServer | null = null;

  /**
   * Connect and auto-wire the server JTAG system
   */
  static async connect(): Promise<JTAGSystemServer> {
    if (JTAGSystemServer.instance) {
      return JTAGSystemServer.instance;
    }

    // 1. Create server context
    const context: JTAGContext = {
      uuid: `jtag_server_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`,
      environment: JTAG_ENVIRONMENTS.SERVER
    };

    console.log(`🔄 JTAG System: Connecting server environment...`);

    // 2. Create universal router
    const router = new JTAGRouter(context);
    
    // Emit initializing event
    router.eventSystem.emit(SYSTEM_EVENTS.INITIALIZING, {
      context,
      timestamp: new Date().toISOString()
    });
    console.log(`🎬 JTAG System: Initializing server environment`);
    
    await router.initialize();

    // 3. Create server system instance
    const system = new JTAGSystemServer(context, router);
    
    // 4. Setup daemons directly (no delegation needed)
    await system.setupDaemons();

    // 5. Setup cross-context transport
    await system.setupTransports();
    
    // Emit transport ready event
    router.eventSystem.emit(SYSTEM_EVENTS.TRANSPORT_READY, {
      context,
      timestamp: new Date().toISOString(),
      transportType: 'websocket-server'
    });
    console.log(`🔗 JTAG System: Transport ready event emitted`);

    JTAGSystemServer.instance = system;
    
    console.log(`✅ JTAG System: Connected server successfully`);
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