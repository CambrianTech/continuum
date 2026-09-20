#!/usr/bin/env tsx
/**
 * Milestone 1: E-commerce Foundation
 *
 * Tests:
 *  1. Server starts on port 3458
 *  2. GET /health returns {status:'ok'}
 *  3. GET /products returns 6 seeded products
 *  4. Products have required fields
 *  5. POST /auth/register creates a user
 *  6. POST /auth/register rejects duplicate email (409)
 *  7. POST /auth/login returns token for valid credentials
 *  8. GET /auth/me with Bearer token returns profile
 *  9. POST /cart/items adds item to cart (X-Session-Id)
 * 10. GET /cart returns items with subtotal and total
 */

import http from 'http';

const PORT = 3458;
const BASE = `http://localhost:${PORT}`;

let passed = 0;
let failed = 0;
let httpServer: any;

function request(
  path: string,
  options: http.RequestOptions & { body?: string } = {},
): Promise<{ status: number; body: string; headers: http.IncomingHttpHeaders }> {
  return new Promise((resolve, reject) => {
    const url = new URL(path, BASE);
    const { body, ...reqOpts } = options;
    const req = http.request(url, reqOpts, (res) => {
      let data = '';
      res.on('data', (chunk) => (data += chunk));
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

const TOTAL_TESTS = 10;

async function main() {
  console.log('Milestone 1: E-commerce Foundation');
  console.log('─'.repeat(50));

  try {
    const mod = await import('../src/index');
    const startable = (mod as any).server || (mod as any).httpServer || (mod as any).app || (mod as any).default;
    if (!startable) {
      console.log('❌ Could not import server or app from src/index.ts');
      console.log(`\nResults: 0 passed, ${TOTAL_TESTS} failed`);
      process.exit(1);
    }
    httpServer = startable.listen ? startable.listen(PORT) : startable(PORT);
    await new Promise((r) => setTimeout(r, 500));
  } catch (err) {
    console.log(`❌ Failed to start server: ${err}`);
    console.log(`\nResults: 0 passed, ${TOTAL_TESTS} failed`);
    process.exit(1);
  }

  try {
    // Test 1: Server responds
    try {
      await request('/health');
      assert('Server starts on port 3458', true);
    } catch {
      assert('Server starts on port 3458', false, 'Connection refused');
      console.log(`\nResults: ${passed} passed, ${TOTAL_TESTS - passed} failed`);
      httpServer?.close();
      process.exit(1);
    }

    // Test 2: Health endpoint
    const healthRes = await request('/health');
    const healthBody = JSON.parse(healthRes.body);
    assert(
      'GET /health returns {status:"ok"}',
      healthRes.status === 200 && healthBody.status === 'ok',
      `Status: ${healthRes.status}, body: ${healthRes.body}`,
    );

    // Test 3: Product listing returns 6 products
    const listRes = await request('/products');
    let products: any[] = [];
    try {
      products = JSON.parse(listRes.body);
      assert(
        'GET /products returns 6 seeded products',
        listRes.status === 200 && Array.isArray(products) && products.length === 6,
        `Status: ${listRes.status}, count: ${Array.isArray(products) ? products.length : 'not array'}`,
      );
    } catch {
      assert('GET /products returns 6 seeded products', false, `Body: ${listRes.body.slice(0, 100)}`);
    }

    // Test 4: Products have required fields
    if (products.length > 0) {
      const p = products[0];
      const hasFields =
        'id' in p && 'name' in p && 'price' in p && 'category' in p && 'description' in p && 'inStock' in p;
      assert('Products have required fields (id, name, price, category, description, inStock)', hasFields, `Keys: ${Object.keys(p).join(', ')}`);
    } else {
      assert('Products have required fields', false, 'No products returned');
    }

    // Test 5: Register a user
    const regRes = await request('/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'alice@test.com', password: 'pass123', name: 'Alice' }),
    });
    let regBody: any = {};
    try {
      regBody = JSON.parse(regRes.body);
    } catch {}
    assert(
      'POST /auth/register creates user and returns {userId, email, name}',
      regRes.status === 201 && regBody.userId && regBody.email === 'alice@test.com' && regBody.name === 'Alice',
      `Status: ${regRes.status}, body: ${regRes.body.slice(0, 150)}`,
    );

    // Test 6: Duplicate email returns 409
    const dupRes = await request('/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'alice@test.com', password: 'other', name: 'Alice2' }),
    });
    assert(
      'POST /auth/register rejects duplicate email with 409',
      dupRes.status === 409,
      `Got ${dupRes.status}`,
    );

    // Test 7: Login returns token
    const loginRes = await request('/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'alice@test.com', password: 'pass123' }),
    });
    let token = '';
    try {
      const loginBody = JSON.parse(loginRes.body);
      token = loginBody.token || '';
      assert(
        'POST /auth/login returns {token}',
        loginRes.status === 200 && typeof token === 'string' && token.length > 0,
        `Status: ${loginRes.status}, token: ${token.slice(0, 40)}`,
      );
    } catch {
      assert('POST /auth/login returns {token}', false, `Body: ${loginRes.body.slice(0, 100)}`);
    }

    // Test 8: GET /auth/me with token
    const meRes = await request('/auth/me', {
      method: 'GET',
      headers: { Authorization: `Bearer ${token}` },
    });
    let meBody: any = {};
    try {
      meBody = JSON.parse(meRes.body);
    } catch {}
    assert(
      'GET /auth/me with valid token returns user profile',
      meRes.status === 200 && meBody.email === 'alice@test.com',
      `Status: ${meRes.status}, body: ${meRes.body.slice(0, 100)}`,
    );

    // Test 9: Add item to cart
    const sessionId = 'test-session-001';
    const firstProductId = products[0]?.id || 'p1';
    const cartAddRes = await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': sessionId },
      body: JSON.stringify({ productId: firstProductId, quantity: 2 }),
    });
    assert(
      'POST /cart/items adds item to cart',
      cartAddRes.status === 200 || cartAddRes.status === 201,
      `Status: ${cartAddRes.status}, body: ${cartAddRes.body.slice(0, 100)}`,
    );

    // Test 10: GET /cart returns items with subtotal and total
    const cartRes = await request('/cart', {
      method: 'GET',
      headers: { 'X-Session-Id': sessionId },
    });
    let cartBody: any = {};
    try {
      cartBody = JSON.parse(cartRes.body);
    } catch {}
    const hasCart =
      cartRes.status === 200 &&
      Array.isArray(cartBody.items) &&
      cartBody.items.length > 0 &&
      typeof cartBody.total === 'number' &&
      typeof cartBody.items[0].subtotal === 'number';
    assert(
      'GET /cart returns items with subtotal and total',
      hasCart,
      `Status: ${cartRes.status}, body: ${cartRes.body.slice(0, 150)}`,
    );
  } finally {
    httpServer?.close();
  }

  console.log(`\nResults: ${passed} passed, ${failed} failed`);
  process.exit(failed > 0 ? 1 : 0);
}

main().catch((err) => {
  console.error('Fatal:', err);
  httpServer?.close();
  console.log(`\nResults: ${passed} passed, ${failed + 1} failed`);
  process.exit(1);
});
