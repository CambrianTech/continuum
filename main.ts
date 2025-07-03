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
  
  // Check if we're running in daemon mode (default) or attached mode
  const isDaemonMode = !process.argv.includes('--attach');
  
  if (isDaemonMode) {
    // In daemon mode, CTRL+C should NOT stop the daemons
    process.on('SIGINT', () => {
      console.log('\n👋 Detaching from Continuum daemons (they continue running)...');
      console.log('💡 To stop daemons: continuum stop');
      console.log('💡 To re-attach: continuum attach');
      process.exit(0);
    });
  } else {
    // In attached mode, graceful shutdown on signals
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
  }

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
    
    if (isDaemonMode) {
      console.log('🎯 Daemons running in background. Press CTRL+C to detach from this session.');
      console.log('');
      
      // In daemon mode, just keep the process alive to show logs
      // but daemons should actually run independently
      // TODO: Implement proper daemon forking/detaching
    } else {
      console.log('📎 Running in attached mode. CTRL+C will stop all daemons.');
      console.log('');
    }
  } catch (error) {
    console.error('💥 System startup failed:', error);
    process.exit(1);
  }
}

main();