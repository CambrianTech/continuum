/**
 * JTAG Universal Command Bus - Browser Entry Point
 * 
 * Browser-specific entry point that exports JTAGSystemBrowser as jtag
 */

import { JTAGSystemBrowser } from './system/core/system/browser/JTAGSystemBrowser';
import { JTAGClientBrowser } from './system/core/client/browser/JTAGClientBrowser';
import type { JTAGBase } from './system/core/shared/JTAGBase';

export const jtag = {
  // Universal client interface - always returns connection result with client property
  async connect() {
    console.log('🔌 Browser: Connecting via JTAGClientBrowser (local connection)');
    const connectionResult = await JTAGClientBrowser.connectLocal();
    console.log('✅ Browser: JTAGClient connected with local system');
    return connectionResult;
  },

  // Legacy: Full system access (for advanced usage)
  async getSystem() {
    return JTAGSystemBrowser.connect();
  }
};

// Direct export for advanced usage
export { JTAGSystemBrowser };
export * from './system/core/types/JTAGTypes';