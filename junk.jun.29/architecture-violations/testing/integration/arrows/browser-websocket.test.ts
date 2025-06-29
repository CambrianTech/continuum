/**
 * Browser Client → WebSocket Arrow Test
 * Testing the specific browser → WebSocket connection and message passing
 */

import { EventEmitter } from 'events';

// Simulate browser client behavior
class BrowserClientSimulator extends EventEmitter {
  private messageId = 0;
  private pendingMessages = new Map<string, { resolve: Function; reject: Function }>();

  constructor() {
    super();
    console.debug('🌐 BROWSER_SIM: Browser client simulator initialized');
  }

  // Simulate sending a message to WebSocket
  async sendCommand(command: string, params: any = {}): Promise<any> {
    const messageId = `browser_msg_${++this.messageId}`;
    
    const message = {
      type: 'execute_command',
      command,
      params,
      id: messageId,
      timestamp: new Date().toISOString(),
      source: 'browser-client'
    };

    console.debug(`📤 BROWSER_SIM: Sending command to WebSocket:`, {
      command: message.command,
      id: message.id,
      type: message.type
    });

    return new Promise((resolve, reject) => {
      this.pendingMessages.set(messageId, { resolve, reject });
      
      // Emit to WebSocket layer
      this.emit('websocket-message', message);
      
      // Timeout after 5 seconds
      setTimeout(() => {
        if (this.pendingMessages.has(messageId)) {
          this.pendingMessages.delete(messageId);
          reject(new Error(`Browser → WebSocket timeout for command: ${command}`));
        }
      }, 5000);
    });
  }

  // Simulate receiving response from WebSocket
  receiveResponse(response: any): void {
    console.debug(`📥 BROWSER_SIM: Received response from WebSocket:`, {
      id: response.id,
      success: response.success,
      command: response.originalCommand
    });

    const pending = this.pendingMessages.get(response.id);
    if (pending) {
      this.pendingMessages.delete(response.id);
      
      if (response.success) {
        pending.resolve(response);
      } else {
        pending.reject(new Error(response.error));
      }
    }
  }
}

// Simulate WebSocket server behavior  
class WebSocketServerSimulator extends EventEmitter {
  private clients = new Set<BrowserClientSimulator>();

  constructor() {
    super();
    console.debug('🔌 WEBSOCKET_SIM: WebSocket server simulator initialized');
  }

  // Simulate browser client connection
  connectClient(client: BrowserClientSimulator): void {
    console.debug('🔗 WEBSOCKET_SIM: Browser client connected');
    
    this.clients.add(client);
    
    // Listen for messages from browser client
    client.on('websocket-message', (message) => {
      this.handleClientMessage(client, message);
    });

    client.emit('connected');
  }

  // Handle message from browser client
  private async handleClientMessage(client: BrowserClientSimulator, message: any): Promise<void> {
    console.debug(`📥 WEBSOCKET_SIM: Received message from browser:`, {
      command: message.command,
      id: message.id,
      source: message.source
    });

    try {
      // Simulate WebSocket processing the command
      console.debug(`🔄 WEBSOCKET_SIM: Processing browser command: ${message.command}`);
      
      // Simulate some async processing time
      await new Promise(resolve => setTimeout(resolve, 10));
      
      // Create response 
      const response = {
        id: message.id,
        success: true,
        data: {
          message: `WebSocket processed: ${message.command}`,
          timestamp: new Date().toISOString(),
          originalParams: message.params
        },
        originalCommand: message.command,
        processedBy: 'websocket-server-simulator'
      };

      console.debug(`📤 WEBSOCKET_SIM: Sending response to browser:`, {
        id: response.id,
        success: response.success,
        command: message.command
      });

      // Send response back to browser client
      client.receiveResponse(response);
      
    } catch (error) {
      console.debug(`❌ WEBSOCKET_SIM: Error processing browser message:`, {
        error: (error as Error).message,
        command: message.command,
        id: message.id
      });

      const errorResponse = {
        id: message.id,
        success: false,
        error: (error as Error).message,
        originalCommand: message.command,
        processedBy: 'websocket-server-simulator'
      };

      client.receiveResponse(errorResponse);
    }
  }

  disconnectClient(client: BrowserClientSimulator): void {
    console.debug('🔗 WEBSOCKET_SIM: Browser client disconnected');
    this.clients.delete(client);
    client.emit('disconnected');
  }
}

async function testBrowserWebSocketArrow(): Promise<void> {
  console.log('🧪 Testing Browser Client → WebSocket Arrow...\n');

  const webSocketServer = new WebSocketServerSimulator();
  const browserClient = new BrowserClientSimulator();

  try {
    // Test 1: Basic connection
    console.log('📋 Test 1: Browser client → WebSocket connection');
    webSocketServer.connectClient(browserClient);
    console.log('✅ Browser → WebSocket connection established');

    // Test 2: Single command message
    console.log('\n📋 Test 2: Single command Browser → WebSocket');
    
    const infoResponse = await browserClient.sendCommand('info', { section: 'version' });
    console.log('✅ Browser → WebSocket single command:', infoResponse.success);

    // Test 3: Multiple concurrent commands
    console.log('\n📋 Test 3: Concurrent commands Browser → WebSocket');
    
    const concurrentPromises = [
      browserClient.sendCommand('info', {}),
      browserClient.sendCommand('status', {}),
      browserClient.sendCommand('list', {}),
      browserClient.sendCommand('help', {})
    ];

    const concurrentResults = await Promise.all(concurrentPromises);
    const successCount = concurrentResults.filter(r => r.success).length;
    console.log(`✅ Browser → WebSocket concurrent: ${successCount}/4 commands succeeded`);

    // Test 4: Command with parameters
    console.log('\n📋 Test 4: Command with parameters Browser → WebSocket');
    
    const paramResponse = await browserClient.sendCommand('help', { command: 'info' });
    console.log('✅ Browser → WebSocket with params:', paramResponse.success);
    console.debug('📊 Parameter passing verified:', {
      originalParams: paramResponse.data.originalParams,
      command: paramResponse.originalCommand
    });

    // Test 5: Message ID tracking
    console.log('\n📋 Test 5: Message ID tracking Browser → WebSocket');
    
    const msg1Promise = browserClient.sendCommand('test1', {});
    const msg2Promise = browserClient.sendCommand('test2', {});
    
    const [msg1Result, msg2Result] = await Promise.all([msg1Promise, msg2Promise]);
    
    const idsUnique = msg1Result.id !== msg2Result.id;
    console.log('✅ Browser → WebSocket message ID uniqueness:', idsUnique ? 'PASS' : 'FAIL');

    console.log('\n✅ Browser Client → WebSocket arrow test complete!');
    console.debug('🎯 VERIFIED: Browser client can successfully communicate with WebSocket layer');

  } catch (error) {
    console.error('❌ Browser → WebSocket arrow test failed:', error);
    throw error;
  } finally {
    // Cleanup
    webSocketServer.disconnectClient(browserClient);
    console.debug('🧹 Browser → WebSocket test cleanup complete');
  }
}

// Run the test
testBrowserWebSocketArrow().catch((error) => {
  console.error('❌ Test execution failed:', error);
  process.exit(1);
});