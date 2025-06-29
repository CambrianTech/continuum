/**
 * Simple Process Test 2 - Test with simplified daemon
 */

import { spawn } from 'child_process';

async function testSimpleIPC() {
  console.log('🧪 Testing simple IPC with basic daemon...');
  
  const child = spawn('npx', ['tsx', 'src/process/simple-version-daemon.ts'], {
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
      
      if (message.responseId === 'test-ping' && message.data?.pong) {
        console.log('✅ Ping response received!');
        
        // Now test version command
        console.log('📤 Sending version request...');
        child.send?.({
          responseId: 'test-version',
          type: 'version',
          data: {},
          timestamp: Date.now()
        });
      }
      
      if (message.responseId === 'test-version' && message.data?.version) {
        console.log('✅ Version response received:', message.data);
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

    child.on('exit', (code, signal) => {
      console.log(`📋 Child exited: code=${code}, signal=${signal}`);
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
  .then(() => console.log('🎉 Complete IPC test passed!'))
  .catch(error => {
    console.error('❌ IPC test failed:', error);
    process.exit(1);
  });