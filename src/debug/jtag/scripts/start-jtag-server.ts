#!/usr/bin/env npx tsx
/**
 * Start JTAG WebSocket Server
 * 
 * Starts the actual JTAG WebSocket server system on port 9001
 * This is what the browser clients connect to for the JTAG system
 */

import { JTAGSystemServer } from '../system/core/system/server/JTAGSystemServer';

async function startJTAGServer() {
  console.log('🚀 Starting JTAG WebSocket Server System...');
  
  try {
    // Start the actual JTAG system server
    const system = await JTAGSystemServer.connect();
    
    console.log('✅ JTAG WebSocket Server System started successfully');
    console.log('🔌 Browser clients can now connect to ws://localhost:9001');
    
    // Keep the process alive
    process.on('SIGINT', async () => {
      console.log('\n⚡ Shutting down JTAG server...');
      // TODO: Add proper shutdown logic if available
      process.exit(0);
    });
    
    process.on('SIGTERM', async () => {
      console.log('\n⚡ Terminating JTAG server...');
      process.exit(0);
    });
    
    // Keep running
    console.log('📡 JTAG Server running - press Ctrl+C to stop');
    
  } catch (error) {
    console.error('❌ Failed to start JTAG server:', error);
    process.exit(1);
  }
}

// Handle uncaught errors
process.on('uncaughtException', (error) => {
  console.error('🚨 Uncaught Exception in JTAG server:', error);
  process.exit(1);
});

process.on('unhandledRejection', (reason) => {
  console.error('🚨 Unhandled Rejection in JTAG server:', reason);
  process.exit(1);
});

startJTAGServer();