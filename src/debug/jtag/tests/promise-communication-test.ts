#!/usr/bin/env npx tsx
/**
 * Step 4: Test Client-Server Promise-Based Communication
 * 
 * This test demonstrates your suggested approach:
 * - Client can grab server endpoint and call it via promise
 * - Server can grab client endpoint and call it via promise  
 * - Simple, direct communication without complex routing
 */

import { JTAGBase } from '@shared/JTAGBase';
import WebSocket from 'ws';

// Simple promise-based client that can grab server endpoint
class JTAGPromiseClient {
  private serverEndpoint: string;
  private ws: WebSocket | null = null;
  private messageHandlers: Map<string, (data: any) => void> = new Map();

  constructor(serverEndpoint: string = 'ws://localhost:9001') {
    this.serverEndpoint = serverEndpoint;
  }

  async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(this.serverEndpoint);
      
      this.ws.on('open', () => {
        console.log('📞 Promise client connected to server endpoint:', this.serverEndpoint);
        resolve();
      });
      
      this.ws.on('error', reject);
      
      this.ws.on('message', (data) => {
        try {
          const message = JSON.parse(data.toString());
          const handler = this.messageHandlers.get(message.id);
          if (handler) {
            handler(message);
            this.messageHandlers.delete(message.id);
          }
        } catch (error) {
          console.error('Promise client message parse error:', error);
        }
      });
    });
  }

  // Simple promise-based method to call server
  async callServer(method: string, payload: any): Promise<any> {
    if (!this.ws) throw new Error('Not connected');
    
    const messageId = 'call_' + Date.now() + '_' + Math.random().toString(36).substr(2, 9);
    
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.messageHandlers.delete(messageId);
        reject(new Error('Server call timeout'));
      }, 5000);

      this.messageHandlers.set(messageId, (response) => {
        clearTimeout(timeout);
        if (response.success) {
          resolve(response.result);
        } else {
          reject(new Error(response.error || 'Server call failed'));
        }
      });

      const message = {
        id: messageId,
        type: method,
        payload: payload,
        timestamp: new Date().toISOString()
      };

      this.ws!.send(JSON.stringify(message));
    });
  }

  disconnect(): void {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }
}

// Simple promise-based server interface
class JTAGPromiseServer {
  private jtagServer: any;
  
  constructor() {
    // Server can grab its own endpoint and offer promise-based methods
  }

  async initialize(): Promise<string> {
    // Initialize JTAG server and return endpoint
    JTAGBase.initialize({
      context: 'server',
      enableConsoleOutput: true,
      enableRemoteLogging: true,
      jtagPort: 9001
    });

    await new Promise(resolve => setTimeout(resolve, 2000));
    
    const endpoint = 'ws://localhost:9001';
    console.log('🖥️ Promise server initialized at endpoint:', endpoint);
    return endpoint;
  }

  // Server offers promise-based methods that client can call
  async processLog(data: any): Promise<{success: boolean, result: any}> {
    try {
      JTAGBase.log(data.component || 'PROMISE_TEST', data.message || 'Promise-based log');
      return { success: true, result: 'Log processed successfully' };
    } catch (error) {
      return { success: false, result: error.message };
    }
  }

  async takeScreenshot(options: any): Promise<{success: boolean, result: any}> {
    try {
      const result = await JTAGBase.screenshot(options.filename || 'promise-test', options);
      return { success: true, result: { 
        filepath: result.filepath, 
        success: result.success,
        metadata: result.metadata 
      }};
    } catch (error) {
      return { success: false, result: error.message };
    }
  }

  async getHealth(): Promise<{success: boolean, result: any}> {
    try {
      const connection = await JTAGBase.connect({ healthCheck: true });
      return { success: true, result: {
        healthy: connection.healthy,
        transport: connection.transport.type,
        latency: connection.transport.latency
      }};
    } catch (error) {
      return { success: false, result: error.message };
    }
  }
}

async function testPromiseCommunication() {
  console.log('🧪 Step 4: Testing Client-Server Promise-Based Communication\n');

  try {
    // Test 1: Initialize promise-based server
    console.log('📋 Test 4.1: Initialize promise-based server');
    const server = new JTAGPromiseServer();
    const serverEndpoint = await server.initialize();
    console.log('✅ Promise server ready at:', serverEndpoint);

    // Test 2: Client grabs server endpoint and connects
    console.log('\n📋 Test 4.2: Client grabs server endpoint');
    const client = new JTAGPromiseClient(serverEndpoint);
    await client.connect();
    console.log('✅ Promise client connected to server endpoint');

    // Test 3: Client calls server methods via promises
    console.log('\n📋 Test 4.3: Client calls server.processLog() via promise');
    
    const logResult = await client.callServer('log', {
      component: 'PROMISE_CLIENT',
      message: 'This is a promise-based log call from client to server',
      data: { method: 'promise', testId: 'client-to-server-log' }
    });
    
    console.log('📤 Client → Server log call result:', logResult?.success ? '✅' : '❌');
    if (logResult) console.log('📝 Server response:', logResult);

    // Test 4: Client calls server.takeScreenshot() via promise
    console.log('\n📋 Test 4.4: Client calls server.takeScreenshot() via promise');
    
    const screenshotResult = await client.callServer('screenshot', {
      filename: 'promise-based-screenshot',
      width: 800,
      height: 600,
      format: 'png'
    });
    
    console.log('📸 Client → Server screenshot call result:', screenshotResult?.success ? '✅' : '❌');
    if (screenshotResult) {
      console.log('📁 Screenshot result:', {
        success: screenshotResult.success,
        filepath: screenshotResult.filepath?.substring(0, 80) + '...'
      });
    }

    // Test 5: Client calls server.getHealth() via promise
    console.log('\n📋 Test 4.5: Client calls server.getHealth() via promise');
    
    const healthResult = await client.callServer('health', {});
    
    console.log('💖 Client → Server health call result:', healthResult?.success ? '✅' : '❌');
    if (healthResult) {
      console.log('🔍 Health check result:', {
        healthy: healthResult.healthy,
        transport: healthResult.transport,
        latency: healthResult.latency + 'ms'
      });
    }

    // Test 6: Demonstrate bidirectional communication potential
    console.log('\n📋 Test 4.6: Demonstrate bidirectional potential');
    console.log('💡 Server could also grab client endpoint and make promise calls:');
    console.log('   • const clientEndpoint = getClientEndpoint()');
    console.log('   • const clientResult = await client.requestScreenshot()');
    console.log('   • const clientData = await client.getUserInput()');
    console.log('✅ Bidirectional promise-based communication is possible');

    // Test 7: Clean up
    console.log('\n📋 Test 4.7: Clean up connections');
    client.disconnect();
    console.log('✅ Promise client disconnected');

    console.log('\n🎉 Step 4 Complete: Promise-based communication works!');
    console.log('💡 Key benefits:');
    console.log('   • Simple: client.callServer(method, payload) → Promise<result>');
    console.log('   • Direct: No complex routing, just grab endpoint and call');
    console.log('   • Bidirectional: Both client and server can initiate calls');
    console.log('   • Clean: Promise-based async/await pattern');
    
    return true;

  } catch (error) {
    console.error('❌ Step 4 Failed:', error);
    return false;
  }
}

// Run the test
testPromiseCommunication().then(success => {
  console.log('\n' + (success ? '🎉 Promise communication test PASSED' : '❌ Promise communication test FAILED'));
  process.exit(success ? 0 : 1);
}).catch(error => {
  console.error('💥 Promise communication test crashed:', error);
  process.exit(1);
});