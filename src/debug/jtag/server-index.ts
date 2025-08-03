/**
 * JTAG Universal Command Bus - Server Entry Point
 * 
 * Server-specific entry point that exports JTAGSystemServer as jtag
 */

import { JTAGSystemServer } from './system/core/system/server/JTAGSystemServer';
import { JTAGClientServer } from './system/core/client/server/JTAGClientServer';
import type { JTAGBase } from './system/core/shared/JTAGBase';

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
export { JTAGSystem } from './system/core/system/shared/JTAGSystem';
export * from './system/core/types/JTAGTypes';