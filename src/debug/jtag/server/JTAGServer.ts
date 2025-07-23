/**
 * JTAG System - Server Implementation
 * 
 * Server-specific JTAG system that registers server daemons.
 */

import { JTAGSystem } from '../shared/JTAGSystem';
import { JTAGContext } from '../shared/JTAGTypes';
import { JTAGRouter } from '../shared/JTAGRouter';
import { SERVER_DAEMONS, createServerDaemon } from './structure';
import { SystemEvents } from '../shared/events/SystemEvents';

export class JTAGServer extends JTAGSystem {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }
  
  /**
   * Setup server-specific daemons using static structure
   */
  async setupDaemons(): Promise<void> {
    // Emit daemons loading event
    this.router.eventSystem.emit(SystemEvents.DAEMONS_LOADING, {
      context: this.context,
      timestamp: new Date().toISOString(),
      expectedDaemons: SERVER_DAEMONS.map(d => d.name)
    });
    console.log(`🏗️ JTAG Server: Loading ${SERVER_DAEMONS.length} daemons...`);
    
    const daemonPromises = SERVER_DAEMONS.map(async (daemonEntry) => {
      try {
        const daemon = createServerDaemon(daemonEntry.name, this.context, this.router);
        
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
   * Setup server-specific transports
   */
  async setupTransports(): Promise<void> {
    await this.router.setupCrossContextTransport();
    console.log(`🔗 JTAG Server System: Cross-context transport configured`);
  }
}