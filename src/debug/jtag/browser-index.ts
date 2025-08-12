/**
 * JTAG Universal Command Bus - Browser Entry Point
 * 
 * Browser-specific entry point that exports JTAGSystemBrowser as jtag
 */

import { JTAGSystemBrowser } from './system/core/system/browser/JTAGSystemBrowser';
import { JTAGClientBrowser } from './system/core/client/browser/JTAGClientBrowser';

export const jtag = {
  // Universal client interface - always returns connection result with client property
  async connect(): Promise<ReturnType<typeof JTAGClientBrowser.connectLocal>> {
    console.log('🔌 Browser: Connecting via JTAGClientBrowser (local connection)');
    const connectionResult = await JTAGClientBrowser.connectLocal();
    console.log('✅ Browser: JTAGClient connected with local system');
    return connectionResult;
  },

  // Legacy: Full system access (for advanced usage)
  async getSystem(): Promise<ReturnType<typeof JTAGSystemBrowser.connect>> {
    return JTAGSystemBrowser.connect();
  }
};

// Direct export for advanced usage
export { JTAGSystemBrowser };
export * from './system/core/types/JTAGTypes';