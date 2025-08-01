/**
 * JTAG Universal Command Bus - Server Entry Point
 * 
 * Server-specific entry point that exports JTAGSystemServer as jtag
 */

import { JTAGSystemServer } from './server/JTAGSystemServer';
import { JTAGClientServer } from './server/JTAGClientServer';
import type { JTAGBase } from './shared/JTAGBase';

export const jtag = {
  // Universal client interface - allows targeting different environments
  async connect(options?: { targetEnvironment?: 'server' | 'browser' }) {
    const targetEnv = options?.targetEnvironment || 'server';
    console.log(`🔌 Server: Connecting via JTAGClientServer (target: ${targetEnv})`);
    
    const connectionResult = await JTAGClientServer.connectRemote({ 
      targetEnvironment: targetEnv 
    });
    
    console.log(`✅ Server: JTAGClient connected with ${connectionResult.listResult.totalCount} commands`);
    return connectionResult;
  },

  // Legacy: Full system access (for advanced usage)
  async getSystem() {
    return JTAGSystemServer.connect();
  }
};

// Direct export for advanced usage
export { JTAGSystemServer };
export * from '@shared/JTAGTypes';