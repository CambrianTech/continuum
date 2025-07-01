#!/usr/bin/env tsx
/**
 * Continuum Main Entry Point
 * 
 * Delegates to properly organized system startup module
 */

import { ContinuumSystem } from './src/system/startup/ContinuumSystemStartup';

// CRASH DETECTION - Log exactly what kills the system
process.on('uncaughtException', (error) => {
  console.error('🚨🚨🚨 UNCAUGHT EXCEPTION - SYSTEM DYING:');
  console.error('Error:', error.message);
  console.error('Stack:', error.stack);
  console.error('Time:', new Date().toISOString());
  process.exit(1);
});

process.on('unhandledRejection', (reason, promise) => {
  console.error('🚨🚨🚨 UNHANDLED PROMISE REJECTION - SYSTEM DYING:');
  console.error('Reason:', reason);
  console.error('Promise:', promise);
  console.error('Time:', new Date().toISOString());
  process.exit(1);
});

process.on('exit', (code) => {
  console.log(`🛑 Process exiting with code: ${code} at ${new Date().toISOString()}`);
});

async function main() {
  const system = new ContinuumSystem();
  
  // Graceful shutdown handling
  process.on('SIGINT', async () => {
    console.log('\n🛑 Received SIGINT, shutting down gracefully...');
    await system.stop();
    process.exit(0);
  });

  process.on('SIGTERM', async () => {
    console.log('\n🛑 Received SIGTERM, shutting down gracefully...');
    await system.stop();
    process.exit(0);
  });

  try {
    await system.start();
    console.log('✅ Continuum launched successfully');
    console.log('🌐 Browser interface: http://localhost:9000');  
    console.log('🔄 Daemons running in background');
    
    // Exit cleanly - daemons continue independently
    setTimeout(() => {
      process.exit(0);
    }, 2000);
  } catch (error) {
    console.error('💥 System startup failed:', error);
    process.exit(1);
  }
}

main();