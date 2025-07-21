#!/usr/bin/env npx tsx
/**
 * Step 2B: Test WebSocket Server with Node.js WebSocket Client
 * 
 * This test verifies:
 * 1. WebSocket server starts correctly
 * 2. Node.js client can connect to WebSocket server
 * 3. Messages can be sent from Node.js client to server
 * 4. Server processes messages correctly
 */

import { JTAGBase } from '../shared/JTAGBase';
import WebSocket from 'ws';
import { JTAGMessageFactory, JTAG_MESSAGE_TYPES, JTAG_CONTEXTS } from '../shared/JTAGTypes';

async function testServerWebSocket() {
  console.log('🧪 Step 2B: Testing WebSocket Server with Node.js Client\n');

  try {
    // Test 1: Initialize JTAG server with WebSocket
    console.log('📋 Test 2B.1: Initialize JTAG server with WebSocket');
    JTAGBase.initialize({
      context: 'server',
      enableConsoleOutput: true,
      enableRemoteLogging: true,
      jtagPort: 9001
    });
    
    // Wait for WebSocket server to start
    await new Promise(resolve => setTimeout(resolve, 3000));
    console.log('✅ JTAG server initialized with WebSocket');

    // Test 2: Test server connection status
    console.log('\n📋 Test 2B.2: Verify JTAG server connection');
    const connection = await JTAGBase.connect({
      healthCheck: true,
      timeout: 5000
    });
    
    console.log('🔗 Connection healthy:', connection.healthy);
    console.log('🚀 Transport type:', connection.transport.type);
    console.log('🎯 Transport state:', connection.transport.state);
    
    if (!connection.healthy) {
      throw new Error('JTAG server connection is not healthy');
    }
    console.log('✅ JTAG server connection verified');

    // Test 3: Create Node.js WebSocket client to connect to JTAG server
    console.log('\n📋 Test 2B.3: Create Node.js WebSocket client');
    
    const wsClient = new WebSocket('ws://localhost:9001');
    
    // Promise wrapper for WebSocket events
    const wsConnected = new Promise<void>((resolve, reject) => {
      wsClient.on('open', () => {
        console.log('✅ Node.js WebSocket client connected to JTAG server');
        resolve();
      });
      
      wsClient.on('error', (error) => {
        console.error('❌ WebSocket connection error:', error);
        reject(error);
      });
      
      setTimeout(() => {
        reject(new Error('WebSocket connection timeout'));
      }, 10000);
    });
    
    await wsConnected;

    // Test 4: Send log message from Node.js client to JTAG server
    console.log('\n📋 Test 2B.4: Send log message from Node.js client to server');
    
    const logMessage = JTAGMessageFactory.createRequest(
      JTAG_MESSAGE_TYPES.LOG,
      JTAG_CONTEXTS.EXTERNAL, // Coming from external Node.js client
      {
        level: 'log',
        message: 'Test log message from Node.js WebSocket client',
        component: 'NODEJS_WS_TEST',
        data: { 
          testId: 'nodejs-ws-001', 
          timestamp: new Date().toISOString(),
          clientType: 'nodejs-websocket'
        }
      }
    );

    // Send message and wait for response
    const messageReceived = new Promise<any>((resolve, reject) => {
      wsClient.on('message', (data) => {
        try {
          const response = JSON.parse(data.toString());
          console.log('📥 Received response from server:', {
            type: response.type,
            success: response.payload?.success,
            id: response.id
          });
          resolve(response);
        } catch (error) {
          reject(error);
        }
      });
      
      setTimeout(() => {
        reject(new Error('Message response timeout'));
      }, 5000);
    });

    wsClient.send(JSON.stringify(logMessage));
    console.log('📤 Sent log message to JTAG server');
    
    const response = await messageReceived;
    console.log('✅ Log message successfully processed by server');

    // Test 5: Test screenshot message
    console.log('\n📋 Test 2B.5: Test screenshot message through WebSocket');
    
    const screenshotMessage = JTAGMessageFactory.createRequest(
      JTAG_MESSAGE_TYPES.SCREENSHOT,
      JTAG_CONTEXTS.EXTERNAL,
      {
        filename: 'nodejs-ws-test-screenshot',
        width: 800,
        height: 600,
        format: 'png',
        metadata: { source: 'nodejs-websocket-test' }
      }
    );

    const screenshotReceived = new Promise<any>((resolve, reject) => {
      let messageHandler = (data: any) => {
        try {
          const response = JSON.parse(data.toString());
          if (response.type === 'response' && response.parentId === screenshotMessage.id) {
            wsClient.off('message', messageHandler);
            resolve(response);
          }
        } catch (error) {
          reject(error);
        }
      };
      
      wsClient.on('message', messageHandler);
      
      setTimeout(() => {
        wsClient.off('message', messageHandler);
        reject(new Error('Screenshot response timeout'));
      }, 8000);
    });

    wsClient.send(JSON.stringify(screenshotMessage));
    console.log('📤 Sent screenshot message to JTAG server');
    
    const screenshotResponse = await screenshotReceived;
    console.log('📸 Screenshot response received:', {
      success: screenshotResponse.payload?.success,
      data: screenshotResponse.payload?.data?.success ? 'Screenshot created' : 'Screenshot failed'
    });
    console.log('✅ Screenshot message successfully processed');

    // Test 6: Clean up
    console.log('\n📋 Test 2B.6: Clean up connections');
    wsClient.close();
    console.log('✅ WebSocket client disconnected');

    console.log('\n🎉 Step 2B Complete: WebSocket server communication works correctly!');
    return true;

  } catch (error) {
    console.error('❌ Step 2B Failed:', error);
    console.error('Error details:', {
      message: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack?.split('\n').slice(0, 3).join('\n') : undefined
    });
    return false;
  }
}

// Run the test
testServerWebSocket().then(success => {
  console.log('\n' + (success ? '🎉 WebSocket server test PASSED' : '❌ WebSocket server test FAILED'));
  process.exit(success ? 0 : 1);
}).catch(error => {
  console.error('💥 WebSocket server test crashed:', error);
  process.exit(1);
});