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
    
    // Listen for session events to get real session info
    const sessionInfo = await new Promise<{ sessionId: string; storageDir: string }>((resolve) => {
      // Set up event listener for session creation
      const onSessionCreated = (event: any) => {
        if (event.type === 'session_created') {
          resolve({
            sessionId: event.sessionId,
            storageDir: event.metadata.storageDir
          });
        }
      };

      // In a real implementation, we'd get the session manager from the daemon registry
      // For now, we'll simulate what happens when a session is created
      setTimeout(() => {
        // Create default CLI session identity
        const identity = {
          starter: 'cli' as const,
          name: process.env.USER || process.env.USERNAME || 'developer',
          user: process.env.USER || process.env.USERNAME || 'developer',
          type: 'development' as const,
          metadata: {
            project: 'continuum',
            branch: process.env.GIT_BRANCH || 'main',
            task: 'development'
          }
        };
        
        // Generate session ID that matches SessionManagerDaemon pattern
        const today = new Date();
        const dateStr = today.getFullYear().toString().slice(-2) + 
                       String(today.getMonth() + 1).padStart(2, '0') + 
                       String(today.getDate()).padStart(2, '0');
        const timeStr = String(today.getHours()).padStart(2, '0') + 
                       String(today.getMinutes()).padStart(2, '0');
        
        const sessionId = `cli-${identity.name}-development-${dateStr}-${timeStr}`;
        const baseDir = process.cwd();
        const storageDir = `${baseDir}/.continuum/sessions/${sessionId}`;
        
        // Simulate session created event
        resolve({ sessionId, storageDir });
      }, 100);
    });
    
    const browserLogPath = `${sessionInfo.storageDir}/logs/browser.log`;
    const serverLogPath = `${sessionInfo.storageDir}/logs/server.log`;
    
    console.log('\n╔═════════════════════════════════════════════════════════════════════════════════════╗');
    console.log('║                                🎉 CONTINUUM READY                                  ║');
    console.log('╠═════════════════════════════════════════════════════════════════════════════════════╣');
    console.log('║  🌐 Interface:  http://localhost:9000                                              ║');
    console.log('║  🔄 Status:     Daemons running in background                                      ║');
    console.log('╠═════════════════════════════════════════════════════════════════════════════════════╣');
    console.log(`║  📊 Browser logs: ${browserLogPath} ║`);
    console.log(`║  📋 Server logs:  ${serverLogPath}  ║`);
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