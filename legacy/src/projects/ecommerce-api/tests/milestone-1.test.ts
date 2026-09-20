#!/usr/bin/env tsx
/**
 * Milestone 1: Product Catalog API
 *
 * Tests:
 * 1. Server starts on port 3457
 * 2. GET /health returns 200 {status: "ok"}
 * 3. GET /products returns array of 6 products
 * 4. Products have required fields (id, name, price, category, description, inStock)
 * 5. GET /products/:id returns single product
 * 6. GET /products/:id with unknown ID returns 404
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

const TOTAL_TESTS = 6;

async function main() {
  console.log('Milestone 1: Product Catalog API');
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
    // Test 1: Server responds
    try {
      await request('/health');
      assert('Server starts on port 3457', true);
    } catch {
      assert('Server starts on port 3457', false, 'Connection refused');
      console.log(`\nResults: ${passed} passed, ${TOTAL_TESTS - passed} failed`);
      server?.close(); process.exit(1);
    }

    // Test 2: Health endpoint
    const healthRes = await request('/health');
    const healthBody = JSON.parse(healthRes.body);
    assert('GET /health returns 200 {status:"ok"}',
      healthRes.status === 200 && healthBody.status === 'ok',
      `Status: ${healthRes.status}, body: ${healthRes.body}`);

    // Test 3: Product listing returns 6 products
    const listRes = await request('/products');
    let products: any[] = [];
    try {
      products = JSON.parse(listRes.body);
      assert('GET /products returns 6 products',
        listRes.status === 200 && Array.isArray(products) && products.length === 6,
        `Status: ${listRes.status}, count: ${Array.isArray(products) ? products.length : 'not array'}`);
    } catch {
      assert('GET /products returns 6 products', false, `Body not JSON: ${listRes.body.slice(0, 100)}`);
    }

    // Test 4: Products have required fields
    if (products.length > 0) {
      const p = products[0];
      const hasFields = 'id' in p && 'name' in p && 'price' in p &&
        'category' in p && 'description' in p && 'inStock' in p;
      assert('Products have required fields',
        hasFields,
        `First product keys: ${Object.keys(p).join(', ')}`);
    } else {
      assert('Products have required fields', false, 'No products to check');
    }

    // Test 5: Single product by ID
    const singleRes = await request('/products/p1');
    try {
      const product = JSON.parse(singleRes.body);
      assert('GET /products/:id returns product',
        singleRes.status === 200 && product.id === 'p1' && product.name === 'Laptop Pro',
        `Status: ${singleRes.status}, id: ${product.id}`);
    } catch {
      assert('GET /products/:id returns product', false, `Body: ${singleRes.body.slice(0, 100)}`);
    }

    // Test 6: Unknown product returns 404
    const notFoundRes = await request('/products/nonexistent');
    assert('Unknown product returns 404',
      notFoundRes.status === 404,
      `Got ${notFoundRes.status}`);

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
