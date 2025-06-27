/**
 * Actual Website Integration Test - 50 lines max
 * Tests that http://localhost:9000 returns valid HTML
 */

import { describe, it, expect, beforeAll, afterAll } from '@jest/globals';
import * as http from 'http';

describe('Actual Website Integration', () => {
  let systemStarted = false;

  beforeAll(async () => {
    // Give system time to start if it's starting
    await new Promise(resolve => setTimeout(resolve, 2000));
  });

  it('should return valid HTML from localhost:9000', async () => {
    const response = await makeHTTPRequest('http://localhost:9000/');
    
    expect(response.statusCode).toBe(200);
    expect(response.headers['content-type']).toMatch(/text\/html/);
    expect(response.body).toContain('<!DOCTYPE html>');
    expect(response.body).toContain('<html');
    expect(response.body).toContain('</html>');
    expect(response.body).toContain('Continuum');
  });

  it('should serve continuum.js script', async () => {
    const response = await makeHTTPRequest('http://localhost:9000/src/ui/continuum.js');
    
    expect(response.statusCode).toBe(200);
    expect(response.headers['content-type']).toMatch(/javascript/);
    expect(response.body).toContain('ContinuumAPI');
    expect(response.body).toContain('WebSocket');
  });

  it('should serve widget loader', async () => {
    const response = await makeHTTPRequest('http://localhost:9000/dist/ui/widget-loader.js');
    
    expect(response.statusCode).toBe(200);
    expect(response.headers['content-type']).toMatch(/javascript/);
  });
});

function makeHTTPRequest(url: string): Promise<{
  statusCode: number;
  headers: Record<string, string>;
  body: string;
}> {
  return new Promise((resolve) => {
    const urlObj = new URL(url);
    const options = {
      hostname: urlObj.hostname,
      port: urlObj.port || 9000,
      path: urlObj.pathname,
      method: 'GET'
    };

    const req = http.request(options, (res) => {
      let body = '';
      res.on('data', (chunk) => body += chunk);
      res.on('end', () => {
        resolve({
          statusCode: res.statusCode || 0,
          headers: res.headers as Record<string, string>,
          body
        });
      });
    });

    req.on('error', () => {
      resolve({ statusCode: 0, headers: {}, body: 'Connection failed' });
    });

    req.setTimeout(5000, () => {
      req.destroy();
      resolve({ statusCode: 0, headers: {}, body: 'Timeout' });
    });

    req.end();
  });
}