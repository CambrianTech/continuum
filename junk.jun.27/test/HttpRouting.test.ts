/**
 * HTTP Routing Test - 50 lines max
 * Tests that WebSocketDaemon properly routes HTTP requests to registered handlers
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { WebSocketDaemon } from '../WebSocketDaemon.js';
import * as http from 'http';

describe('HTTP Routing', () => {
  let daemon: WebSocketDaemon;

  beforeEach(async () => {
    daemon = new WebSocketDaemon({ port: 9002 }); // Use different port for testing
    await daemon.start();
  });

  afterEach(async () => {
    if (daemon) {
      await daemon.stop();
    }
  });

  it('should register and route to root path handler', async () => {
    // Register a test handler for root path
    let handlerCalled = false;
    const testHandler = async (path: string, req: any, res: any) => {
      handlerCalled = true;
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end('<!DOCTYPE html><html><body>Test HTML</body></html>');
    };

    daemon.registerRouteHandler('/', { name: 'test-daemon' }, testHandler);

    // Make HTTP request to root
    const response = await makeRequest('http://localhost:9002/');
    
    expect(handlerCalled).toBe(true);
    expect(response.statusCode).toBe(200);
    expect(response.body).toContain('<!DOCTYPE html>');
    expect(response.body).toContain('Test HTML');
  });

  it('should route wildcard patterns correctly', async () => {
    let handlerCalled = false;
    const testHandler = async (path: string, req: any, res: any) => {
      handlerCalled = true;
      res.writeHead(200, { 'Content-Type': 'application/javascript' });
      res.end('console.log("test");');
    };

    daemon.registerRouteHandler('/src/*', { name: 'test-daemon' }, testHandler);

    const response = await makeRequest('http://localhost:9002/src/ui/continuum.js');
    
    expect(handlerCalled).toBe(true);
    expect(response.statusCode).toBe(200);
    expect(response.body).toContain('console.log');
  });
});

function makeRequest(url: string): Promise<{statusCode: number; body: string}> {
  return new Promise((resolve) => {
    const urlObj = new URL(url);
    const req = http.request({
      hostname: urlObj.hostname,
      port: urlObj.port,
      path: urlObj.pathname,
      method: 'GET'
    }, (res) => {
      let body = '';
      res.on('data', chunk => body += chunk);
      res.on('end', () => resolve({statusCode: res.statusCode || 0, body}));
    });
    req.on('error', () => resolve({statusCode: 0, body: 'Connection failed'}));
    req.setTimeout(3000, () => { req.destroy(); resolve({statusCode: 0, body: 'Timeout'}); });
    req.end();
  });
}