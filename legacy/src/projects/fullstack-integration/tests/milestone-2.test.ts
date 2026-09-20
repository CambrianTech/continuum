#!/usr/bin/env tsx
/**
 * Milestone 2: Checkout and Inventory
 *
 * Tests:
 *  1. GET /products/:id includes stockCount field
 *  2. POST /cart/items rejects out-of-stock product (400)
 *  3. POST /orders creates order from authenticated user's cart
 *  4. Order has status='confirmed', items, total, createdAt
 *  5. POST /orders clears the cart
 *  6. GET /orders lists user's orders
 *  7. GET /orders/:id returns order detail (404 for other user)
 *  8. Order creation decrements product stockCount
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

const TOTAL_TESTS = 8;

async function main() {
  console.log('Milestone 2: Checkout and Inventory');
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
    // Setup: register user, login, get token and products
    await request('/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'bob@test.com', password: 'pass123', name: 'Bob' }),
    });
    const loginRes = await request('/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'bob@test.com', password: 'pass123' }),
    });
    let token = '';
    let products: any[] = [];
    try {
      token = JSON.parse(loginRes.body).token || '';
    } catch {}
    const productsRes = await request('/products');
    try {
      products = JSON.parse(productsRes.body);
    } catch {}

    const inStockProduct = products.find((p: any) => p.inStock !== false) || products[0];
    const productId = inStockProduct?.id || 'p1';
    const sessionId = 'ms2-session-001';

    // Test 1: Products have stockCount
    const singleRes = await request(`/products/${productId}`);
    let singleProduct: any = {};
    try {
      singleProduct = JSON.parse(singleRes.body);
    } catch {}
    assert(
      'GET /products/:id includes stockCount field',
      singleRes.status === 200 && typeof singleProduct.stockCount === 'number',
      `stockCount: ${singleProduct.stockCount}, keys: ${Object.keys(singleProduct).join(', ')}`,
    );

    // Test 2: Out-of-stock product rejected
    // Admin sets stockCount to 0 via PUT /admin/products/:id
    // First registered user is admin — register admin user (may already exist, ignore 409)
    await request('/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'admin@test.com', password: 'adminpass', name: 'Admin' }),
    });
    const adminLogin = await request('/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'admin@test.com', password: 'adminpass' }),
    });
    let adminToken = '';
    try {
      adminToken = JSON.parse(adminLogin.body).token || '';
    } catch {}

    // Use last product as the out-of-stock test subject
    const outOfStockProduct = products[products.length - 1];
    if (outOfStockProduct && adminToken) {
      await request(`/admin/products/${outOfStockProduct.id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${adminToken}` },
        body: JSON.stringify({ stockCount: 0, inStock: false }),
      });
    }

    const oosRes = await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': sessionId },
      body: JSON.stringify({ productId: outOfStockProduct?.id || 'p_oos', quantity: 1 }),
    });
    assert(
      'POST /cart/items rejects out-of-stock product with 400',
      oosRes.status === 400,
      `Got status ${oosRes.status}`,
    );

    // Add in-stock item to cart for order tests
    await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': sessionId },
      body: JSON.stringify({ productId, quantity: 1 }),
    });

    // Test 3: POST /orders creates order
    const orderRes = await request('/orders', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}`, 'X-Session-Id': sessionId },
    });
    let order: any = {};
    try {
      order = JSON.parse(orderRes.body);
    } catch {}
    assert(
      'POST /orders creates order from cart',
      orderRes.status === 201 && order.id && Array.isArray(order.items) && order.items.length > 0,
      `Status: ${orderRes.status}, body: ${orderRes.body.slice(0, 150)}`,
    );

    // Test 4: Order shape
    assert(
      "Order has status='confirmed', total, and createdAt",
      order.status === 'confirmed' && typeof order.total === 'number' && order.createdAt,
      `status: ${order.status}, total: ${order.total}, createdAt: ${order.createdAt}`,
    );

    // Test 5: Cart is cleared after order
    const cartAfterRes = await request('/cart', {
      headers: { 'X-Session-Id': sessionId },
    });
    let cartAfter: any = {};
    try {
      cartAfter = JSON.parse(cartAfterRes.body);
    } catch {}
    assert(
      'POST /orders clears the cart',
      cartAfterRes.status === 200 && Array.isArray(cartAfter.items) && cartAfter.items.length === 0,
      `Cart items after order: ${JSON.stringify(cartAfter.items)}`,
    );

    // Test 6: GET /orders lists user's orders
    const ordersListRes = await request('/orders', {
      headers: { Authorization: `Bearer ${token}` },
    });
    let ordersList: any[] = [];
    try {
      ordersList = JSON.parse(ordersListRes.body);
    } catch {}
    assert(
      'GET /orders returns authenticated user orders list',
      ordersListRes.status === 200 && Array.isArray(ordersList) && ordersList.length >= 1,
      `Status: ${ordersListRes.status}, count: ${ordersList.length}`,
    );

    // Test 7: GET /orders/:id returns detail; 404 for another user
    const orderId = order.id;
    const detailRes = await request(`/orders/${orderId}`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    assert(
      'GET /orders/:id returns order detail',
      detailRes.status === 200,
      `Status: ${detailRes.status}`,
    );

    // Register second user and verify 404 for their access to first user's order
    await request('/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'carol@test.com', password: 'pass123', name: 'Carol' }),
    });
    const carolLogin = await request('/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'carol@test.com', password: 'pass123' }),
    });
    let carolToken = '';
    try {
      carolToken = JSON.parse(carolLogin.body).token || '';
    } catch {}
    const crossRes = await request(`/orders/${orderId}`, {
      headers: { Authorization: `Bearer ${carolToken}` },
    });
    assert(
      "GET /orders/:id returns 404 for another user's order",
      crossRes.status === 404,
      `Got ${crossRes.status}`,
    );

    // Test 8: stockCount decremented after order
    const afterStockRes = await request(`/products/${productId}`);
    let afterProduct: any = {};
    try {
      afterProduct = JSON.parse(afterStockRes.body);
    } catch {}
    const beforeCount = singleProduct.stockCount ?? Infinity;
    const afterCount = afterProduct.stockCount ?? -1;
    assert(
      'Order creation decrements product stockCount',
      afterCount < beforeCount,
      `Before: ${beforeCount}, After: ${afterCount}`,
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
