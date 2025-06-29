#!/usr/bin/env tsx
/**
 * Daemon System Startup - Main entry point for TypeScript daemon ecosystem
 * 
 * 🎫 ISSUE TICKETS (Priority: HIGH → LOW):
 * 
 * #001 [HIGH] Replace polling with event-driven status
 *   - PROBLEM: setInterval every 30s is anti-pattern causing log flood
 *   - SOLUTION: Use daemon.on('status_change') + EventEmitter architecture
 *   - FILES: start-daemons.ts:44-51, DaemonManager.ts
 *   - BLOCKED_BY: None
 * 
 * #002 [HIGH] Fix daemon registration system  
 *   - PROBLEM: DaemonManager.ts:95-99 has commented registration code
 *   - SOLUTION: Implement webSocketDaemon.registerExternalDaemon()
 *   - FILES: DaemonManager.ts, WebSocketDaemon.ts
 *   - BLOCKED_BY: #003
 * 
 * #003 [MED] Separate DaemonManager concerns
 *   - PROBLEM: God object handling spawn + registration + status
 *   - SOLUTION: Split into DaemonSpawner, DaemonRegistry, StatusManager  
 *   - FILES: DaemonManager.ts
 *   - BLOCKED_BY: None
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
    
    // TODO: FLAWED DESIGN - Replace with event-driven status
    // This setInterval is architectural anti-pattern - should be:
    // - daemon.on('status_change', updateStatus) 
    // - process.on('message', handleDaemonMessage)
    // - EventEmitter-based status propagation
    // 
    // TEMPORARY: Disabled to prevent log flooding
    // setInterval(() => {
    //   const status = daemonManager.getAllStatus();
    //   console.log(`\n📊 Daemon Status (${new Date().toLocaleTimeString()}):`);
    //   for (const [name, info] of Object.entries(status)) {
    //     const critical = info.critical ? '🔴' : '🟡';
    //     console.log(`  ${critical} ${name}: ${info.status} (PID: ${info.pid}, restarts: ${info.restartCount})`);
    //   }
    // }, 30000);
    
    // Keep process alive with minimal logging
    console.log('✅ Daemon system running - status polling disabled to prevent log flood');
    console.log('🔧 TODO: Implement event-driven status updates instead of polling');
    
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