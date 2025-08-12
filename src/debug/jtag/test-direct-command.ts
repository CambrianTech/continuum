#!/usr/bin/env tsx
/**
 * Test Direct Command - Bypass broken RemoteConnection and call commands directly
 */

import { WebSocketTransportClientServer } from './system/transports/websocket-transport/server/WebSocketTransportClientServer';
import type { JTAGMessage } from './shared/JTAGTypes';
import { SYSTEM_SCOPES } from './shared/SystemScopes';
import { generateUUID } from './shared/CrossPlatformUUID';

// Simple handler that collects responses and handles session handshake
class DirectCommandHandler {
  private responses = new Map<string, JTAGMessage>();
  public realSessionId: string | null = null;

  async handleTransportMessage(message: JTAGMessage): Promise<void> {
    console.log(`📨 Received: ${message.type} to ${message.endpoint || 'no-endpoint'}`);
    
    // Handle session handshake response
    if (message.type === 'session_handshake_response') {
      this.realSessionId = message.payload?.sessionId || message.sessionId;
      console.log(`🤝 Got real session ID: ${this.realSessionId}`);
      return;
    }
    
    if (message.type === 'response' && message.correlationId) {
      this.responses.set(message.correlationId, message);
      console.log(`✅ Got response for ${message.correlationId}`);
    }
  }

  async sendAndWait(transport: WebSocketTransportClientServer, command: string, params: any = {}): Promise<any> {
    const correlationId = generateUUID();
    
    const message: JTAGMessage = {
      id: generateUUID(),
      sessionId: SYSTEM_SCOPES.UNKNOWN_SESSION,
      type: 'request',
      source: 'client',
      endpoint: `commands/${command}`, // Direct command endpoint
      payload: params,
      timestamp: Date.now(),
      correlationId
    };
    
    console.log(`📤 Sending ${command} command...`);
    await transport.send(message);
    
    // Wait for response
    for (let i = 0; i < 50; i++) {
      await new Promise(resolve => setTimeout(resolve, 100));
      if (this.responses.has(correlationId)) {
        const response = this.responses.get(correlationId)!;
        console.log(`✅ Command ${command} completed`);
        return response.payload;
      }
    }
    
    throw new Error(`Command ${command} timeout`);
  }
}

async function testDirectCommands() {
  try {
    console.log('🧪 Testing direct command execution...');
    
    const handler = new DirectCommandHandler();
    const transport = new WebSocketTransportClientServer({
      url: 'ws://localhost:9001',
      handler: handler as any,
      sessionHandshake: true
    });
    
    await transport.connect('ws://localhost:9001');
    console.log('✅ Connected to your running system');
    
    // Wait for session handshake
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Test LIST command
    console.log('\n📋 Testing LIST command...');
    const listResult = await handler.sendAndWait(transport, 'list', { category: 'all' });
    console.log('📝 Available commands:', listResult.commands?.length || 0);
    
    // Test FILE/SAVE command (should work without browser)
    console.log('\n💾 Testing FILE/SAVE command...');
    const saveResult = await handler.sendAndWait(transport, 'file/save', {
      filename: 'test-direct-command.txt',
      content: 'Hello from direct command!'
    });
    console.log('📁 File save result:', saveResult);
    
    console.log('\n🎉 Direct command test completed!');
    
    await transport.disconnect();
    
  } catch (error) {
    console.error('❌ Direct command test failed:', error);
  }
}

testDirectCommands();