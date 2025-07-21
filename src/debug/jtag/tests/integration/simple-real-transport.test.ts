#!/usr/bin/env npx tsx
/**
 * Simple Real Transport Test
 * 
 * This test assumes npm start is running with:
 * - JTAG server on port 9001
 * - Browser page connected to that server
 * 
 * The test will create actual transport connections and send real messages
 * that should appear in the browser Network panel.
 */

import { JTAGWebSocketTransportImpl } from '../../shared/transports/WebSocketTransport';
import { JTAGHTTPTransportImpl } from '../../shared/transports/HTTPTransport';
import { JTAGRESTTransportImpl } from '../../shared/transports/RESTTransport';
import { JTAG_STATUS, JTAGConfig, JTAGWebSocketMessage } from '../../shared/JTAGTypes';
import { jtag } from '../../index';

class SimpleRealTransportTester {
  
  async testRealTransports(): Promise<void> {
    console.log('\n🚀 Simple Real Transport Test');
    console.log('============================');
    console.log('Testing actual transport connections to running JTAG server');
    console.log('These messages should appear in browser Network panel if connected\n');

    jtag.test('SIMPLE_REAL_TEST', 'Starting simple real transport tests');

    // Test WebSocket transport (most likely to work with npm start)
    await this.testWebSocketTransport();
    
    // Test HTTP transport
    await this.testHTTPTransport();
    
    // Test REST transport
    await this.testRESTTransport();
    
    jtag.test('SIMPLE_REAL_TEST', 'Simple real transport tests completed');
    console.log('\n✅ Real transport tests completed');
    console.log('🔍 Check browser Network panel for WebSocket/HTTP traffic on port 9001');
  }

  private async testWebSocketTransport(): Promise<void> {
    console.log('📡 Testing WebSocket Transport...');
    jtag.test('WEBSOCKET_TEST', 'Starting WebSocket transport test');

    try {
      const transport = new JTAGWebSocketTransportImpl();
      transport.enableTestMode();

      // Track messages
      const receivedMessages: any[] = [];
      transport.onMessage((message) => {
        receivedMessages.push(message);
        jtag.test('WEBSOCKET_RECEIVED', 'WebSocket received message', message);
      });

      const config: JTAGConfig = {
        context: 'browser', // Simulate browser client
        jtagPort: 9001,    // Connect to real JTAG server
        enableRemoteLogging: true,
        enableConsoleOutput: false,
        maxBufferSize: 100
      };

      console.log('  🔌 Connecting to WebSocket server on port 9001...');
      const connected = await transport.initialize(config);
      
      if (!connected) {
        console.log('  ❌ WebSocket connection failed');
        jtag.test('WEBSOCKET_FAILED', 'WebSocket connection failed');
        return;
      }

      await transport.waitForStatus(JTAG_STATUS.READY, 3000);
      console.log('  ✅ WebSocket connected');
      jtag.test('WEBSOCKET_CONNECTED', 'WebSocket successfully connected');

      // Send real test messages
      const testMessages = [
        {
          type: 'log' as const,
          payload: {
            component: 'REAL_WEBSOCKET_TEST',
            message: 'Real WebSocket message 1',
            timestamp: new Date().toISOString()
          },
          timestamp: new Date().toISOString(),
          messageId: 'ws-real-1-' + Date.now()
        },
        {
          type: 'log' as const,
          payload: {
            component: 'REAL_WEBSOCKET_TEST',
            message: 'Real WebSocket message 2',
            timestamp: new Date().toISOString()
          },
          timestamp: new Date().toISOString(),
          messageId: 'ws-real-2-' + Date.now()
        }
      ];

      console.log('  📤 Sending real messages...');
      for (const message of testMessages) {
        const response = await transport.send(message);
        
        if (response.success) {
          console.log(`  ✅ Message sent: ${message.messageId}`);
          jtag.test('WEBSOCKET_MESSAGE_SENT', 'WebSocket message sent successfully', { 
            messageId: message.messageId 
          });
        } else {
          console.log(`  ❌ Message failed: ${response.error}`);
          jtag.test('WEBSOCKET_MESSAGE_FAILED', 'WebSocket message failed', { 
            error: response.error 
          });
        }
      }

      // Wait for potential responses
      await new Promise(resolve => setTimeout(resolve, 2000));

      console.log(`  📨 Responses received: ${receivedMessages.length}`);
      jtag.test('WEBSOCKET_TEST_COMPLETE', 'WebSocket test completed', {
        messagesSent: testMessages.length,
        responsesReceived: receivedMessages.length
      });

      await transport.disconnect();
      console.log('  🔌 WebSocket disconnected');

    } catch (error: any) {
      console.log(`  ❌ WebSocket test error: ${error.message}`);
      jtag.test('WEBSOCKET_ERROR', 'WebSocket test error', { error: error.message });
    }
  }

  private async testHTTPTransport(): Promise<void> {
    console.log('\n📡 Testing HTTP Transport...');
    jtag.test('HTTP_TEST', 'Starting HTTP transport test');

    try {
      const transport = new JTAGHTTPTransportImpl();
      transport.enableTestMode();

      const config: JTAGConfig = {
        context: 'browser',
        jtagPort: 9001,
        enableRemoteLogging: true,
        enableConsoleOutput: false,
        maxBufferSize: 100
      };

      console.log('  🔌 Initializing HTTP transport...');
      const connected = await transport.initialize(config);
      
      if (!connected) {
        console.log('  ❌ HTTP transport initialization failed');
        return;
      }

      console.log('  ✅ HTTP transport initialized');

      // Send HTTP test message
      const testMessage: JTAGWebSocketMessage = {
        type: 'log',
        payload: {
          component: 'REAL_HTTP_TEST',
          message: 'Real HTTP transport message',
          timestamp: new Date().toISOString()
        },
        timestamp: new Date().toISOString(),
        messageId: 'http-real-' + Date.now()
      };

      console.log('  📤 Sending HTTP message...');
      const response = await transport.send(testMessage);
      
      if (response.success) {
        console.log(`  ✅ HTTP message sent successfully`);
        jtag.test('HTTP_MESSAGE_SENT', 'HTTP message sent successfully');
      } else {
        console.log(`  ❌ HTTP message failed: ${response.error}`);
        jtag.test('HTTP_MESSAGE_FAILED', 'HTTP message failed', { error: response.error });
      }

      await transport.disconnect();
      console.log('  🔌 HTTP transport disconnected');

    } catch (error: any) {
      console.log(`  ❌ HTTP test error: ${error.message}`);
      jtag.test('HTTP_ERROR', 'HTTP test error', { error: error.message });
    }
  }

  private async testRESTTransport(): Promise<void> {
    console.log('\n📡 Testing REST Transport...');
    jtag.test('REST_TEST', 'Starting REST transport test');

    try {
      const transport = new JTAGRESTTransportImpl();
      transport.enableTestMode();

      const config: JTAGConfig = {
        context: 'browser',
        jtagPort: 9001,
        enableRemoteLogging: true,
        enableConsoleOutput: false,
        maxBufferSize: 100
      };

      console.log('  🔌 Initializing REST transport...');
      const connected = await transport.initialize(config);
      
      if (!connected) {
        console.log('  ❌ REST transport initialization failed');
        return;
      }

      console.log('  ✅ REST transport initialized');

      // Send REST test message
      const testMessage: JTAGWebSocketMessage = {
        type: 'log',
        payload: {
          component: 'REAL_REST_TEST',
          message: 'Real REST API message',
          timestamp: new Date().toISOString()
        },
        timestamp: new Date().toISOString(),
        messageId: 'rest-real-' + Date.now()
      };

      console.log('  📤 Sending REST message...');
      const response = await transport.send(testMessage);
      
      if (response.success) {
        console.log(`  ✅ REST message sent successfully`);
        jtag.test('REST_MESSAGE_SENT', 'REST message sent successfully');
      } else {
        console.log(`  ❌ REST message failed: ${response.error}`);
        jtag.test('REST_MESSAGE_FAILED', 'REST message failed', { error: response.error });
      }

      await transport.disconnect();
      console.log('  🔌 REST transport disconnected');

    } catch (error: any) {
      console.log(`  ❌ REST test error: ${error.message}`);
      jtag.test('REST_ERROR', 'REST test error', { error: error.message });
    }
  }
}

// Run simple real transport tests
async function runSimpleRealTests() {
  const tester = new SimpleRealTransportTester();
  
  try {
    await tester.testRealTransports();
  } catch (error: any) {
    jtag.test('SIMPLE_REAL_ERROR', 'Simple real test failed', { 
      error: error.message 
    });
    console.error('💥 Simple real transport test failed:', error.message);
    process.exit(1);
  }
}

runSimpleRealTests();