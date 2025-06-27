/**
 * Routing Bug Fix Test - 50 lines max
 * Tests the specific bug: registered routes return 404
 */

import { describe, it, expect, beforeAll, afterAll } from '@jest/globals';
import * as http from 'http';

describe('Routing Bug Fix', () => {
  beforeAll(async () => {
    // Wait for system to be ready
    await new Promise(resolve => setTimeout(resolve, 1000));
  });

  it('WebSocketDaemon built-in endpoints should work', async () => {
    const health = await makeRequest('http://localhost:9000/health');
    expect(health.statusCode).toBe(200);
    expect(health.body).toContain('healthy');

    const status = await makeRequest('http://localhost:9000/status');
    expect(status.statusCode).toBe(200);
    expect(status.headers['content-type']).toMatch(/text\/html/);
  });

  it('BUG: Root path should return HTML but returns 404', async () => {
    const response = await makeRequest('http://localhost:9000/');
    
    console.log('Root path response:', response.statusCode, response.body);
    
    // This should pass after fix
    expect(response.statusCode).toBe(200);
    expect(response.body).toContain('<!DOCTYPE html>');
    expect(response.headers['content-type']).toMatch(/text\/html/);
  });

  it('BUG: Static files should be served but return 404', async () => {
    const response = await makeRequest('http://localhost:9000/src/ui/continuum.js');
    
    console.log('Static file response:', response.statusCode, response.body);
    
    // This should pass after fix
    expect(response.statusCode).toBe(200);
    expect(response.headers['content-type']).toMatch(/javascript/);
    expect(response.body).toContain('ContinuumAPI');
  });

  it('BUG: RendererDaemon direct access should serve root', async () => {
    const response = await makeRequest('http://localhost:9001/');
    
    console.log('Direct renderer response:', response.statusCode, response.body);
    
    // This should pass after fix - RendererDaemon should serve root
    expect(response.statusCode).toBe(200);
    expect(response.body).toContain('<!DOCTYPE html>');
  });
});

function makeRequest(url: string): Promise<{
  statusCode: number;
  body: string;
  headers: Record<string, string>;
}> {
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