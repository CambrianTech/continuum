/**
 * JTAG System - Browser Implementation
 * 
 * Browser-specific JTAG system that registers browser daemons.
 */

import { JTAGSystem } from '../shared/JTAGSystem';
import { JTAGContext } from '../shared/JTAGTypes';
import { JTAGRouter } from '../shared/JTAGRouter';
import { BROWSER_DAEMONS, createBrowserDaemon } from './structure';
import { SystemEvents } from '../shared/events/SystemEvents';

export class JTAGBrowser extends JTAGSystem {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }
  
  /**
   * Setup browser-specific daemons using static structure
   */
  async setupDaemons(): Promise<void> {
    // Emit daemons loading event
    this.router.eventSystem.emit(SystemEvents.DAEMONS_LOADING, {
      context: this.context,
      timestamp: new Date().toISOString(),
      expectedDaemons: BROWSER_DAEMONS.map(d => d.name)
    });
    console.log(`🏗️ JTAG Browser: Loading ${BROWSER_DAEMONS.length} daemons...`);
    
    const daemonPromises = BROWSER_DAEMONS.map(async (daemonEntry) => {
      try {
        const daemon = createBrowserDaemon(daemonEntry.name, this.context, this.router);
        
        if (daemon) {
          // Initialize daemon after construction is complete
          await daemon.initializeDaemon();
          
          this.register(daemonEntry.name, daemon);
          console.log(`📦 Registered browser daemon: ${daemonEntry.name} (${daemonEntry.className})`);
          return daemon;
        }
        return null;
      } catch (error: any) {
        console.error(`❌ Failed to create browser daemon ${daemonEntry.name}:`, error.message);
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
    console.log(`🔌 JTAG Browser System: Registered ${this.daemons.size} daemons statically`);
  }

  /**
   * Setup browser-specific transports
   */
  async setupTransports(): Promise<void> {
    await this.router.setupCrossContextTransport();
    console.log(`🔗 JTAG Browser System: Cross-context transport configured`);
  }
}