#!/usr/bin/env tsx
/**
 * Milestone 1: Express Server + Health Endpoint
 *
 * Tests:
 * 1. Server starts on port 3456
 * 2. GET /health returns 200
 * 3. GET /health returns JSON {status: "ok"}
 * 4. Unknown route returns 404
 */

import http from 'http';

const PORT = 3456;
const BASE = `http://localhost:${PORT}`;

let passed = 0;
let failed = 0;
let server: any;

function request(path: string, options: http.RequestOptions = {}): Promise<{ status: number; body: string }> {
  return new Promise((resolve, reject) => {
    const url = new URL(path, BASE);
    const req = http.request(url, { ...options }, (res) => {
      let body = '';
      res.on('data', (chunk) => body += chunk);
      res.on('end', () => resolve({ status: res.statusCode!, body }));
    });
    req.on('error', reject);
    req.setTimeout(5000, () => { req.destroy(); reject(new Error('Timeout')); });
    req.end();
  });
}

function assert(name: string, condition: boolean, detail?: string) {
  if (condition) {
    console.log(`✅ ${name}`);
    passed++;
  } else {
    console.log(`❌ ${name}${detail ? ` — ${detail}` : ''}`);
    failed++;
  }
}

async function main() {
  console.log('Milestone 1: Express Server + Health Endpoint');
  console.log('─'.repeat(50));

  // Import and start the app
  try {
    const mod = await import('../src/index');
    const app = mod.app || mod.default;
    if (!app) {
      console.log('❌ Could not import app from src/index.ts');
      console.log(`\nResults: 0 passed, 4 failed`);
      process.exit(1);
    }

    server = app.listen(PORT);
    // Give server a moment to bind
    await new Promise(r => setTimeout(r, 500));
  } catch (err) {
    console.log(`❌ Failed to start server: ${err}`);
    console.log(`\nResults: 0 passed, 4 failed`);
    process.exit(1);
  }

  try {
    // Test 1: Server is listening
    try {
      const res = await request('/health');
      assert('Server starts on port 3456', true);
    } catch {
      assert('Server starts on port 3456', false, 'Connection refused');
      console.log(`\nResults: ${passed} passed, ${4 - passed} failed`);
      server?.close();
      process.exit(1);
    }

    // Test 2: GET /health returns 200
    const healthRes = await request('/health');
    assert('GET /health returns 200', healthRes.status === 200, `Got ${healthRes.status}`);

    // Test 3: GET /health returns {status: "ok"}
    try {
      const body = JSON.parse(healthRes.body);
      assert('GET /health returns {status: "ok"}', body.status === 'ok', `Got ${JSON.stringify(body)}`);
    } catch {
      assert('GET /health returns {status: "ok"}', false, `Body not valid JSON: ${healthRes.body}`);
    }

    // Test 4: Unknown route returns 404
    const unknownRes = await request('/nonexistent-route-xyz');
    assert('Unknown route returns 404', unknownRes.status === 404, `Got ${unknownRes.status}`);
  } finally {
    server?.close();
  }

  console.log(`\nResults: ${passed} passed, ${failed} failed`);
  process.exit(failed > 0 ? 1 : 0);
}

main().catch(err => {
  console.error('Fatal:', err);
  server?.close();
  console.log(`\nResults: ${passed} passed, ${failed + 1} failed`);
  process.exit(1);
});
