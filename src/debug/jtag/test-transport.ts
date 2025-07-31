#!/usr/bin/env tsx
/**
 * Transport Test - Direct WebSocket transport test
 * 
 * Tests WebSocketTransportServerClient directly without going through JTAGClient
 */

import { WebSocketTransportServerClient } from './system/transports/websocket-transport/server/WebSocketTransportServerClient';
import type { JTAGMessage } from './shared/JTAGTypes';
import { SYSTEM_SCOPES } from './shared/SystemScopes';
import { generateUUID } from './shared/CrossPlatformUUID';

// Minimal transport handler for testing
class TestTransportHandler {
  async handleTransportMessage(message: JTAGMessage): Promise<void> {
    console.log('📨 Received message:', message);
  }
}

async function main() {
  try {
    console.log('🧪 Testing WebSocketTransportServerClient directly...');
    
    const handler = new TestTransportHandler();
    const transport = new WebSocketTransportServerClient({
      url: 'ws://localhost:9001',
      handler: handler as any,
      sessionHandshake: true
    });
    
    console.log('🔗 Connecting to WebSocket server...');
    await transport.connect('ws://localhost:9001');
    
    console.log(`✅ Connected! Transport name: ${transport.name}`);
    console.log(`🟢 Connected status: ${transport.connected}`);
    
    // Test sending a simple message
    const testMessage: JTAGMessage = {
      id: generateUUID(),
      sessionId: SYSTEM_SCOPES.UNKNOWN_SESSION,
      type: 'request',
      source: 'client',
      endpoint: 'health/ping',
      payload: { test: 'Hello from WebSocket client!' },
      timestamp: Date.now(),
      correlationId: generateUUID()
    };
    
    console.log('📤 Sending test message...');
    const result = await transport.send(testMessage);
    console.log(`✅ Send result: success=${result.success}`);
    
    console.log('🎉 WebSocket transport test completed successfully!');
    
    // Clean disconnect
    setTimeout(async () => {
      await transport.disconnect();
      console.log('🔌 Disconnected');
      process.exit(0);
    }, 2000);
    
  } catch (error) {
    console.error('❌ Transport test failed:', error);
    process.exit(1);
  }
}

main().catch(console.error);