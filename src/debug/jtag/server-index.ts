/**
 * JTAG Universal Command Bus - Server Entry Point
 * 
 * Server-specific entry point that exports JTAGSystemServer as jtag
 */

import { JTAGSystemServer } from './server/JTAGSystemServer';

export const jtag = {
  // Full system access
  async connect(): Promise<JTAGSystemServer> {
    return JTAGSystemServer.connect();
  }
};

// Direct export for advanced usage
export { JTAGSystemServer };
export * from './shared/JTAGTypes';