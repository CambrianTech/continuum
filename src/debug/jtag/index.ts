/**
 * JTAG Universal Command Bus - Smart Client Entry Point
 * 
 * Provides intelligent client connection that automatically gives you the best client,
 * fully connected and ready to use, regardless of where you're running from.
 */

// Auto-detect environment for legacy compatibility
const isServer = typeof window === 'undefined';

export const jtag = {
  // System access - delegates to environment-specific implementations
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
export * from './system/core/types/JTAGTypes';
export { JTAGSystem } from './system/core/system/shared/JTAGSystem';

// Default export for easy importing
export default jtag;