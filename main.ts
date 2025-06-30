#!/usr/bin/env tsx
/**
 * Continuum Main Entry Point
 * 
 * Delegates to properly organized system startup module
 */

import { ContinuumSystem } from './src/system/startup/ContinuumSystemStartup';

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
    console.log('🔄 System running - press Ctrl+C to stop');
  } catch (error) {
    console.error('💥 System startup failed:', error);
    process.exit(1);
  }
}

main();