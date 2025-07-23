/**
 * JTAG System - Server Implementation
 * 
 * Server-specific JTAG system that registers server daemons.
 */

import { JTAGSystem } from '../JTAGSystem';
import { JTAGContext } from '../JTAGTypes';
import { JTAGRouter } from '../JTAGRouter';
import { getDaemonManifest } from '../../manifests/daemon-manifest';

export class JTAGServer extends JTAGSystem {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }
  
  /**
   * Setup server-specific daemons using auto-discovery
   */
  protected async setupDaemons(): Promise<void> {
    const daemonManifest = getDaemonManifest('server');
    
    for (const [daemonName, manifestEntry] of Object.entries(daemonManifest)) {
      try {
        // Dynamic import using manifest
        const daemonModule = await import(manifestEntry.importPath);
        const DaemonClass = daemonModule[manifestEntry.className];
        
        if (DaemonClass) {
          const daemon = new DaemonClass(this.context, this.router);
          this.register(daemonName, daemon);
          console.log(`📦 Auto-discovered daemon: ${daemonName}`);
        } else {
          console.warn(`⚠️ Daemon class not found: ${manifestEntry.className} in ${manifestEntry.importPath}`);
        }
      } catch (error: any) {
        console.error(`❌ Failed to load daemon ${daemonName}:`, error.message);
      }
    }

    console.log(`🔌 JTAG Server System: Auto-registered ${this.daemons.size} daemons`);
  }
}