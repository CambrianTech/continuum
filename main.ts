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
    
    // Get current session info from SessionManagerDaemon
    const sessionInfo = await system.getCurrentSessionInfo();
    
    console.log('\n╔═════════════════════════════════════════════════════════════════════════════════════╗');
    console.log('║                                🎉 CONTINUUM READY                                  ║');
    console.log('╠═════════════════════════════════════════════════════════════════════════════════════╣');
    console.log('║  🌐 Interface:  http://localhost:9000                                              ║');
    console.log('║  🔄 Status:     Daemons running in background                                      ║');
    console.log('╠═════════════════════════════════════════════════════════════════════════════════════╣');
    
    // Session orchestration successful - remove debug output
    
    if (sessionInfo && sessionInfo.success) {
      const session = sessionInfo.data.session;
      const actionText = session.action === 'created_new' ? '🆕 Created' : 
                        session.action === 'joined_existing' ? '🔗 Joined' : 
                        '🍴 Forked';
      
      console.log(`║  📋 Session:    ${session.id} (${actionText})              ║`);
      console.log(`║  📝 Browser:    ${session.logPaths.browser.padEnd(60)} ║`);
      console.log(`║  🖥️  Server:     ${session.logPaths.server.padEnd(60)} ║`);
      console.log(`║  📸 Screenshots: ${session.directories.screenshots.padEnd(60)} ║`);
      
      if (session.commands) {
        console.log('╠═════════════════════════════════════════════════════════════════════════════════════╣');
        console.log(`║  💡 Commands:   ${session.commands.info.padEnd(60)} ║`);
        console.log(`║                 ${session.commands.stop.padEnd(60)} ║`);
      }
    } else {
      console.log('║  📋 Sessions:   Managed by session-manager daemon                                  ║');
      console.log('║  💡 Use:        session-paths --owner=$(whoami) for log locations                  ║');
    }
    
    console.log('╚═════════════════════════════════════════════════════════════════════════════════════╝\n');
    
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