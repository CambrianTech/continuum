#!/usr/bin/env npx tsx
/**
 * WebSocket Bi-directional Flow Test
 * Tests actual message sending back and forth with real WebSocket server
 * Uses JTAG logging system for test output
 */

import { JTAGWebSocketTransportImpl } from '@shared/transports/WebSocketTransport';
import { JTAG_STATUS, JTAGConfig } from '@shared/JTAGTypes';
import { jtag } from '../../index';
import * as http from 'http';
import * as WebSocket from 'ws';

interface TestMessage {
  type: 'log';
  payload: {
    component: string;
    message: string;
    data?: any;
  };
  timestamp: string;
  messageId: string;
}

interface TestResponse {
  success: boolean;
  messageId: string;
  echo: TestMessage;
  serverTime: string;
}

class WebSocketBidirectionalTester {
  private server?: http.Server;
  private wss?: WebSocket.WebSocketServer;
  private port = 9003; // Different port to avoid conflicts
  private receivedMessages: TestMessage[] = [];
  private sentResponses: TestResponse[] = [];

  async runBidirectionalTest(): Promise<void> {
    // Initialize JTAG for logging  
    jtag.log('TEST_FRAMEWORK', '🔄 WebSocket Bi-directional Flow Test');
    jtag.log('TEST_FRAMEWORK', '=====================================');

    try {
      // 1. Start WebSocket server
      await this.startWebSocketServer();
      jtag.log('WEBSOCKET_SERVER', '✅ WebSocket server started on port ' + this.port);

      // 2. Test transport connection and message flow
      await this.testTransportMessageFlow();

      // 3. Verify bi-directional communication
      await this.verifyBidirectionalFlow();

      jtag.log('TEST_FRAMEWORK', '🎉 Bi-directional WebSocket flow test PASSED!');
      jtag.log('TEST_FRAMEWORK', `📨 Messages received: ${this.receivedMessages.length}`);
      jtag.log('TEST_FRAMEWORK', `📤 Responses sent: ${this.sentResponses.length}`);

    } finally {
      await this.cleanup();
    }
  }

  private async startWebSocketServer(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.server = http.createServer();
      this.wss = new WebSocket.WebSocketServer({ server: this.server });

      this.wss.on('connection', (ws) => {
        jtag.log('WEBSOCKET_SERVER', '📡 Client connected to test server');

        ws.on('message', (data) => {
          try {
            const message: TestMessage = JSON.parse(data.toString());
            jtag.log('WEBSOCKET_SERVER', '📨 Server received: ' + message.type + ' - ' + message.payload.message);
            
            this.receivedMessages.push(message);

            // Echo back with server response
            const response: TestResponse = {
              success: true,
              messageId: message.messageId,
              echo: message,
              serverTime: new Date().toISOString()
            };

            ws.send(JSON.stringify(response));
            this.sentResponses.push(response);
            
            jtag.log('WEBSOCKET_SERVER', '📤 Server responded to: ' + message.messageId);
          } catch (error: any) {
            jtag.error('WEBSOCKET_SERVER', '❌ Server message parse error: ' + error.message);
          }
        });

        ws.on('close', () => {
          jtag.log('WEBSOCKET_SERVER', '🔌 Client disconnected from test server');
        });
      });

      this.server.listen(this.port, () => {
        jtag.log('WEBSOCKET_SERVER', `🚀 Test WebSocket server listening on port ${this.port}`);
        resolve();
      });

      this.server.on('error', reject);
    });
  }

  private async testTransportMessageFlow(): Promise<void> {
    jtag.log('TRANSPORT_TEST', '🔧 Testing transport message flow...');

    const transport = new JTAGWebSocketTransportImpl();
    
    // Enable test mode and track status events
    transport.enableTestMode();
    
    const config: JTAGConfig = {
      context: 'browser',
      jtagPort: this.port,
      enableRemoteLogging: true,
      enableConsoleOutput: false,
      maxBufferSize: 100
    };

    // Track received responses
    const receivedResponses: any[] = [];
    transport.onMessage((response) => {
      jtag.log('TRANSPORT_TEST', '📨 Transport received response: ' + JSON.stringify(response));
      receivedResponses.push(response);
    });

    jtag.log('TRANSPORT_TEST', '🔌 Connecting transport to server...');
    const connected = await transport.initialize(config);
    
    if (!connected) {
      throw new Error('Transport failed to connect');
    }

    // Wait for READY status
    await transport.waitForStatus(JTAG_STATUS.READY, 3000);
    jtag.log('TRANSPORT_TEST', '✅ Transport connected and ready');

    // Send test messages
    const testMessages: TestMessage[] = [
      {
        type: 'log',
        payload: {
          component: 'TEST_COMPONENT_1',
          message: 'Test message 1 from transport',
          data: { testId: 'msg1' }
        },
        timestamp: new Date().toISOString(),
        messageId: 'test-msg-1-' + Date.now()
      },
      {
        type: 'log',
        payload: {
          component: 'TEST_COMPONENT_2', 
          message: 'Test message 2 from transport',
          data: { testId: 'msg2' }
        },
        timestamp: new Date().toISOString(),
        messageId: 'test-msg-2-' + Date.now()
      }
    ];

    jtag.log('TRANSPORT_TEST', '📤 Sending test messages...');
    for (const message of testMessages) {
      const response = await transport.send(message);
      
      if (!response.success) {
        throw new Error(`Message send failed: ${response.error}`);
      }
      
      jtag.log('TRANSPORT_TEST', '✅ Message sent successfully: ' + message.messageId);
    }

    // Wait for responses
    jtag.log('TRANSPORT_TEST', '⏳ Waiting for server responses...');
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Disconnect
    jtag.log('TRANSPORT_TEST', '🔌 Disconnecting transport...');
    await transport.disconnect();
    
    jtag.log('TRANSPORT_TEST', `✅ Transport test completed. Received ${receivedResponses.length} responses`);
  }

  private async verifyBidirectionalFlow(): Promise<void> {
    jtag.log('FLOW_VERIFICATION', '🔍 Verifying bi-directional flow...');

    // Verify server received messages
    if (this.receivedMessages.length === 0) {
      throw new Error('Server did not receive any messages');
    }
    jtag.log('FLOW_VERIFICATION', '✅ Server received messages: ' + this.receivedMessages.length);

    // Verify server sent responses  
    if (this.sentResponses.length === 0) {
      throw new Error('Server did not send any responses');
    }
    jtag.log('FLOW_VERIFICATION', '✅ Server sent responses: ' + this.sentResponses.length);

    // Verify message content
    for (const message of this.receivedMessages) {
      if (!message.type || !message.payload || !message.messageId) {
        throw new Error('Invalid message structure received by server');
      }
    }
    jtag.log('FLOW_VERIFICATION', '✅ Message structure validation passed');

    // Verify response content
    for (const response of this.sentResponses) {
      if (!response.success || !response.messageId || !response.echo) {
        throw new Error('Invalid response structure sent by server');
      }
    }
    jtag.log('FLOW_VERIFICATION', '✅ Response structure validation passed');

    jtag.log('FLOW_VERIFICATION', '✅ Bi-directional flow verification completed');
  }

  private async cleanup(): Promise<void> {
    if (this.wss) {
      this.wss.close();
    }
    if (this.server) {
      await new Promise<void>((resolve) => {
        this.server!.close(() => resolve());
      });
    }
    jtag.log('TEST_FRAMEWORK', '🧹 Test server cleanup completed');
  }
}

// Run the test
const tester = new WebSocketBidirectionalTester();
tester.runBidirectionalTest().catch(error => {
  jtag.error('TEST_FRAMEWORK', '💥 Bi-directional test failed: ' + error.message);
  process.exit(1);
});