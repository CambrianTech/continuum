/**
 * JTAG Universal Command Bus - Dynamic Entry Point
 * 
 * Automatically redirects to appropriate environment-specific entry point
 */

// Auto-detect environment
const isServer = typeof window === 'undefined';

// Dynamic export based on environment
export const jtag = {
  // Full system access
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
export * from './shared/JTAGTypes';
export { JTAGSystem } from './shared/JTAGSystem';

// Default export for easy importing
export default jtag;