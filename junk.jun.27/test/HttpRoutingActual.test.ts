/**
 * HTTP Routing Actual Test - 50 lines max
 * Tests that HTTP requests actually return HTML (not "Not Found")
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { WebSocketDaemon } from '../WebSocketDaemon.js';
import { RendererDaemon } from '../../../daemons/renderer/RendererDaemon.js';
import * as http from 'http';

describe('HTTP Routing Actual', () => {
  let webSocketDaemon: WebSocketDaemon;
  let rendererDaemon: RendererDaemon;

  beforeEach(async () => {
    webSocketDaemon = new WebSocketDaemon({ port: 9007 });
    rendererDaemon = new RendererDaemon();
    await webSocketDaemon.start();
    await rendererDaemon.start();
    await webSocketDaemon.registerExternalDaemon('renderer', rendererDaemon);
  });

  afterEach(async () => {
    if (webSocketDaemon) await webSocketDaemon.stop();
    if (rendererDaemon) await rendererDaemon.stop();
  });

  it('CRITICAL: Root path must return HTML not "Not Found"', async () => {
    const response = await makeRequest('http://localhost:9007/');
    
    expect(response.statusCode).toBe(200);
    expect(response.headers['content-type']).toMatch(/text\/html/);
    expect(response.body).toContain('<!DOCTYPE html>');
    expect(response.body).not.toContain('Not Found');
  });

  it('Health endpoint should work', async () => {
    const response = await makeRequest('http://localhost:9007/health');
    
    expect(response.statusCode).toBe(200);
    expect(response.body).toContain('healthy');
  });

  it('Static JS files should be served', async () => {
    const response = await makeRequest('http://localhost:9007/src/ui/continuum.js');
    
    expect(response.statusCode).toBe(200);
    expect(response.headers['content-type']).toMatch(/javascript/);
    expect(response.body).not.toContain('Not Found');
  });
});

function makeRequest(url: string): Promise<{statusCode: number; body: string; headers: Record<string, string>}> {
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
      res.on('end', () => resolve({
        statusCode: res.statusCode || 0,
        body,
        headers: res.headers as Record<string, string>
      }));
    });
    req.on('error', () => resolve({statusCode: 0, body: 'Error', headers: {}}));
    req.setTimeout(3000, () => {req.destroy(); resolve({statusCode: 0, body: 'Timeout', headers: {}});});
    req.end();
  });
}