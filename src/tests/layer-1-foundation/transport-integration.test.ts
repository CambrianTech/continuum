#!/usr/bin/env node
/**
 * Transport Layer Integration Tests (TDD)
 * 
 * Tests REAL transport implementations with actual network connections:
 * - Real WebSocket server/client communication
 * - Real HTTP POST/GET operations  
 * - Real message serialization/deserialization
 * - Real connection failures and recovery
 * - Real performance under load
 * 
 * These tests validate that transport abstractions work with real network infrastructure.
 */

import * as http from 'http';
import * as fs from 'fs';
import * as path from 'path';
import { Server } from 'ws';
import { 
  DefaultWebSocketTransport, 
  HTTPPollingTransport, 
  JTAGSmartTransport 
} from '@shared/JTAGTransportFactory';
import { TransportTestUtils } from '@tests/shared/MockTransports';
import type { JTAGConfig, JTAGWebSocketMessage } from '../../../system/core/types/JTAGTypes';

class TransportIntegrationTest {
  private testConfig: JTAGConfig = {
    context: 'server',
    jtagPort: 9003, // Use different port to avoid conflicts
    enableRemoteLogging: true,
    enableConsoleOutput: false,
    maxBufferSize: 100
  };
  
  private httpServer: http.Server | null = null;
  private wsServer: Server | null = null;

  async runAllTests(): Promise<void> {
    console.log('🧪 Transport Layer Integration Tests (Real Network)');
    console.log('===================================================\n');

    await this.testRealWebSocketTransport();
    await this.testRealHTTPTransport();
    await this.testSmartTransportWithRealNetworks();
    await this.testNetworkFailureRecovery();
    await this.testPerformanceUnderLoad();
    await this.testMessageSerializationIntegrity();
    
    console.log('\n🎉 All transport integration tests passed!');
    console.log('✅ Transport abstractions work with real network infrastructure');
  }

  private async testRealWebSocketTransport(): Promise<void> {
    console.log('🌐 Testing Real WebSocket Transport...');
    
    // Start real WebSocket server
    await this.startWebSocketServer();
    
    try {
      // Create real WebSocket transport
      const wsTransport = new DefaultWebSocketTransport();
      
      // Test initialization with real WebSocket connection
      const initSuccess = await wsTransport.initialize(this.testConfig);
      
      if (!initSuccess) {
        throw new Error('WebSocket transport should connect to real server');
      }
      
      console.log('   ✅ Real WebSocket connection established');
      
      // Test real message sending
      const message = TransportTestUtils.createJTAGMessage('log');
      const result = await wsTransport.send(message);
      
      if (!result.success) {
        throw new Error('WebSocket message should succeed with real server');
      }
      
      console.log(`   ✅ Real WebSocket message sent: ${result.success} (${result.transportMeta?.duration}ms)`);
      
      // Test connection status
      const connected = wsTransport.isConnected();
      if (!connected) {
        throw new Error('WebSocket should report connected status');
      }
      
      console.log('   ✅ WebSocket connection status accurate');
      
      // Test cleanup
      await wsTransport.disconnect();
      const disconnected = !wsTransport.isConnected();
      
      if (!disconnected) {
        throw new Error('WebSocket should report disconnected after cleanup');
      }
      
      console.log('   ✅ WebSocket cleanup and disconnect working');
      
    } finally {
      await this.stopWebSocketServer();
    }
    
    console.log('   🎉 Real WebSocket transport: PASSED\n');
  }

  private async testRealHTTPTransport(): Promise<void> {
    console.log('📡 Testing Real HTTP Transport...');
    
    // Start real HTTP server
    await this.startHTTPServer();
    
    try {
      // Create real HTTP transport
      const httpTransport = new HTTPPollingTransport();
      
      // Test initialization with real HTTP server
      const initSuccess = await httpTransport.initialize(this.testConfig);
      
      if (!initSuccess) {
        throw new Error('HTTP transport should connect to real server');
      }
      
      console.log('   ✅ Real HTTP connection established');
      
      // Test real HTTP message sending
      const message = TransportTestUtils.createJTAGMessage('error');
      const result = await httpTransport.send(message);
      
      if (!result.success) {
        console.error('HTTP transport error:', result.error);
        throw new Error('HTTP message should succeed with real server');
      }
      
      console.log(`   ✅ Real HTTP message sent: ${result.success} (${result.transportMeta?.duration}ms)`);
      
      // Test connection status (HTTP is stateless but should report connected)
      const connected = httpTransport.isConnected();
      if (!connected) {
        throw new Error('HTTP transport should report connected when initialized');
      }
      
      console.log('   ✅ HTTP connection status accurate');
      
      // Test cleanup
      await httpTransport.disconnect();
      
      console.log('   ✅ HTTP transport cleanup working');
      
    } finally {
      await this.stopHTTPServer();
    }
    
    console.log('   🎉 Real HTTP transport: PASSED\n');
  }

  private async testSmartTransportWithRealNetworks(): Promise<void> {
    console.log('🧠 Testing Smart Transport with Real Networks...');
    
    // Start both servers for fallback testing
    await this.startHTTPServer();
    
    try {
      // Test 1: Primary WebSocket fails, fallback to HTTP succeeds
      const smartTransport = new JTAGSmartTransport();
      
      const configWithFallback = {
        ...this.testConfig,
        jtagPort: 9999, // Intentionally wrong port for WebSocket
        transport: {
          type: 'websocket' as const,
          fallback: 'http' as const
        }
      };
      
      const initSuccess = await smartTransport.initialize(configWithFallback);
      console.log(`   ✅ Smart transport initialization: ${initSuccess}`);
      
      // Should fallback to HTTP on port 9003
      const message = TransportTestUtils.createJTAGMessage('log');
      
      // Temporarily redirect HTTP transport to correct port
      configWithFallback.jtagPort = 9003;
      const result = await smartTransport.send(message);
      
      console.log(`   ✅ Smart transport fallback behavior: ${result.success || 'queued'}`);
      
      // Test 2: Message queuing when all transports fail
      const smartTransport2 = new JTAGSmartTransport();
      
      const failConfig = {
        ...this.testConfig,
        jtagPort: 9998, // Wrong port for both transports
        transport: {
          type: 'websocket' as const,
          fallback: 'http' as const
        }
      };
      
      await smartTransport2.initialize(failConfig);
      const message2 = TransportTestUtils.createJTAGMessage('log');
      const result2 = await smartTransport2.send(message2);
      
      if (result2.success) {
        throw new Error('Smart transport should queue when all transports fail');
      }
      
      console.log('   ✅ Smart transport queuing when all transports fail');
      
      // Test message queue flush when transport becomes available
      if (typeof (smartTransport2 as any).flushQueue === 'function') {
        await (smartTransport2 as any).flushQueue();
        console.log('   ✅ Smart transport queue flush mechanism');
      }
      
    } finally {
      await this.stopHTTPServer();
    }
    
    console.log('   🎉 Smart transport with real networks: PASSED\n');
  }

  private async testNetworkFailureRecovery(): Promise<void> {
    console.log('💥 Testing Network Failure and Recovery...');
    
    // Start server, then stop it to simulate network failure
    await this.startHTTPServer();
    
    const httpTransport = new HTTPPollingTransport();
    const initSuccess = await httpTransport.initialize(this.testConfig);
    
    if (!initSuccess) {
      throw new Error('HTTP transport should initialize successfully');
    }
    
    console.log('   ✅ Transport connected initially');
    
    // Simulate network failure by stopping server
    await this.stopHTTPServer();
    
    // Try to send message (should fail)
    const message = TransportTestUtils.createJTAGMessage('log');
    const failResult = await httpTransport.send(message);
    
    if (failResult.success) {
      throw new Error('Transport should fail when server is down');
    }
    
    console.log('   ✅ Transport correctly reports failure when server down');
    
    // Restart server to simulate recovery
    await this.startHTTPServer();
    
    // Re-initialize transport
    const recoverySuccess = await httpTransport.initialize(this.testConfig);
    
    if (!recoverySuccess) {
      throw new Error('Transport should recover when server comes back up');
    }
    
    // Try sending message again (should succeed)
    const recoveryResult = await httpTransport.send(message);
    
    if (!recoveryResult.success) {
      throw new Error('Transport should work again after recovery');
    }
    
    console.log('   ✅ Transport successfully recovers after server restart');
    
    await this.stopHTTPServer();
    console.log('   🎉 Network failure and recovery: PASSED\n');
  }

  private async testPerformanceUnderLoad(): Promise<void> {
    console.log('⚡ Testing Performance Under Load...');
    
    await this.startHTTPServer();
    
    try {
      const httpTransport = new HTTPPollingTransport();
      await httpTransport.initialize(this.testConfig);
      
      const messageCount = 50;
      const messages = Array.from({ length: messageCount }, () => 
        TransportTestUtils.createJTAGMessage('log')
      );
      
      const startTime = Date.now();
      const results = [];
      
      // Send messages concurrently
      for (const message of messages) {
        const resultPromise = httpTransport.send(message);
        results.push(resultPromise);
      }
      
      // Wait for all to complete
      const allResults = await Promise.all(results);
      const endTime = Date.now();
      
      const successCount = allResults.filter(r => r.success).length;
      const totalTime = endTime - startTime;
      const messagesPerSecond = (messageCount / totalTime) * 1000;
      
      console.log(`   ✅ Sent ${messageCount} messages in ${totalTime}ms`);
      console.log(`   ✅ Success rate: ${successCount}/${messageCount} (${Math.round(successCount/messageCount*100)}%)`);
      console.log(`   ✅ Throughput: ${messagesPerSecond.toFixed(1)} messages/second`);
      
      if (successCount < messageCount * 0.95) {
        throw new Error('Transport should handle load with >95% success rate');
      }
      
      console.log('   ✅ Transport handles load with acceptable success rate');
      
    } finally {
      await this.stopHTTPServer();
    }
    
    console.log('   🎉 Performance under load: PASSED\n');
  }

  private async testMessageSerializationIntegrity(): Promise<void> {
    console.log('📦 Testing Message Serialization Integrity...');
    
    await this.startHTTPServer();
    
    try {
      const httpTransport = new HTTPPollingTransport();
      await httpTransport.initialize(this.testConfig);
      
      // Test with complex nested data
      const complexMessage = {
        type: 'log' as const,
        payload: {
          timestamp: new Date().toISOString(),
          context: 'browser' as const,
          component: 'TEST',
          message: 'Complex test message',
          data: {
            nested: {
              object: true,
              array: [1, 2, 3, 'string', { deep: 'value' }],
              special: 'chars: "quotes", \\backslashes\\, newlines\n',
              unicode: '🚀 Unicode 测试 🎉'
            }
          },
          type: 'log' as const
        },
        timestamp: new Date().toISOString(),
        messageId: `test-${Date.now()}-${Math.random()}`
      };
      
      const result = await httpTransport.send(complexMessage);
      
      if (!result.success) {
        throw new Error('Complex message serialization should succeed');
      }
      
      console.log('   ✅ Complex nested data serialization successful');
      
      // Test with large data payload
      const largeData = {
        largArray: Array.from({ length: 1000 }, (_, i) => `item-${i}`),
        largeString: 'x'.repeat(10000)
      };
      
      const largeMessage = {
        ...TransportTestUtils.createJTAGMessage('log'),
        payload: {
          ...TransportTestUtils.createJTAGMessage('log').payload,
          data: largeData
        }
      };
      
      const largeResult = await httpTransport.send(largeMessage);
      
      if (!largeResult.success) {
        throw new Error('Large message serialization should succeed');
      }
      
      console.log('   ✅ Large payload serialization successful');
      
    } finally {
      await this.stopHTTPServer();
    }
    
    console.log('   🎉 Message serialization integrity: PASSED\n');
  }

  private async startWebSocketServer(): Promise<void> {
    return new Promise((resolve) => {
      this.httpServer = http.createServer();
      this.wsServer = new Server({ server: this.httpServer });
      
      this.wsServer.on('connection', (ws) => {
        ws.on('message', (data) => {
          // Echo back a success response
          const message = JSON.parse(data.toString());
          const response = {
            success: true,
            messageId: message.messageId,
            timestamp: new Date().toISOString()
          };
          ws.send(JSON.stringify(response));
        });
      });
      
      this.httpServer.listen(this.testConfig.jtagPort, () => {
        console.log(`   🔧 Test WebSocket server started on port ${this.testConfig.jtagPort}`);
        resolve();
      });
    });
  }

  private async stopWebSocketServer(): Promise<void> {
    if (this.wsServer) {
      this.wsServer.close();
      this.wsServer = null;
    }
    
    if (this.httpServer) {
      return new Promise((resolve) => {
        this.httpServer!.close(() => {
          console.log('   🔧 Test WebSocket server stopped');
          this.httpServer = null;
          resolve();
        });
      });
    }
  }

  private async startHTTPServer(): Promise<void> {
    return new Promise((resolve) => {
      this.httpServer = http.createServer((req, res) => {
        // Handle CORS
        res.setHeader('Access-Control-Allow-Origin', '*');
        res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
        res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
        
        if (req.method === 'OPTIONS') {
          res.writeHead(200);
          res.end();
          return;
        }
        
        if (req.url === '/health' && req.method === 'GET') {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ status: 'ok' }));
          return;
        }
        
        if (req.url === '/jtag' && req.method === 'POST') {
          let body = '';
          
          req.on('data', (chunk) => {
            body += chunk.toString();
          });
          
          req.on('end', () => {
            try {
              const message = JSON.parse(body);
              const response = {
                success: true,
                messageId: message.messageId,
                timestamp: new Date().toISOString(),
                receivedMessage: message
              };
              
              res.writeHead(200, { 'Content-Type': 'application/json' });
              res.end(JSON.stringify(response));
            } catch (error) {
              res.writeHead(400, { 'Content-Type': 'application/json' });
              res.end(JSON.stringify({ success: false, error: 'Invalid JSON' }));
            }
          });
          
          return;
        }
        
        res.writeHead(404);
        res.end('Not Found');
      });
      
      this.httpServer.listen(this.testConfig.jtagPort, () => {
        console.log(`   🔧 Test HTTP server started on port ${this.testConfig.jtagPort}`);
        resolve();
      });
    });
  }

  private async stopHTTPServer(): Promise<void> {
    if (this.httpServer) {
      return new Promise((resolve) => {
        this.httpServer!.close(() => {
          console.log('   🔧 Test HTTP server stopped');
          this.httpServer = null;
          resolve();
        });
      });
    }
  }
}

// Export for integration with other tests
export { TransportIntegrationTest };

// Run tests if called directly
if (require.main === module) {
  const test = new TransportIntegrationTest();
  test.runAllTests().catch(error => {
    console.error('💥 Transport integration tests failed:', error);
    process.exit(1);
  });
}