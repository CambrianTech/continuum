/**
 * JTAG Universal Command Bus - Server Entry Point
 * 
 * Server-specific entry point that exports JTAGSystemServer as jtag
 */

import { JTAGSystemServer } from './server/JTAGSystemServer';
import { JTAGClientServer } from './server/JTAGClientServer';
import type { JTAGBase } from './shared/JTAGBase';

export const jtag = {
  // Universal client interface - always returns JTAGClientServer for server environment
  async connect() {
    console.log('🔌 Server: Connecting via JTAGClientServer (remote connection)');
    const { client } = await JTAGClientServer.connectRemote();
    console.log('✅ Server: JTAGClient connected via transport');
    return client;
  },

  // Legacy: Full system access (for advanced usage)
  async getSystem() {
    return JTAGSystemServer.connect();
  }
};

// Direct export for advanced usage
export { JTAGSystemServer };
export * from '@shared/JTAGTypes';