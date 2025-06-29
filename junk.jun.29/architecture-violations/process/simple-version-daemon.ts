/**
 * Simple Version Daemon - Basic IPC test
 */

import { readFileSync } from 'fs';
import { join } from 'path';

function getVersion(): string {
  try {
    const packagePath = join(process.cwd(), 'package.json');
    const packageData = JSON.parse(readFileSync(packagePath, 'utf8'));
    return packageData.version || '0.0.0';
  } catch {
    return 'unknown';
  }
}

// Set up IPC message handling
process.on('message', (message: any) => {
  console.log('🔍 Daemon received message:', message);
  
  if (message.type === 'ping') {
    const response = {
      responseId: message.responseId,
      success: true,
      data: { pong: true, timestamp: Date.now() },
      processId: 'simple-version-daemon'
    };
    
    console.log('📤 Daemon sending response:', response);
    process.send?.(response);
  }
  
  if (message.type === 'version') {
    const response = {
      responseId: message.responseId,
      success: true,
      data: { 
        version: getVersion(),
        daemon: 'SimpleVersionDaemon',
        timestamp: Date.now()
      },
      processId: 'simple-version-daemon'
    };
    
    console.log('📤 Daemon sending version response:', response);
    process.send?.(response);
  }
});

// Handle graceful shutdown
process.on('SIGINT', () => {
  console.log('🛑 Daemon shutting down...');
  process.exit(0);
});

process.on('SIGTERM', () => {
  console.log('🛑 Daemon terminating...');
  process.exit(0);
});

console.log('🏷️ Simple Version Daemon started, PID:', process.pid);
console.log('✅ Ready for IPC messages');

// Keep process alive
setInterval(() => {
  // Heartbeat
}, 5000);