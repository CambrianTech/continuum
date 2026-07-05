#!/usr/bin/env tsx
/**
 * Milestone 6: Admin Product Management
 *
 * Tests (includes M1-M5 regression):
 * 1. GET /products works (M1 regression)
 * 2. Auth works (M4 regression)
 * 3. POST /admin/products creates product (admin)
 * 4. Created product appears in catalog
 * 5. PUT /admin/products/:id updates product
 * 6. DELETE /admin/products/:id removes product
 * 7. Non-admin user gets 403
 * 8. Unauthenticated request gets 401
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

const TOTAL_TESTS = 8;

async function main() {
  console.log('Milestone 6: Admin Product Management');
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
      prodRes.status === 200 && JSON.parse(prodRes.body).length >= 6,
      `Status: ${prodRes.status}`);

    // Test 2: M4 regression — first user = admin
    const adminToken = await registerAndLogin('admin@store.com', 'admin123', 'Admin');
    assert('M4: Auth works (first user = admin)',
      adminToken.length > 0,
      `Token length: ${adminToken.length}`);

    // Register a second (non-admin) user
    const userToken = await registerAndLogin('user@store.com', 'user123', 'Regular User');

    // Test 3: Admin creates product
    const createRes = await request('/admin/products', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${adminToken}`,
      },
      body: JSON.stringify({
        name: 'Gaming Keyboard',
        price: 79.99,
        category: 'electronics',
        description: 'Mechanical gaming keyboard with RGB',
        inStock: true,
      }),
    });
    let newProduct: any = {};
    try { newProduct = JSON.parse(createRes.body); } catch {}
    assert('POST /admin/products creates product',
      createRes.status === 201 && newProduct.name === 'Gaming Keyboard',
      `Status: ${createRes.status}, name: ${newProduct.name}`);

    // Test 4: New product in catalog
    const catalogRes = await request('/products');
    const catalog = JSON.parse(catalogRes.body);
    const found = catalog.find((p: any) => p.name === 'Gaming Keyboard');
    assert('Created product appears in catalog',
      found !== undefined,
      `Catalog size: ${catalog.length}, found: ${!!found}`);

    const newId = newProduct.id || found?.id;

    // Test 5: Update product
    const updateRes = await request(`/admin/products/${newId}`, {
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${adminToken}`,
      },
      body: JSON.stringify({ price: 69.99 }),
    });
    assert('PUT /admin/products/:id updates product',
      updateRes.status === 200,
      `Status: ${updateRes.status}`);

    // Test 6: Delete product
    const deleteRes = await request(`/admin/products/${newId}`, {
      method: 'DELETE',
      headers: { 'Authorization': `Bearer ${adminToken}` },
    });
    const afterDelete = await request('/products');
    const afterCatalog = JSON.parse(afterDelete.body);
    const stillExists = afterCatalog.find((p: any) => p.id === newId);
    assert('DELETE /admin/products/:id removes product',
      deleteRes.status === 200 && !stillExists,
      `Status: ${deleteRes.status}, stillExists: ${!!stillExists}`);

    // Test 7: Non-admin gets 403
    const nonAdminRes = await request('/admin/products', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${userToken}`,
      },
      body: JSON.stringify({ name: 'Hack', price: 0, category: 'test', description: 'x', inStock: true }),
    });
    assert('Non-admin user gets 403',
      nonAdminRes.status === 403,
      `Got ${nonAdminRes.status}`);

    // Test 8: Unauthenticated gets 401
    const noAuthRes = await request('/admin/products', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: 'Hack', price: 0, category: 'test', description: 'x', inStock: true }),
    });
    assert('Unauthenticated gets 401',
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
