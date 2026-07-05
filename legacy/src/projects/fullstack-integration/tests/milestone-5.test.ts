#!/usr/bin/env tsx
/**
 * Milestone 5: Analytics Pipeline
 *
 * Tests:
 *  1. GET /products/:id increments view count in analytics
 *  2. POST /cart/items increments cartAdds for that product
 *  3. POST /orders increments purchases and totalRevenue
 *  4. GET /analytics/summary returns totalOrders, totalRevenue, uniqueCustomers, totalProductViews
 *  5. GET /analytics/products returns products with metrics, sorted by views descending
 *  6. GET /analytics/revenue returns last 7 days with date, revenue, orderCount
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
): Promise<{ status: number; body: string }> {
  return new Promise((resolve, reject) => {
    const url = new URL(path, BASE);
    const { body, ...reqOpts } = options;
    const req = http.request(url, reqOpts, (res) => {
      let data = '';
      res.on('data', (chunk) => (data += chunk));
      res.on('end', () => resolve({ status: res.statusCode!, body: data }));
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

const TOTAL_TESTS = 6;

async function main() {
  console.log('Milestone 5: Analytics Pipeline');
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
    // Setup: register user and get products
    await request('/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'dave@test.com', password: 'pass123', name: 'Dave' }),
    });
    const loginRes = await request('/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'dave@test.com', password: 'pass123' }),
    });
    let token = '';
    try { token = JSON.parse(loginRes.body).token || ''; } catch {}

    const productsRes = await request('/products');
    let products: any[] = [];
    try { products = JSON.parse(productsRes.body); } catch {}

    const p1 = products[0];
    const p2 = products[1];
    const sessionId = 'analytics-session-001';

    // Baseline analytics before we generate any events
    const baselineRes = await request('/analytics/summary');
    let baseline: any = {};
    try { baseline = JSON.parse(baselineRes.body); } catch {}
    const baseViews = baseline.totalProductViews ?? 0;
    const baseOrders = baseline.totalOrders ?? 0;
    const baseRevenue = baseline.totalRevenue ?? 0;

    // Test 1: GET /products/:id increments view count
    await request(`/products/${p1.id}`);
    await request(`/products/${p1.id}`);
    await request(`/products/${p1.id}`);  // 3 views on p1
    await request(`/products/${p2.id}`);  // 1 view on p2

    const afterViewsRes = await request('/analytics/summary');
    let afterViews: any = {};
    try { afterViews = JSON.parse(afterViewsRes.body); } catch {}
    assert(
      'GET /products/:id increments totalProductViews in analytics',
      afterViewsRes.status === 200 &&
        typeof afterViews.totalProductViews === 'number' &&
        afterViews.totalProductViews >= baseViews + 4,
      `Before: ${baseViews}, After: ${afterViews.totalProductViews} (expected +4)`,
    );

    // Test 2: POST /cart/items increments cartAdds
    // Get baseline cartAdds for p1
    const baseProductsRes = await request('/analytics/products');
    let baseProducts: any[] = [];
    try { baseProducts = JSON.parse(baseProductsRes.body); } catch {}
    const p1Baseline = baseProducts.find((p: any) => p.productId === p1.id);
    const baseCartAdds = p1Baseline?.cartAdds ?? 0;

    await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': sessionId },
      body: JSON.stringify({ productId: p1.id, quantity: 1 }),
    });
    await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': sessionId },
      body: JSON.stringify({ productId: p1.id, quantity: 1 }),
    });  // 2 cart adds for p1

    const afterCartRes = await request('/analytics/products');
    let afterCartProducts: any[] = [];
    try { afterCartProducts = JSON.parse(afterCartRes.body); } catch {}
    const p1AfterCart = afterCartProducts.find((p: any) => p.productId === p1.id);
    assert(
      'POST /cart/items increments cartAdds for that product',
      afterCartRes.status === 200 &&
        p1AfterCart &&
        typeof p1AfterCart.cartAdds === 'number' &&
        p1AfterCart.cartAdds >= baseCartAdds + 2,
      `Before: ${baseCartAdds}, After: ${p1AfterCart?.cartAdds}`,
    );

    // Test 3: POST /orders increments purchases and totalRevenue
    // Put item in cart then place order
    await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': 'analytics-order-session' },
      body: JSON.stringify({ productId: p1.id, quantity: 1 }),
    });
    const orderRes = await request('/orders', {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}`, 'X-Session-Id': 'analytics-order-session' },
    });
    let order: any = {};
    try { order = JSON.parse(orderRes.body); } catch {}
    const orderTotal = order.total ?? 0;

    const afterOrderRes = await request('/analytics/summary');
    let afterOrder: any = {};
    try { afterOrder = JSON.parse(afterOrderRes.body); } catch {}
    assert(
      'POST /orders increments totalOrders and totalRevenue',
      afterOrderRes.status === 200 &&
        typeof afterOrder.totalOrders === 'number' &&
        afterOrder.totalOrders >= baseOrders + 1 &&
        typeof afterOrder.totalRevenue === 'number' &&
        afterOrder.totalRevenue >= baseRevenue + orderTotal - 0.01,
      `Orders: ${baseOrders}→${afterOrder.totalOrders}, Revenue: ${baseRevenue}→${afterOrder.totalRevenue} (orderTotal: ${orderTotal})`,
    );

    // Test 4: GET /analytics/summary has all required fields
    const summaryRes = await request('/analytics/summary');
    let summary: any = {};
    try { summary = JSON.parse(summaryRes.body); } catch {}
    assert(
      'GET /analytics/summary has totalOrders, totalRevenue, uniqueCustomers, totalProductViews',
      summaryRes.status === 200 &&
        typeof summary.totalOrders === 'number' &&
        typeof summary.totalRevenue === 'number' &&
        typeof summary.uniqueCustomers === 'number' &&
        typeof summary.totalProductViews === 'number',
      `Keys: ${Object.keys(summary).join(', ')}`,
    );

    // Test 5: GET /analytics/products returns sorted array with metrics
    const analyticsProductsRes = await request('/analytics/products');
    let analyticsProducts: any[] = [];
    try { analyticsProducts = JSON.parse(analyticsProductsRes.body); } catch {}
    const hasProductMetrics =
      Array.isArray(analyticsProducts) &&
      analyticsProducts.length > 0 &&
      analyticsProducts.every(
        (p: any) =>
          typeof p.productId === 'string' &&
          typeof p.name === 'string' &&
          typeof p.views === 'number' &&
          typeof p.cartAdds === 'number' &&
          typeof p.purchases === 'number',
      );
    const isSortedByViews =
      analyticsProducts.length < 2 ||
      analyticsProducts.every((p: any, i: number) =>
        i === 0 ? true : p.views <= analyticsProducts[i - 1].views,
      );
    assert(
      'GET /analytics/products returns products with {productId, name, views, cartAdds, purchases} sorted by views',
      analyticsProductsRes.status === 200 && hasProductMetrics && isSortedByViews,
      `Count: ${analyticsProducts.length}, sorted: ${isSortedByViews}, firstKeys: ${Object.keys(analyticsProducts[0] || {}).join(', ')}`,
    );

    // Test 6: GET /analytics/revenue returns last 7 days
    const revenueRes = await request('/analytics/revenue');
    let revenueBody: any = {};
    try { revenueBody = JSON.parse(revenueRes.body); } catch {}
    const dailyArr = revenueBody.daily;
    const hasRevenue =
      revenueRes.status === 200 &&
      Array.isArray(dailyArr) &&
      dailyArr.length === 7 &&
      dailyArr.every(
        (d: any) =>
          typeof d.date === 'string' &&
          typeof d.revenue === 'number' &&
          typeof d.orderCount === 'number',
      );
    assert(
      'GET /analytics/revenue returns last 7 days with {date, revenue, orderCount}',
      hasRevenue,
      `Status: ${revenueRes.status}, days: ${dailyArr?.length}, sample: ${JSON.stringify(dailyArr?.[0])}`,
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
