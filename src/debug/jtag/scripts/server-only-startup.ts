#!/usr/bin/env tsx
/**
 * Server-Only Startup - Starts JTAG server without running tests
 * 
 * Based on test-with-server.ts but removes test execution.
 * Keeps the server running permanently.
 */

import { startSystem } from './system-startup';

async function runServerOnly(): Promise<void> {
  console.log('🚀 JTAG SERVER STARTUP - NO TESTS');
  console.log('📋 Starting server and keeping it running...');
  
  try {
    // Use the same server startup logic as tests but skip tests
    await startSystem('npm-start');
    
    // Server is now running and startSystem() includes keep-alive logic
    console.log('✅ Server startup complete - server running persistently');
    
  } catch (error) {
    console.error('💥 Server startup failed:', error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

// If called directly, run server startup
if (require.main === module) {
  runServerOnly();
}