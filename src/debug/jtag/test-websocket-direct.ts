#!/usr/bin/env tsx
/**
 * Direct WebSocket Test
 * 
 * Test direct WebSocket communication to running server
 */

import WebSocket from 'ws';
import { JTAGMessageFactory } from './shared/JTAGTypes';

async function testDirectWebSocket() {
  console.log('🧪 Direct WebSocket Test');
  console.log('🔗 Connecting to ws://localhost:9001...');
  
  return new Promise<void>((resolve, reject) => {
    const ws = new WebSocket('ws://localhost:9001');
    
    ws.on('open', () => {
      console.log('✅ WebSocket connected');
      
      // Create proper JTAG request message
      const context = { uuid: 'test-session', environment: 'server' as const };
      const payload = {
        sessionId: 'test-session',
        context: context,
        message: 'test-ping',
        includeEnvironment: true,
        includeTimestamp: true
      };
      
      const message = JTAGMessageFactory.createRequest(
        context,
        'client',
        'commands/ping',
        payload,
        'test-123'
      );
      
      console.log('📤 Sending ping message:', JSON.stringify(message, null, 2));
      ws.send(JSON.stringify(message));
    });
    
    ws.on('message', (data) => {
      console.log('📨 Received message:', data.toString());
      ws.close();
      resolve();
    });
    
    ws.on('error', (error) => {
      console.error('❌ WebSocket error:', error.message);
      reject(error);
    });
    
    ws.on('close', () => {
      console.log('🔌 WebSocket closed');
    });
    
    // Timeout after 5 seconds
    setTimeout(() => {
      ws.close();
      reject(new Error('WebSocket test timeout'));
    }, 5000);
  });
}

// Run the test
testDirectWebSocket()
  .then(() => {
    console.log('✅ WebSocket test completed');
    process.exit(0);
  })
  .catch((error) => {
    console.error('❌ WebSocket test failed:', error.message);
    process.exit(1);
  });