#!/usr/bin/env tsx
/**
 * Milestone 3: Redirect + Click Stats
 *
 * Tests (includes M1+M2 regression):
 * 1. GET /health returns 200 {status: "ok"}
 * 2. POST /shorten creates a short URL
 * 3. GET /:code redirects (302) to original URL
 * 4. GET /stats/:code returns {url, shortCode, clicks}
 * 5. Clicks increment on each redirect
 * 6. Unknown code returns 404 on GET /:code
 * 7. Unknown code returns 404 on GET /stats/:code
 */

import http from 'http';

const PORT = 3456;
const BASE = `http://localhost:${PORT}`;

let passed = 0;
let failed = 0;
let server: any;

function request(
  path: string,
  options: http.RequestOptions & { body?: string; followRedirects?: boolean } = {},
): Promise<{ status: number; body: string; headers: http.IncomingHttpHeaders }> {
  return new Promise((resolve, reject) => {
    const url = new URL(path, BASE);
    const { body, followRedirects, ...reqOpts } = options;
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

const TOTAL_TESTS = 7;

async function main() {
  console.log('Milestone 3: Redirect + Click Stats');
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
    assert('GET /health returns 200',
      healthRes.status === 200 && JSON.parse(healthRes.body).status === 'ok',
      `Status: ${healthRes.status}`);

    // Test 2: M2 regression — create a short URL
    const shortenRes = await request('/shorten', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ url: 'https://example.com/test-page' }),
    });
    let shortCode = '';
    try {
      const body = JSON.parse(shortenRes.body);
      shortCode = body.shortCode || '';
      assert('POST /shorten creates short URL', shortenRes.status === 201 && shortCode.length === 6,
        `Status: ${shortenRes.status}, code: ${shortCode}`);
    } catch {
      assert('POST /shorten creates short URL', false, `Status: ${shortenRes.status}`);
    }

    if (!shortCode) {
      // Can't test redirect/stats without a valid shortCode
      console.log('⚠ Skipping redirect/stats tests — no valid shortCode');
      failed += 5;
      console.log(`\nResults: ${passed} passed, ${failed} failed`);
      server?.close();
      process.exit(1);
    }

    // Test 3: GET /:code redirects (302)
    const redirectRes = await request(`/${shortCode}`);
    assert('GET /:code returns 302 redirect',
      redirectRes.status === 302,
      `Got ${redirectRes.status}`);

    // Verify redirect location
    const location = redirectRes.headers.location || '';
    assert('Redirect location is original URL',
      location === 'https://example.com/test-page',
      `Location: ${location}`);

    // Test 4: GET /stats/:code returns stats
    // First do another redirect to get clicks > 0
    await request(`/${shortCode}`);

    const statsRes = await request(`/stats/${shortCode}`);
    try {
      const stats = JSON.parse(statsRes.body);
      assert('GET /stats/:code returns click stats',
        statsRes.status === 200 && stats.shortCode === shortCode && stats.clicks >= 2,
        `Status: ${statsRes.status}, clicks: ${stats.clicks}`);
    } catch {
      assert('GET /stats/:code returns click stats', false, `Body: ${statsRes.body}`);
    }

    // Test 5: Unknown code → 404 on redirect
    const unknownRedirect = await request('/zzzzzz');
    assert('Unknown code returns 404 (redirect)',
      unknownRedirect.status === 404,
      `Got ${unknownRedirect.status}`);

    // Test 6: Unknown code → 404 on stats
    const unknownStats = await request('/stats/zzzzzz');
    assert('Unknown code returns 404 (stats)',
      unknownStats.status === 404,
      `Got ${unknownStats.status}`);

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
