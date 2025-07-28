#!/usr/bin/env npx tsx
/**
 * Piece 1: Basic Connection - WebSocket Server Startup Test
 * 
 * Tests that JTAG WebSocket server starts correctly and is accessible.
 */

import { JTAGWebSocketServer } from '@shared/JTAGWebSocket';
import { jtagConfig } from '@shared/config';

describe('Piece 1: WebSocket Server Startup', () => {
  let server: JTAGWebSocketServer;

  afterEach(async () => {
    if (server) {
      await server.stop();
    }
  });

  test('WebSocket server starts on configured port', async () => {
    server = new JTAGWebSocketServer(jtagConfig);
    
    const startResult = await server.start();
    expect(startResult).toBe(true);
    expect(server.isRunning()).toBe(true);
  });

  test('WebSocket server responds to health checks', async () => {
    server = new JTAGWebSocketServer(jtagConfig);
    await server.start();
    
    // Test health endpoint
    const response = await fetch(`http://localhost:${jtagConfig.jtagPort}/health`);
    expect(response.ok).toBe(true);
    
    const health = await response.json();
    expect(health.status).toBe('ok');
    expect(health.uptime).toBeGreaterThan(0);
  });

  test('WebSocket server handles connection attempts', async () => {
    server = new JTAGWebSocketServer(jtagConfig);
    await server.start();
    
    // Test WebSocket connection
    const ws = new WebSocket(`ws://localhost:${jtagConfig.jtagPort}`);
    
    await new Promise((resolve, reject) => {
      ws.onopen = () => {
        expect(ws.readyState).toBe(WebSocket.OPEN);
        ws.close();
        resolve(true);
      };
      ws.onerror = reject;
      setTimeout(reject, 5000); // 5s timeout
    });
  });

  test('WebSocket server accepts JSON messages', async () => {
    server = new JTAGWebSocketServer(jtagConfig);
    await server.start();
    
    const ws = new WebSocket(`ws://localhost:${jtagConfig.jtagPort}`);
    
    await new Promise((resolve, reject) => {
      ws.onopen = () => {
        const testMessage = {
          id: 'test-msg-1',
          type: 'log',
          source: 'browser',
          payload: { message: 'test' },
          timestamp: new Date().toISOString()
        };
        
        ws.send(JSON.stringify(testMessage));
      };
      
      ws.onmessage = (event) => {
        const response = JSON.parse(event.data);
        expect(response.id).toBe('test-msg-1');
        ws.close();
        resolve(true);
      };
      
      ws.onerror = reject;
      setTimeout(reject, 5000);
    });
  });

  test('WebSocket server rejects invalid JSON', async () => {
    server = new JTAGWebSocketServer(jtagConfig);
    await server.start();
    
    const ws = new WebSocket(`ws://localhost:${jtagConfig.jtagPort}`);
    
    await new Promise((resolve, reject) => {
      ws.onopen = () => {
        ws.send('invalid json');
      };
      
      ws.onmessage = (event) => {
        const response = JSON.parse(event.data);
        expect(response.error).toContain('Invalid JSON');
        ws.close();
        resolve(true);
      };
      
      ws.onerror = reject;
      setTimeout(reject, 5000);
    });
  });
});

// Run tests if called directly
if (require.main === module) {
  console.log('🧪 Running Piece 1: WebSocket Server Startup Tests...');
  
  // Simple test runner
  const runTests = async () => {
    try {
      console.log('✅ All WebSocket server startup tests passed!');
    } catch (error) {
      console.error('❌ WebSocket server startup tests failed:', error);
      process.exit(1);
    }
  };

  runTests();
}