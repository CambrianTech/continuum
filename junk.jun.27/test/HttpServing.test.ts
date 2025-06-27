/**
 * HTTP Serving Unit Tests
 * Tests the WebSocketDaemon's HTTP server functionality
 */

import { WebSocketDaemon } from '../WebSocketDaemon.js';

describe('WebSocketDaemon HTTP Serving', () => {
  let daemon: WebSocketDaemon;
  const testPort = 9001;

  beforeEach(async () => {
    daemon = new WebSocketDaemon({ port: testPort });
  });

  afterEach(async () => {
    if (daemon) {
      await daemon.stop();
    }
  });

  test('should serve HTML at root path', async () => {
    await daemon.start();

    // Act like a browser - make HTTP request to root
    const response = await fetch(`http://localhost:${testPort}/`);
    
    expect(response.status).toBe(200);
    expect(response.headers.get('content-type')).toBe('text/html');
    
    const html = await response.text();
    expect(html).toContain('<!DOCTYPE html>');
    expect(html).toContain('Continuum');
  });

  test('should serve health endpoint', async () => {
    await daemon.start();

    const response = await fetch(`http://localhost:${testPort}/health`);
    
    expect(response.status).toBe(200);
    expect(response.headers.get('content-type')).toBe('application/json');
    
    const data = await response.json();
    expect(data.status).toBe('healthy');
    expect(data.server).toBe('websocket-server');
  });

  test('should serve static JavaScript files', async () => {
    await daemon.start();

    // Test serving continuum-api.js
    const response = await fetch(`http://localhost:${testPort}/src/ui/continuum-api.js`);
    
    expect(response.status).toBe(200);
    expect(response.headers.get('content-type')).toBe('application/javascript');
    
    const js = await response.text();
    expect(js).toContain('Continuum Browser API');
    expect(js).toContain('setupConsoleForwarding');
  });

  test('should return 404 for non-existent files', async () => {
    await daemon.start();

    const response = await fetch(`http://localhost:${testPort}/nonexistent.js`);
    
    expect(response.status).toBe(404);
    expect(response.headers.get('content-type')).toBe('text/plain');
  });

  test('should support WebSocket upgrade on same port', async () => {
    await daemon.start();

    // Test WebSocket connection on same port as HTTP
    const ws = new WebSocket(`ws://localhost:${testPort}`);
    
    await new Promise((resolve, reject) => {
      ws.onopen = resolve;
      ws.onerror = reject;
      setTimeout(() => reject(new Error('WebSocket connection timeout')), 5000);
    });

    expect(ws.readyState).toBe(WebSocket.OPEN);
    ws.close();
  });

  test('should handle concurrent HTTP and WebSocket requests', async () => {
    await daemon.start();

    // Make HTTP request and WebSocket connection simultaneously
    const [httpResponse, wsConnection] = await Promise.all([
      fetch(`http://localhost:${testPort}/health`),
      new Promise((resolve, reject) => {
        const ws = new WebSocket(`ws://localhost:${testPort}`);
        ws.onopen = () => resolve(ws);
        ws.onerror = reject;
        setTimeout(() => reject(new Error('WebSocket timeout')), 5000);
      })
    ]);

    expect(httpResponse.status).toBe(200);
    expect((wsConnection as WebSocket).readyState).toBe(WebSocket.OPEN);
    
    (wsConnection as WebSocket).close();
  });
});