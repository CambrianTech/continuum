/**
 * JTAG - Universal Command Bus Entry Point
 * 
 * This provides a simplified interface to the complete JTAG system while
 * preserving the full auto-discovery architecture underneath.
 */

import { JTAGSystem } from './shared/JTAGSystem';
import { JTAGContext, JTAGEnvironment } from './shared/JTAGTypes';

// Auto-detect environment
const isServer = typeof window === 'undefined';
const environment: JTAGEnvironment = isServer ? 'server' : 'browser';

// Create context for this environment
const context: JTAGContext = {
  uuid: `jtag-index-${Date.now()}-${Math.random().toString(36).substr(2, 6)}`,
  environment
};

/**
 * Simplified JTAG API
 * 
 * For complex usage, use: await JTAGSystem.connect()
 * For simple usage, use this direct interface
 */
const jtag = {
  // Environment info
  environment,
  context,

  // Core logging methods - simplified interface
  log: (component: string, message: string, data?: unknown) => {
    console.log(`[JTAG:${component}] ${message}`, data || '');
    // TODO: Route through actual system when needed
  },

  warn: (component: string, message: string, data?: unknown) => {
    console.warn(`[JTAG:${component}] ${message}`, data || '');
    // TODO: Route through actual system when needed
  },

  error: (component: string, message: string, data?: unknown) => {
    console.error(`[JTAG:${component}] ${message}`, data || '');
    // TODO: Route through actual system when needed
  },

  critical: (component: string, message: string, data?: unknown) => {
    console.error(`[JTAG:CRITICAL:${component}] ${message}`, data || '');
    // TODO: Route through actual system when needed
  },

  info: (component: string, message: string, data?: unknown) => {
    console.info(`[JTAG:${component}] ${message}`, data || '');
    // TODO: Route through actual system when needed
  },

  debug: (component: string, message: string, data?: unknown) => {
    console.debug(`[JTAG:${component}] ${message}`, data || '');
    // TODO: Route through actual system when needed
  },

  // Full system access
  async connect() {
    return await JTAGSystem.connect();
  },

  // System class for advanced usage
  System: JTAGSystem
};

// Export interfaces
export { jtag, JTAGSystem };
export * from './shared/JTAGTypes';
export default jtag;