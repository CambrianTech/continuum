/**
 * Simple Process Test - Test basic IPC communication
 */

import { spawn } from 'child_process';

async function testSimpleIPC() {
  console.log('🧪 Testing simple IPC communication...');
  
  const child = spawn('npx', ['tsx', 'src/process/daemons/version/VersionDaemon.ts'], {
    stdio: ['pipe', 'pipe', 'pipe', 'ipc'],
    cwd: process.cwd()
  });

  return new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => {
      child.kill();
      reject(new Error('Test timeout'));
    }, 10000);

    child.on('message', (message) => {
      console.log('📨 Received from child:', message);
      
      if (message.responseId === 'test-ping') {
        console.log('✅ Ping response received!');
        clearTimeout(timeout);
        child.kill();
        resolve();
      }
    });

    child.stdout?.on('data', (data) => {
      console.log('📝 Child stdout:', data.toString().trim());
    });

    child.stderr?.on('data', (data) => {
      console.log('⚠️ Child stderr:', data.toString().trim());
    });

    child.on('error', (error) => {
      console.error('❌ Child error:', error);
      clearTimeout(timeout);
      reject(error);
    });

    // Wait for child to be ready, then send ping
    setTimeout(() => {
      console.log('📤 Sending ping to child...');
      child.send?.({
        responseId: 'test-ping',
        type: 'ping',
        data: {},
        timestamp: Date.now()
      });
    }, 2000);
  });
}

testSimpleIPC()
  .then(() => console.log('✅ Simple IPC test passed'))
  .catch(error => {
    console.error('❌ Simple IPC test failed:', error);
    process.exit(1);
  });