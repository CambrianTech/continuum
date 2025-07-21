/**
 * JTAG - Universal Debugging System
 * 
 * Super simple API for debugging - works everywhere.
 * 
 * Usage:
 *   import { jtag } from './src/debug/jtag';
 *   
 *   // Dead simple calls
 *   jtag.log('Component', 'message');
 *   jtag.critical('WebSocket', 'connection failed', errorData);
 *   jtag.trace('Router', 'handleMessage', 'ENTER');
 *   jtag.probe('Registry', 'command_count', registry.size);
 *   jtag.screenshot('debug-screenshot.png');
 */

// import { JTAG as JTAGInstance } from './shared/JTAGInstance';
import { JTAGBase } from './shared/JTAGBase';
import { jtagConfig } from './shared/config';

// ROBUST AUTO-INITIALIZATION: Never fail on import
try {
  // Create default instance will auto-initialize
  console.log('[JTAG] Creating instance-based JTAG system...');
} catch (error: any) {
  // GRACEFUL DEGRADATION: If JTAG fails, don't break the importing application
  const fallbackConsole = typeof console !== 'undefined' ? console : { log: () => {}, error: () => {}, warn: () => {} };
  fallbackConsole.warn('[JTAG] Initialization failed, running in fallback mode:', error.message);
}

/**
 * Production-grade JTAG API with graceful degradation
 * Never throws errors - always returns reasonable defaults
 */
const createRobustJTAG = () => {
  // Fallback functions for when JTAG fails
  const fallback = {
    log: () => { /* silent fallback */ },
    warn: () => { /* silent fallback */ },
    error: () => { /* silent fallback */ },
    critical: () => { /* silent fallback */ },
    trace: () => { /* silent fallback */ },
    probe: () => { /* silent fallback */ },
    test: () => { /* silent fallback */ },
    screenshot: async () => ({ success: false, error: 'JTAG unavailable' }),
    exec: async () => ({ success: false, error: 'JTAG unavailable' }),
    connect: async () => ({ healthy: false, transport: { type: 'websocket' as const, state: 'error' as const, endpoint: '', latency: 0 }, session: { id: '', uuid: '', uptime: 0 } }),
    getUUID: () => ({ uuid: 'fallback_' + Date.now(), sessionId: 'fallback_session' }),
    config: { jtagPort: 9001, enableRemoteLogging: false, enableConsoleOutput: false }
  };

  try {
    // Try to use real JTAG API
    return {
      log: JTAGBase.log.bind(JTAGBase),
      warn: JTAGBase.warn.bind(JTAGBase),
      error: JTAGBase.error.bind(JTAGBase),
      critical: JTAGBase.critical.bind(JTAGBase),
      trace: JTAGBase.trace.bind(JTAGBase),
      probe: JTAGBase.probe.bind(JTAGBase),
      test: JTAGBase.test.bind(JTAGBase),
      screenshot: JTAGBase.screenshot.bind(JTAGBase),
      exec: JTAGBase.exec.bind(JTAGBase),
      connect: JTAGBase.connect.bind(JTAGBase),
      getUUID: JTAGBase.getUUID.bind(JTAGBase),
      config: jtagConfig
    };
  } catch (error) {
    // Return fallback API if JTAGBase is broken
    return fallback;
  }
};

export const jtag = createRobustJTAG();

// Alias exports for different naming preferences
export const JTAG = JTAGBase;
export const debug = JTAGBase;

// Type exports
export * from './shared/JTAGTypes';

// Export transport and router for advanced usage
export { JTAGRouter, jtagRouter } from './shared/JTAGRouter';
// Legacy command transport removed - use router directly