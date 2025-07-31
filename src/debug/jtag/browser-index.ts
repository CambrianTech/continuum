/**
 * JTAG Universal Command Bus - Browser Entry Point
 * 
 * Browser-specific entry point that exports JTAGSystemBrowser as jtag
 */

import { JTAGSystemBrowser } from './browser/JTAGSystemBrowser';
import { JTAGClientBrowser } from './browser/JTAGClientBrowser';
import type { JTAGBase } from './shared/JTAGBase';

export const jtag = {
  // Universal client interface - always returns JTAGClientBrowser for browser environment
  async connect() {
    console.log('🔌 Browser: Connecting via JTAGClientBrowser (local connection)');
    const { client } = await JTAGClientBrowser.connectLocal();
    console.log('✅ Browser: JTAGClient connected with local system');
    return client;
  },

  // Legacy: Full system access (for advanced usage)
  async getSystem() {
    return JTAGSystemBrowser.connect();
  }
};

// Direct export for advanced usage
export { JTAGSystemBrowser };
export * from '@shared/JTAGTypes';