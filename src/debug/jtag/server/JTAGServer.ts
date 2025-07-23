/**
 * JTAG System - Server Implementation
 * 
 * Server-specific JTAG system that registers server daemons.
 */

import { JTAGSystem } from '../shared/JTAGSystem';
import { JTAGContext } from '../shared/JTAGTypes';
import { JTAGRouter } from '../shared/JTAGRouter';
import { SERVER_DAEMONS, createServerDaemon } from './structure';

export class JTAGServer extends JTAGSystem {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }
  
  /**
   * Setup server-specific daemons using static structure
   */
  async setupDaemons(): Promise<void> {
    for (const daemonEntry of SERVER_DAEMONS) {
      try {
        const daemon = createServerDaemon(daemonEntry.name, this.context, this.router);
        
        if (daemon) {
          this.register(daemonEntry.name, daemon);
          console.log(`📦 Registered server daemon: ${daemonEntry.name} (${daemonEntry.className})`);
        }
      } catch (error: any) {
        console.error(`❌ Failed to create server daemon ${daemonEntry.name}:`, error.message);
      }
    }

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