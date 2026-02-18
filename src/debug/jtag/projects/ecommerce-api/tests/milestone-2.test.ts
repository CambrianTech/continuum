#!/usr/bin/env tsx
/**
 * Milestone 2: Search, Filter, and Sort
 *
 * Tests (includes M1 regression):
 * 1. GET /products returns all 6 (M1 regression)
 * 2. Filter by category
 * 3. Search by name (case-insensitive)
 * 4. Filter by price range
 * 5. Sort by price ascending
 * 6. Sort by price descending
 * 7. Combined filters work together
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

const TOTAL_TESTS = 7;

async function main() {
  console.log('Milestone 2: Search, Filter, and Sort');
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
    // Test 1: M1 regression — all products
    const allRes = await request('/products');
    const all = JSON.parse(allRes.body);
    assert('GET /products returns all 6 products',
      allRes.status === 200 && Array.isArray(all) && all.length === 6,
      `Count: ${Array.isArray(all) ? all.length : 'not array'}`);

    // Test 2: Filter by category
    const electronicsRes = await request('/products?category=electronics');
    const electronics = JSON.parse(electronicsRes.body);
    assert('Filter by category=electronics',
      Array.isArray(electronics) && electronics.length === 2 &&
      electronics.every((p: any) => p.category === 'electronics'),
      `Count: ${electronics.length}, categories: ${electronics.map((p: any) => p.category).join(',')}`);

    // Test 3: Search by name (case-insensitive)
    const searchRes = await request('/products?search=laptop');
    const searched = JSON.parse(searchRes.body);
    assert('Search by name (case-insensitive)',
      Array.isArray(searched) && searched.length >= 1 &&
      searched.some((p: any) => p.name.toLowerCase().includes('laptop')),
      `Count: ${searched.length}`);

    // Test 4: Price range filter
    const priceRes = await request('/products?minPrice=20&maxPrice=45');
    const priced = JSON.parse(priceRes.body);
    const allInRange = priced.every((p: any) => p.price >= 20 && p.price <= 45);
    assert('Filter by price range (20-45)',
      Array.isArray(priced) && priced.length > 0 && allInRange,
      `Count: ${priced.length}, inRange: ${allInRange}`);

    // Test 5: Sort by price ascending
    const sortAscRes = await request('/products?sort=price_asc');
    const sortedAsc = JSON.parse(sortAscRes.body);
    let isAscending = true;
    for (let i = 1; i < sortedAsc.length; i++) {
      if (sortedAsc[i].price < sortedAsc[i - 1].price) { isAscending = false; break; }
    }
    assert('Sort by price ascending',
      Array.isArray(sortedAsc) && sortedAsc.length === 6 && isAscending,
      `First: $${sortedAsc[0]?.price}, Last: $${sortedAsc[sortedAsc.length - 1]?.price}`);

    // Test 6: Sort by price descending
    const sortDescRes = await request('/products?sort=price_desc');
    const sortedDesc = JSON.parse(sortDescRes.body);
    let isDescending = true;
    for (let i = 1; i < sortedDesc.length; i++) {
      if (sortedDesc[i].price > sortedDesc[i - 1].price) { isDescending = false; break; }
    }
    assert('Sort by price descending',
      Array.isArray(sortedDesc) && sortedDesc.length === 6 && isDescending,
      `First: $${sortedDesc[0]?.price}, Last: $${sortedDesc[sortedDesc.length - 1]?.price}`);

    // Test 7: Combined filters
    const combinedRes = await request('/products?category=books&sort=price_asc');
    const combined = JSON.parse(combinedRes.body);
    const allBooks = combined.every((p: any) => p.category === 'books');
    let booksAsc = true;
    for (let i = 1; i < combined.length; i++) {
      if (combined[i].price < combined[i - 1].price) { booksAsc = false; break; }
    }
    assert('Combined filters (category + sort)',
      Array.isArray(combined) && combined.length === 2 && allBooks && booksAsc,
      `Count: ${combined.length}, allBooks: ${allBooks}, sorted: ${booksAsc}`);

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
