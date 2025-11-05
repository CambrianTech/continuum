#!/usr/bin/env npx tsx

/**
 * Direct Server Client Test - No complex bootstrap
 * Tests direct WebSocket connection from server-side to running JTAG system
 */

import { WebSocket } from 'ws';

console.log('🔧 Testing direct server-side WebSocket connection...');

async function testDirectConnection() {
  try {
    console.log('🔗 Creating direct WebSocket connection to ws://localhost:9001...');
    
    const ws = new WebSocket('ws://localhost:9001');
    
    ws.on('open', () => {
      console.log('✅ WebSocket connection established!');
      
      // Test sending a ping message
      const pingMessage = JSON.stringify({
        type: 'request',
        endpoint: 'server/commands/ping',
        correlationId: 'test-ping-' + Date.now(),
        data: { message: 'ping from server client' }
      });
      
      console.log('📤 Sending ping message:', pingMessage);
      ws.send(pingMessage);
      
      setTimeout(() => {
        console.log('🔚 Closing connection after test');
        ws.close();
      }, 2000);
    });
    
    ws.on('message', (data) => {
      console.log('📥 Received message:', data.toString());
    });
    
    ws.on('close', (code, reason) => {
      console.log(`🔌 Connection closed: ${code} ${reason}`);
    });
    
    ws.on('error', (error) => {
      console.error('❌ WebSocket error:', error.message);
      console.error('Error details:', error);
    });
    
  } catch (error) {
    console.error('💥 Test failed:', error);
  }
}

testDirectConnection().then(() => {
  console.log('🏁 Direct connection test completed');
}).catch(error => {
  console.error('💥 Test script failed:', error);
  process.exit(1);
});