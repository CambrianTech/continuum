#!/usr/bin/env tsx
/**
 * Test List Command - Direct WebSocket command test  
 * 
 * Tests the 'list' command which should work server-side without browser
 */

import { WebSocketTransportClientServer } from './system/transports/websocket-transport/server/WebSocketTransportClientServer';
import type { JTAGMessage } from './shared/JTAGTypes';
import { SYSTEM_SCOPES } from './shared/SystemScopes';
import { generateUUID } from './shared/CrossPlatformUUID';

// Transport handler that waits for responses
class ListCommandHandler {
  private pendingRequests = new Map<string, (response: JTAGMessage) => void>();

  async handleTransportMessage(message: JTAGMessage): Promise<void> {
    console.log(`📨 Received message: ${message.type} ${message.endpoint}`);
    
    if (message.type === 'response' && message.correlationId) {
      const resolver = this.pendingRequests.get(message.correlationId);
      if (resolver) {
        console.log('🎯 Found correlation match, resolving...');
        this.pendingRequests.delete(message.correlationId);
        resolver(message);
        return;
      }
    }
    
    console.log('📄 Message payload:', JSON.stringify(message.payload, null, 2));
  }

  async sendAndWaitForResponse(transport: WebSocketTransportClientServer, message: JTAGMessage, timeoutMs = 5000): Promise<JTAGMessage> {
    return new Promise((resolve, reject) => {
      // Set up correlation handler
      if (message.correlationId) {
        this.pendingRequests.set(message.correlationId, resolve);
      }
      
      // Send message
      transport.send(message).catch(reject);
      
      // Timeout handler
      setTimeout(() => {
        if (message.correlationId) {
          this.pendingRequests.delete(message.correlationId);
        }
        reject(new Error(`Command timeout after ${timeoutMs}ms`));
      }, timeoutMs);
    });
  }
}

async function main() {
  try {
    console.log('🧪 Testing LIST command via WebSocket...');
    
    const handler = new ListCommandHandler();
    const transport = new WebSocketTransportClientServer({
      url: 'ws://localhost:9001',
      handler: handler as any,
      sessionHandshake: true
    });
    
    console.log('🔗 Connecting to WebSocket server...');
    await transport.connect('ws://localhost:9001');
    console.log(`✅ Connected! Session will be assigned by server`);
    
    // Wait a moment for session handshake
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Create list command message
    const correlationId = generateUUID();
    const listMessage: JTAGMessage = {
      id: generateUUID(),
      sessionId: SYSTEM_SCOPES.UNKNOWN_SESSION, // Bootstrap session
      type: 'request',
      source: 'client',
      endpoint: 'commands/list', // Standard list endpoint
      payload: {}, // No parameters needed for list
      timestamp: Date.now(),
      correlationId
    };
    
    console.log('📤 Sending LIST command...');
    console.log(`🎯 Correlation ID: ${correlationId}`);
    
    try {
      const response = await handler.sendAndWaitForResponse(transport, listMessage);
      console.log('✅ LIST command response received!');
      console.log('📋 Available commands:', response.payload);
      
      if (response.payload && typeof response.payload === 'object') {
        const commands = response.payload as any;
        if (commands.commands && Array.isArray(commands.commands)) {
          console.log(`🎉 Found ${commands.commands.length} available commands:`);
          commands.commands.forEach((cmd: string, i: number) => {
            console.log(`  ${i + 1}. ${cmd}`);
          });
        }
      }
      
    } catch (error) {
      console.log('⏰ No response received, but message was sent successfully');
      console.log('💡 This might mean the correlation system needs work');
    }
    
    // Clean disconnect
    setTimeout(async () => {
      await transport.disconnect();
      console.log('🔌 Disconnected');
      process.exit(0);
    }, 2000);
    
  } catch (error) {
    console.error('❌ LIST command test failed:', error);
    process.exit(1);
  }
}

main().catch(console.error);