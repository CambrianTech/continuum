/**
 * JTAG Universal Command Bus - Smart Client Entry Point
 * 
 * Provides intelligent client connection that automatically gives you the best client,
 * fully connected and ready to use, regardless of where you're running from.
 */

import { JTAGClientFactory } from './shared/JTAGClientFactory';

// Auto-detect environment for legacy compatibility
const isServer = typeof window === 'undefined';

export const jtag = {
  // Smart client factory - automatically gives you the best client, fully connected
  getClient: JTAGClientFactory.getClient.bind(JTAGClientFactory),

  // Legacy system access (creates new systems)
  async connect() {
    if (isServer) {
      const { jtag } = await import('./server-index');
      return jtag.connect();
    } else {
      const { jtag } = await import('./browser-index');
      return jtag.connect();
    }
  }
};

// Export types and base classes for advanced usage
export * from '@shared/JTAGTypes';
export { JTAGSystem } from '@shared/JTAGSystem';

// Default export for easy importing
export default jtag;