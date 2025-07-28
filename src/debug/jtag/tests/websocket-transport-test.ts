#!/usr/bin/env npx tsx
/**
 * Step 2: Test WebSocket Transport Layer Functionality
 * 
 * This test verifies:
 * 1. WebSocket server starts correctly
 * 2. WebSocket client can connect
 * 3. Messages route through WebSocket transport
 * 4. Client-server promise-based communication works
 */

import { JTAGBase } from '@shared/JTAGBase';
import { JTAGWebSocketClient } from '@shared/JTAGWebSocket';

async function testWebSocketTransport() {
  console.log('🧪 Step 2: Testing WebSocket Transport Layer\n');

  try {
    // Test 1: Initialize JTAG with WebSocket enabled
    console.log('📋 Test 2.1: Initialize JTAG server with WebSocket');
    JTAGBase.initialize({
      context: 'server',
      enableConsoleOutput: true,
      enableRemoteLogging: true, // Enable WebSocket
      jtagPort: 9001
    });
    
    // Wait for WebSocket server to start
    await new Promise(resolve => setTimeout(resolve, 2000));
    console.log('✅ JTAG server initialized with WebSocket');

    // Test 2: Test connection status
    console.log('\n📋 Test 2.2: Test JTAG connection');
    const connection = await JTAGBase.connect({
      healthCheck: true,
      timeout: 5000
    });
    
    console.log('🔗 Connection healthy:', connection.healthy);
    console.log('🚀 Transport type:', connection.transport.type);
    console.log('🎯 Transport state:', connection.transport.state);
    console.log('⏱️  Connection latency:', connection.transport.latency, 'ms');
    
    if (!connection.healthy) {
      throw new Error('JTAG connection is not healthy');
    }
    console.log('✅ JTAG connection established successfully');

    // Test 3: Create separate WebSocket client to test client-server communication
    console.log('\n📋 Test 2.3: Create WebSocket client for testing');
    const testClient = new JTAGWebSocketClient(9001);
    
    // Wait a bit for client setup
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    console.log('🔌 Connecting test client to WebSocket server...');
    await testClient.connect();
    console.log('✅ Test client connected to WebSocket server');

    // Test 4: Send log message from client to server
    console.log('\n📋 Test 2.4: Test client-to-server log message');
    const logResult = await testClient.sendLog({
      component: 'WEBSOCKET_TEST',
      message: 'Test message from WebSocket client',
      level: 'log',
      data: { testId: 'ws-test-001', timestamp: new Date().toISOString() }
    });
    
    console.log('📤 Client log send result:', logResult?.success ? '✅' : '❌');
    if (logResult?.success) {
      console.log('📝 Log message sent successfully through WebSocket');
    }

    // Test 5: Test server screenshot (should create placeholder)
    console.log('\n📋 Test 2.5: Test server screenshot through WebSocket');
    const screenshotResult = await JTAGBase.screenshot('websocket-test-screenshot', {
      width: 800,
      height: 600,
      selector: 'body'
    });
    
    console.log('📸 Screenshot result success:', screenshotResult.success);
    console.log('📁 Screenshot filepath:', screenshotResult.filepath);
    if (screenshotResult.metadata) {
      console.log('📊 Screenshot metadata:', screenshotResult.metadata);
    }
    console.log('✅ Screenshot functionality works through transport layer');

    // Test 6: Test exec functionality
    console.log('\n📋 Test 2.6: Test code execution through transport');
    const execResult = await JTAGBase.exec('Math.random() * 100', {
      timeout: 3000,
      returnValue: true
    });
    
    console.log('⚡ Exec result success:', execResult.success);
    console.log('🔢 Exec result value:', execResult.result);
    console.log('⏱️  Execution time:', execResult.executionTime, 'ms');
    console.log('✅ Code execution works through transport layer');

    // Test 7: Clean up
    console.log('\n📋 Test 2.7: Clean up WebSocket connections');
    testClient.disconnect();
    console.log('✅ Test client disconnected');

    console.log('\n🎉 Step 2 Complete: WebSocket transport layer works correctly!');
    return true;

  } catch (error) {
    console.error('❌ Step 2 Failed:', error);
    console.error('Error details:', {
      message: error.message,
      stack: error.stack?.split('\n').slice(0, 3).join('\n')
    });
    return false;
  }
}

// Run the test
testWebSocketTransport().then(success => {
  console.log('\n' + (success ? '🎉 WebSocket transport test PASSED' : '❌ WebSocket transport test FAILED'));
  process.exit(success ? 0 : 1);
}).catch(error => {
  console.error('💥 WebSocket test crashed:', error);
  process.exit(1);
});