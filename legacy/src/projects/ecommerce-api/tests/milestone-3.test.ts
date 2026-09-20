#!/usr/bin/env tsx
/**
 * Milestone 3: Shopping Cart
 *
 * Tests (includes M1+M2 regression):
 * 1. GET /products returns 6 products (M1 regression)
 * 2. GET /products?category=books filters (M2 regression)
 * 3. POST /cart/items adds product to cart
 * 4. GET /cart returns items with subtotals and total
 * 5. PUT /cart/items/:productId updates quantity
 * 6. DELETE /cart/items/:productId removes item
 * 7. Missing X-Session-Id returns 400
 * 8. Invalid productId returns 404
 * 9. Separate sessions have independent carts
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
const SESSION_A = 'session-test-a';
const SESSION_B = 'session-test-b';

async function main() {
  console.log('Milestone 3: Shopping Cart');
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
    const all = JSON.parse(allRes.body);
    assert('M1: GET /products returns 6 products',
      allRes.status === 200 && Array.isArray(all) && all.length === 6,
      `Count: ${Array.isArray(all) ? all.length : 'not array'}`);

    // Test 2: M2 regression
    const booksRes = await request('/products?category=books');
    const books = JSON.parse(booksRes.body);
    assert('M2: Filter by category works',
      Array.isArray(books) && books.length === 2,
      `Count: ${books.length}`);

    // Test 3: Add item to cart
    const addRes = await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': SESSION_A },
      body: JSON.stringify({ productId: 'p1', quantity: 2 }),
    });
    assert('POST /cart/items adds product',
      addRes.status === 201 || addRes.status === 200,
      `Status: ${addRes.status}`);

    // Add a second product
    await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': SESSION_A },
      body: JSON.stringify({ productId: 'p3', quantity: 1 }),
    });

    // Test 4: Get cart with totals
    const cartRes = await request('/cart', {
      headers: { 'X-Session-Id': SESSION_A },
    });
    const cart = JSON.parse(cartRes.body);
    const hasItems = Array.isArray(cart.items) && cart.items.length === 2;
    const hasTotal = typeof cart.total === 'number' && cart.total > 0;
    assert('GET /cart returns items with total',
      cartRes.status === 200 && hasItems && hasTotal,
      `Items: ${cart.items?.length}, total: ${cart.total}`);

    // Test 5: Update quantity
    const updateRes = await request('/cart/items/p1', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': SESSION_A },
      body: JSON.stringify({ quantity: 5 }),
    });
    assert('PUT /cart/items/:id updates quantity',
      updateRes.status === 200,
      `Status: ${updateRes.status}`);

    // Test 6: Delete item
    const deleteRes = await request('/cart/items/p3', {
      method: 'DELETE',
      headers: { 'X-Session-Id': SESSION_A },
    });
    const cartAfterDelete = await request('/cart', {
      headers: { 'X-Session-Id': SESSION_A },
    });
    const remaining = JSON.parse(cartAfterDelete.body);
    assert('DELETE /cart/items/:id removes item',
      deleteRes.status === 200 && remaining.items?.length === 1,
      `Status: ${deleteRes.status}, remaining: ${remaining.items?.length}`);

    // Test 7: Missing session header
    const noSessionRes = await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ productId: 'p1', quantity: 1 }),
    });
    assert('Missing X-Session-Id returns 400',
      noSessionRes.status === 400,
      `Got ${noSessionRes.status}`);

    // Test 8: Invalid product ID
    const badProductRes = await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': SESSION_A },
      body: JSON.stringify({ productId: 'nonexistent', quantity: 1 }),
    });
    assert('Invalid productId returns 404',
      badProductRes.status === 404,
      `Got ${badProductRes.status}`);

    // Test 9: Independent sessions
    await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': SESSION_B },
      body: JSON.stringify({ productId: 'p2', quantity: 3 }),
    });
    const cartB = await request('/cart', { headers: { 'X-Session-Id': SESSION_B } });
    const cartBData = JSON.parse(cartB.body);
    const cartA = await request('/cart', { headers: { 'X-Session-Id': SESSION_A } });
    const cartAData = JSON.parse(cartA.body);
    assert('Sessions have independent carts',
      cartBData.items?.length === 1 && cartAData.items?.length === 1 &&
      cartBData.items[0]?.productId !== cartAData.items[0]?.productId,
      `A: ${cartAData.items?.length} items, B: ${cartBData.items?.length} items`);

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
