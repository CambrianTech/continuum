/**
 * JTAG Universal Command Bus - Browser Entry Point
 * 
 * Browser-specific entry point that exports JTAGSystemBrowser as jtag
 */

import { JTAGSystemBrowser } from './browser/JTAGSystemBrowser';

export const jtag = {
  // Full system access
  async connect() {
    return JTAGSystemBrowser.connect();
  }
};

// Direct export for advanced usage
export { JTAGSystemBrowser };
export * from '@shared/JTAGTypes';