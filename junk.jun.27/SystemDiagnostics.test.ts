/**
 * System Diagnostics Test - 50 lines max
 * Tests each component step by step to find what's broken
 */

import { describe, it, expect } from '@jest/globals';
import * as http from 'http';

describe('System Diagnostics', () => {
  it('WebSocket daemon should be listening on port 9000', async () => {
    const response = await makeRequest('http://localhost:9000/health');
    console.log('Health check response:', response.statusCode, response.body);
    expect(response.statusCode).toBeGreaterThan(0); // At least responding
  });

  it('should serve root path with HTML', async () => {
    const response = await makeRequest('http://localhost:9000/');
    console.log('Root response:', response.statusCode, response.body.substring(0, 200));
    
    if (response.statusCode === 404) {
      console.log('❌ ROOT PATH NOT ROUTED - Route registration failed');
    }
    
    expect(response.statusCode).toBe(200);
    expect(response.body).toContain('<!DOCTYPE html>');
  });

  it('Renderer daemon should be serving on port 9001', async () => {
    const response = await makeRequest('http://localhost:9001/');
    console.log('Renderer response:', response.statusCode, response.body.substring(0, 200));
    expect(response.statusCode).toBe(200);
  });

  it('should serve static files via registered routes', async () => {
    const response = await makeRequest('http://localhost:9000/src/ui/continuum.js');
    console.log('Static file response:', response.statusCode, response.body.substring(0, 100));
    expect(response.statusCode).toBe(200);
  });
});

function makeRequest(url: string): Promise<{statusCode: number; body: string}> {
  return new Promise((resolve) => {
    const urlObj = new URL(url);
    const req = http.request({
      hostname: urlObj.hostname,
      port: urlObj.port || (urlObj.protocol === 'https:' ? 443 : 80),
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