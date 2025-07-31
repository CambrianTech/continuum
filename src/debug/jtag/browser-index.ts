/**
 * JTAG Universal Command Bus - Browser Entry Point
 * 
 * Browser-specific entry point that exports JTAGSystemBrowser as jtag
 */

import { JTAGSystemBrowser } from './browser/JTAGSystemBrowser';
import { JTAGClientBrowser } from './browser/JTAGClientBrowser';
import type { JTAGBase } from './shared/JTAGBase';

export const jtag = {
  // Full system access
  async connect() {
    return JTAGSystemBrowser.connect();
  },

  // Smart client - tries local system first, falls back to remote client
  async getClient(): Promise<JTAGBase> {
    try {
      // Try local system first (singleton pattern)
      if (JTAGSystemBrowser.instance) {
        console.log('🔌 Browser: Using existing system singleton');
        return JTAGSystemBrowser.instance;
      }
      
      console.log('🔌 Browser: Attempting local system connection...');
      const system = await JTAGSystemBrowser.connect();
      console.log('✅ Browser: Local system connected');
      return system;
      
    } catch (error) {
      // Local system failed
      console.log('⚠️ Browser: Local system unavailable, trying remote client...');
      console.log('   Reason:', error instanceof Error ? error.message : String(error));
      
      // Fall back to remote client
      const { client } = await JTAGClientBrowser.connectRemote({ serverUrl: 'ws://localhost:9001' });
      console.log('✅ Browser: Remote client connected');
      return client;
    }
  }
};

// Direct export for advanced usage
export { JTAGSystemBrowser };
export * from '@shared/JTAGTypes';