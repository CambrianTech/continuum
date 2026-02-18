/**
 * JTAG Universal Command Bus - Server Entry Point
 * 
 * Server-specific entry point that exports JTAGSystemServer as jtag
 */

import { JTAGSystemServer } from './system/core/system/server/JTAGSystemServer';
import { JTAGClientServer } from './system/core/client/server/JTAGClientServer';
import { JTAGClient } from './system/core/client/shared/JTAGClient';
import { SYSTEM_SCOPES } from './system/core/types/SystemScopes';

export const jtag = {
  // Universal client interface - allows targeting different environments
  async connect(options?: { targetEnvironment?: 'server' | 'browser' }): Promise<JTAGClientServer> {
    const targetEnv = options?.targetEnvironment ?? 'server';
    console.log(`🔌 Server: Connecting via JTAGClientServer (target: ${targetEnv})`);
    
    const connectionResult = await JTAGClientServer.connectRemote({ 
      targetEnvironment: targetEnv,
      sessionId: SYSTEM_SCOPES.UNKNOWN_SESSION // Let session daemon assign currentUser session
    });
    
    console.log(`✅ Server: JTAGClient connected with ${connectionResult.listResult.totalCount} commands`);

    // Register client in static registry for sharedInstance access
    JTAGClient.registerClient('default', connectionResult.client);

    // Return the client with commands interface, not the full connection result
    return connectionResult.client;
  },

  // Legacy: Full system access (for advanced usage)
  async getSystem(): Promise<ReturnType<typeof JTAGSystemServer.connect>> {
    return JTAGSystemServer.connect();
  }
};

// Direct export for advanced usage
export { JTAGSystemServer };
export { JTAGSystem } from './system/core/system/shared/JTAGSystem';
export * from './system/core/types/JTAGTypes';