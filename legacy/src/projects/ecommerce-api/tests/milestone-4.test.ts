#!/usr/bin/env tsx
/**
 * Milestone 4: User Authentication
 *
 * Tests (includes M1-M3 regression):
 * 1. GET /products returns 6 products (M1 regression)
 * 2. Cart operations work (M3 regression)
 * 3. POST /auth/register creates user
 * 4. POST /auth/register rejects duplicate email (409)
 * 5. POST /auth/register rejects missing fields (400)
 * 6. POST /auth/login returns token
 * 7. POST /auth/login rejects wrong password (401)
 * 8. GET /auth/me returns profile with valid token
 * 9. GET /auth/me returns 401 without token
 */

import http from 'http';

const PORT = 3457;
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

const TOTAL_TESTS = 9;

async function main() {
  console.log('Milestone 4: User Authentication');
  console.log('─'.repeat(50));

  try {
    const mod = await import('../src/index');
    const app = mod.app || mod.default;
    if (!app) {
      console.log('❌ Could not import app');
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
    // Test 1: M1 regression
    const allRes = await request('/products');
    assert('M1: Products endpoint works',
      allRes.status === 200 && JSON.parse(allRes.body).length === 6,
      `Status: ${allRes.status}`);

    // Test 2: M3 regression — cart operations
    const addCart = await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': 'auth-test-session' },
      body: JSON.stringify({ productId: 'p1', quantity: 1 }),
    });
    assert('M3: Cart add works',
      addCart.status === 200 || addCart.status === 201,
      `Status: ${addCart.status}`);

    // Test 3: Register user
    const registerRes = await request('/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'alice@test.com', password: 'secret123', name: 'Alice' }),
    });
    let regBody: any = {};
    try { regBody = JSON.parse(registerRes.body); } catch {}
    assert('POST /auth/register creates user',
      registerRes.status === 201 && regBody.email === 'alice@test.com' && regBody.name === 'Alice',
      `Status: ${registerRes.status}, body: ${registerRes.body.slice(0, 100)}`);

    // Test 4: Duplicate email
    const dupeRes = await request('/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'alice@test.com', password: 'other', name: 'Other' }),
    });
    assert('Duplicate email returns 409',
      dupeRes.status === 409,
      `Got ${dupeRes.status}`);

    // Test 5: Missing fields
    const missingRes = await request('/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'bob@test.com' }),
    });
    assert('Missing fields returns 400',
      missingRes.status === 400,
      `Got ${missingRes.status}`);

    // Test 6: Login
    const loginRes = await request('/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'alice@test.com', password: 'secret123' }),
    });
    let loginBody: any = {};
    try { loginBody = JSON.parse(loginRes.body); } catch {}
    const token = loginBody.token || '';
    assert('POST /auth/login returns token',
      loginRes.status === 200 && token.length > 0,
      `Status: ${loginRes.status}, hasToken: ${token.length > 0}`);

    // Test 7: Wrong password
    const wrongPwRes = await request('/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'alice@test.com', password: 'wrongpassword' }),
    });
    assert('Wrong password returns 401',
      wrongPwRes.status === 401,
      `Got ${wrongPwRes.status}`);

    // Test 8: GET /auth/me with valid token
    const meRes = await request('/auth/me', {
      headers: { 'Authorization': `Bearer ${token}` },
    });
    let meBody: any = {};
    try { meBody = JSON.parse(meRes.body); } catch {}
    assert('GET /auth/me returns profile',
      meRes.status === 200 && meBody.email === 'alice@test.com',
      `Status: ${meRes.status}, email: ${meBody.email}`);

    // Test 9: GET /auth/me without token
    const noAuthRes = await request('/auth/me');
    assert('GET /auth/me without token returns 401',
      noAuthRes.status === 401,
      `Got ${noAuthRes.status}`);

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
