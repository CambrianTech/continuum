#!/usr/bin/env npx tsx
/**
 * JTAG Auto-Start
 * 
 * Ultra-simple integration - just require and it works automatically
 * 
 * Usage in any Node.js app:
 * ```javascript
 * require('@continuum/jtag/auto-start');
 * // That's it! JTAG is now active
 * ```
 * 
 * Detects Express, Connect, and other frameworks automatically
 */

import { middleware } from './middleware';

/**
 * Attempt to auto-detect and hook into Express/Connect apps
 */
function detectAndInjectMiddleware(): void {
  console.log('🔍 JTAG: Scanning for Express/Connect applications...');

  // Hook into Express application creation
  try {
    const originalExpress = require.cache[require.resolve('express')];
    if (originalExpress && originalExpress.exports) {
      const express = originalExpress.exports;
      const originalApp = express;
      
      // Wrap express() to auto-inject JTAG middleware
      originalExpress.exports = function(...args: any[]) {
        const app = originalApp(...args);
        
        // Auto-inject JTAG middleware
        console.log('🚀 JTAG: Auto-injecting into Express application');
        app.use(middleware({
          development: true,
          injectBrowserClient: true
        }));
        
        return app;
      };
      
      // Preserve all Express properties
      Object.setPrototypeOf(originalExpress.exports, originalApp);
      Object.assign(originalExpress.exports, originalApp);
      
      console.log('✅ JTAG: Express auto-injection enabled');
    }
  } catch (error) {
    // Express not found or already loaded - try other methods
  }

  // Hook into HTTP server creation as fallback
  try {
    const http = require('http');
    const originalCreateServer = http.createServer;
    
    http.createServer = function(...args: any[]) {
      const server = originalCreateServer.apply(this, args);
      console.log('🚀 JTAG: Detected HTTP server creation - JTAG system will start independently');
      
      // Start JTAG system independently
      initializeJTAG();
      
      return server;
    };
    
    console.log('✅ JTAG: HTTP server hook enabled');
  } catch (error) {
    console.warn('⚠️ JTAG: Could not hook HTTP server:', error.message);
  }
}

/**
 * Initialize JTAG system independently
 */
async function initializeJTAG(): Promise<void> {
  try {
    // Import and start JTAG system
    const { JTAGSystemServer } = await import('./system/core/system/server/JTAGSystemServer');
    const { getActivePorts } = await import('./system/shared/ExampleConfig');
    
    let wsPort: number;
    try {
      const activePorts = getActivePorts();
      wsPort = activePorts.websocket_server;
      console.log(`🔧 JTAG: Using WebSocket port ${wsPort} from active configuration`);
    } catch (error) {
      throw new Error(`JTAG Auto-Start: Failed to determine WebSocket port from configuration. ${(error as Error).message}. Ensure package.json configuration is properly set up.`);
    }
    
    await JTAGSystemServer.connect();
    console.log(`✅ JTAG: System auto-started on WebSocket port ${wsPort}`);
    console.log(`🌐 JTAG: Dashboard available at http://localhost:${process.env.PORT || 3000}/__jtag/`);
    
  } catch (error) {
    console.error('❌ JTAG: Auto-start failed:', error.message);
    // Don't crash the main application
  }
}

/**
 * Initialize auto-start detection
 */
function initialize(): void {
  if (process.env.JTAG_DISABLE_AUTO_START === 'true') {
    console.log('🚫 JTAG: Auto-start disabled by environment variable');
    return;
  }

  console.log('🚀 JTAG: Auto-start initializing...');
  
  // Detect existing frameworks
  detectAndInjectMiddleware();
  
  // Fallback: start JTAG system independently after a short delay
  setTimeout(() => {
    console.log('🔄 JTAG: Fallback initialization...');
    initializeJTAG();
  }, 1000);
}

// Auto-initialize when this file is required
if (typeof window === 'undefined' && typeof global !== 'undefined') {
  // We're in Node.js
  initialize();
}

export { initialize };