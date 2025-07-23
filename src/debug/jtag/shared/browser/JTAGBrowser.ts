/**
 * JTAG System - Browser Implementation
 * 
 * Browser-specific JTAG system that registers browser daemons.
 */

import { JTAGSystem } from '../JTAGSystem';
import { JTAGContext } from '../JTAGTypes';
import { JTAGRouter } from '../JTAGRouter';
import { BROWSER_DAEMONS, createBrowserDaemon } from '../../browser/structure';

export class JTAGBrowser extends JTAGSystem {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }
  
  /**
   * Setup browser-specific daemons using static structure
   */
  protected async setupDaemons(): Promise<void> {
    for (const daemonEntry of BROWSER_DAEMONS) {
      try {
        const daemon = createBrowserDaemon(daemonEntry.name, this.context, this.router);
        
        if (daemon) {
          this.register(daemonEntry.name, daemon);
          console.log(`📦 Registered browser daemon: ${daemonEntry.name} (${daemonEntry.className})`);
        }
      } catch (error: any) {
        console.error(`❌ Failed to create browser daemon ${daemonEntry.name}:`, error.message);
      }
    }

    console.log(`🔌 JTAG Browser System: Registered ${this.daemons.size} daemons statically`);
  }
}