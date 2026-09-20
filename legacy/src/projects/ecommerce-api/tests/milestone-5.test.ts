#!/usr/bin/env tsx
/**
 * Milestone 5: Checkout and Orders
 *
 * Tests (includes M1-M4 regression):
 * 1. GET /products works (M1 regression)
 * 2. Auth register + login works (M4 regression)
 * 3. POST /orders creates order from cart
 * 4. Order has correct structure (items, total, status, createdAt)
 * 5. Cart is cleared after order
 * 6. POST /orders with empty cart returns 400
 * 7. POST /orders without auth returns 401
 * 8. GET /orders lists user's orders
 * 9. GET /orders/:id returns order detail
 * 10. GET /orders/:id for other user returns 404
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

async function registerAndLogin(email: string, password: string, name: string): Promise<string> {
  await request('/auth/register', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password, name }),
  });
  const loginRes = await request('/auth/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
  });
  const body = JSON.parse(loginRes.body);
  return body.token || '';
}

const TOTAL_TESTS = 10;

async function main() {
  console.log('Milestone 5: Checkout and Orders');
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
    const prodRes = await request('/products');
    assert('M1: Products endpoint works',
      prodRes.status === 200,
      `Status: ${prodRes.status}`);

    // Test 2: M4 regression — register + login
    const tokenA = await registerAndLogin('order-alice@test.com', 'pass123', 'Alice');
    assert('M4: Register + login works',
      tokenA.length > 0,
      `Token length: ${tokenA.length}`);

    // Add items to cart (using token owner's session)
    // Cart needs to be linked to the authenticated user for orders
    // Use the token's user as the session identifier
    const meRes = await request('/auth/me', {
      headers: { 'Authorization': `Bearer ${tokenA}` },
    });
    const meBody = JSON.parse(meRes.body);
    const sessionId = meBody.userId || meBody.id || 'order-alice-session';

    await request('/cart/items', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Session-Id': sessionId,
        'Authorization': `Bearer ${tokenA}`,
      },
      body: JSON.stringify({ productId: 'p1', quantity: 2 }),
    });
    await request('/cart/items', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Session-Id': sessionId,
        'Authorization': `Bearer ${tokenA}`,
      },
      body: JSON.stringify({ productId: 'p5', quantity: 1 }),
    });

    // Test 3: Create order
    const orderRes = await request('/orders', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${tokenA}`,
        'X-Session-Id': sessionId,
      },
    });
    let order: any = {};
    try { order = JSON.parse(orderRes.body); } catch {}
    assert('POST /orders creates order from cart',
      orderRes.status === 201 || orderRes.status === 200,
      `Status: ${orderRes.status}`);

    // Test 4: Order structure
    const hasStructure = order.id && Array.isArray(order.items) &&
      typeof order.total === 'number' && order.status === 'confirmed' && order.createdAt;
    assert('Order has correct structure',
      hasStructure,
      `Keys: ${Object.keys(order).join(', ')}, status: ${order.status}`);

    // Test 5: Cart cleared after order
    const cartAfter = await request('/cart', {
      headers: { 'X-Session-Id': sessionId, 'Authorization': `Bearer ${tokenA}` },
    });
    const cartData = JSON.parse(cartAfter.body);
    assert('Cart cleared after order',
      cartData.items?.length === 0 || cartData.total === 0,
      `Items: ${cartData.items?.length}`);

    // Test 6: Empty cart order returns 400
    const emptyOrderRes = await request('/orders', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${tokenA}`,
        'X-Session-Id': sessionId,
      },
    });
    assert('Empty cart returns 400',
      emptyOrderRes.status === 400,
      `Got ${emptyOrderRes.status}`);

    // Test 7: No auth returns 401
    const noAuthOrder = await request('/orders', { method: 'POST' });
    assert('POST /orders without auth returns 401',
      noAuthOrder.status === 401,
      `Got ${noAuthOrder.status}`);

    // Test 8: List orders
    const listRes = await request('/orders', {
      headers: { 'Authorization': `Bearer ${tokenA}` },
    });
    const orders = JSON.parse(listRes.body);
    assert('GET /orders lists user orders',
      listRes.status === 200 && Array.isArray(orders) && orders.length >= 1,
      `Count: ${Array.isArray(orders) ? orders.length : 'not array'}`);

    // Test 9: Order detail
    const orderId = order.id;
    const detailRes = await request(`/orders/${orderId}`, {
      headers: { 'Authorization': `Bearer ${tokenA}` },
    });
    const detail = JSON.parse(detailRes.body);
    assert('GET /orders/:id returns detail',
      detailRes.status === 200 && detail.id === orderId,
      `Status: ${detailRes.status}`);

    // Test 10: Other user can't see order
    const tokenB = await registerAndLogin('order-bob@test.com', 'pass456', 'Bob');
    const otherDetailRes = await request(`/orders/${orderId}`, {
      headers: { 'Authorization': `Bearer ${tokenB}` },
    });
    assert('Other user gets 404 for order',
      otherDetailRes.status === 404 || otherDetailRes.status === 403,
      `Got ${otherDetailRes.status}`);

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
