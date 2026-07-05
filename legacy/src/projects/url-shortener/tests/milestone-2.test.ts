#!/usr/bin/env tsx
/**
 * Milestone 2: URL Shortening Endpoint
 *
 * Tests (includes M1 regression):
 * 1. GET /health returns 200 {status: "ok"}
 * 2. POST /shorten with valid URL returns 201
 * 3. Response contains shortCode (6 alphanumeric chars)
 * 4. Response contains shortUrl with shortCode
 * 5. POST /shorten without url returns 400
 */

import http from 'http';

const PORT = 3456;
const BASE = `http://localhost:${PORT}`;

let passed = 0;
let failed = 0;
let server: any;

function request(
  path: string,
  options: http.RequestOptions & { body?: string } = {},
): Promise<{ status: number; body: string; headers: http.IncomingHttpHeaders }> {
  return new Promise((resolve, reject) => {
    const url = new URL(path, BASE);
    const { body, ...reqOpts } = options;
    const req = http.request(url, { ...reqOpts }, (res) => {
      let data = '';
      res.on('data', (chunk) => data += chunk);
      res.on('end', () => resolve({ status: res.statusCode!, body: data, headers: res.headers }));
    });
    req.on('error', reject);
    req.setTimeout(5000, () => { req.destroy(); reject(new Error('Timeout')); });
    if (body) req.write(body);
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

const TOTAL_TESTS = 5;

async function main() {
  console.log('Milestone 2: URL Shortening Endpoint');
  console.log('─'.repeat(50));

  try {
    const mod = await import('../src/index');
    const app = mod.app || mod.default;
    if (!app) {
      console.log('❌ Could not import app from src/index.ts');
      console.log(`\nResults: 0 passed, ${TOTAL_TESTS} failed`);
      process.exit(1);
    }

    server = app.listen(PORT);
    await new Promise(r => setTimeout(r, 500));
  } catch (err) {
    console.log(`❌ Failed to start server: ${err}`);
    console.log(`\nResults: 0 passed, ${TOTAL_TESTS} failed`);
    process.exit(1);
  }

  try {
    // Test 1: M1 regression — health endpoint
    const healthRes = await request('/health');
    assert('GET /health returns 200 {status:"ok"}',
      healthRes.status === 200 && JSON.parse(healthRes.body).status === 'ok',
      `Status: ${healthRes.status}, Body: ${healthRes.body}`);

    // Test 2: POST /shorten with valid URL returns 201
    const shortenRes = await request('/shorten', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ url: 'https://example.com' }),
    });
    assert('POST /shorten returns 201', shortenRes.status === 201, `Got ${shortenRes.status}`);

    // Test 3: Response has shortCode (6 alphanumeric chars)
    let shortCode = '';
    try {
      const body = JSON.parse(shortenRes.body);
      shortCode = body.shortCode || '';
      const isValid = /^[a-zA-Z0-9]{6}$/.test(shortCode);
      assert('shortCode is 6 alphanumeric characters', isValid, `Got "${shortCode}"`);
    } catch {
      assert('shortCode is 6 alphanumeric characters', false, `Body not JSON: ${shortenRes.body}`);
    }

    // Test 4: Response has shortUrl containing shortCode
    try {
      const body = JSON.parse(shortenRes.body);
      const shortUrl = body.shortUrl || '';
      assert('shortUrl contains shortCode', shortUrl.includes(shortCode) && shortCode.length > 0,
        `shortUrl="${shortUrl}", shortCode="${shortCode}"`);
    } catch {
      assert('shortUrl contains shortCode', false, `Body not JSON`);
    }

    // Test 5: POST /shorten without url returns 400
    const noUrlRes = await request('/shorten', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({}),
    });
    assert('POST /shorten without url returns 400', noUrlRes.status === 400, `Got ${noUrlRes.status}`);

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
