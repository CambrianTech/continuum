/**
 * JTAG Universal Command Bus - Server Entry Point
 * 
 * Server-specific entry point that exports JTAGSystemServer as jtag
 */

import { JTAGSystemServer } from './server/JTAGSystemServer';
import { JTAGClient } from './shared/JTAGClient';
import type { JTAGBase } from './shared/JTAGBase';

export const jtag = {
  // Full system access
  async connect(): Promise<JTAGSystemServer> {
    return JTAGSystemServer.connect();
  },

  // Smart client - tries local system first, falls back to remote client
  async getClient(): Promise<JTAGBase> {
    try {
      // Try local system first (singleton pattern)
      if (JTAGSystemServer.instance) {
        console.log('🔌 Server: Using existing system singleton');
        return JTAGSystemServer.instance;
      }
      
      console.log('🔌 Server: Attempting local system connection...');
      const system = await JTAGSystemServer.connect();
      console.log('✅ Server: Local system connected');
      return system;
      
    } catch (error) {
      // Local system failed (port conflict, etc.)
      console.log('⚠️ Server: Local system unavailable, trying remote client...');
      console.log('   Reason:', error instanceof Error ? error.message : String(error));
      
      // Fall back to remote client
      const { JTAGClientServer } = await import('./server/JTAGClientServer');
      const { client } = await JTAGClientServer.connectRemote({ targetEnvironment: 'server' });
      console.log('✅ Server: Remote client connected');
      return client;
    }
  }
};

// Direct export for advanced usage
export { JTAGSystemServer };
export * from '@shared/JTAGTypes';