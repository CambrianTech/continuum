#!/usr/bin/env tsx
/**
 * Daemon System Startup - Main entry point for TypeScript daemon ecosystem
 */

import DaemonManager from './src/core/DaemonManager.js';

async function main() {
  console.log('🌟 Continuum TypeScript Daemon System');
  console.log('====================================');
  
  const daemonManager = new DaemonManager();
  
  // Setup graceful shutdown
  process.on('SIGINT', async () => {
    console.log('\n🛑 Received shutdown signal...');
    await daemonManager.stopAll();
    process.exit(0);
  });
  
  process.on('SIGTERM', async () => {
    console.log('\n🛑 Received termination signal...');
    await daemonManager.stopAll();
    process.exit(0);
  });
  
  // Start the daemon ecosystem
  try {
    await daemonManager.startAll();
    
    // Keep the process alive and show status updates
    setInterval(() => {
      const status = daemonManager.getAllStatus();
      console.log(`\n📊 Daemon Status (${new Date().toLocaleTimeString()}):`);
      for (const [name, info] of Object.entries(status)) {
        const critical = info.critical ? '🔴' : '🟡';
        console.log(`  ${critical} ${name}: ${info.status} (PID: ${info.pid}, restarts: ${info.restartCount})`);
      }
    }, 30000); // Status update every 30 seconds
    
  } catch (error) {
    console.error('❌ Failed to start daemon system:', error);
    process.exit(1);
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch(error => {
    console.error('❌ Daemon system startup failed:', error);
    process.exit(1);
  });
}