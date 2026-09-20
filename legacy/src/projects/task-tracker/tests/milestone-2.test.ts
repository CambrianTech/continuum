#!/usr/bin/env tsx
/**
 * Milestone 2: Filtering, Pagination, and Sorting
 *
 * Tests:
 * 1. GET /tasks?status=pending filters by status
 * 2. GET /tasks?priority=2 filters by priority
 * 3. GET /tasks?search=deploy does case-insensitive title search
 * 4. GET /tasks?page=1&limit=2 returns paginated results
 * 5. Pagination response includes metadata
 * 6. GET /tasks?sort=priority_desc sorts correctly
 * 7. Multiple filters combine
 * 8. Previous health endpoint still works
 */

import http from 'http';

const PORT = 3460;
const BASE = `http://localhost:${PORT}`;

let passed = 0;
let failed = 0;
let server: any;

function request(
  urlPath: string,
  options: http.RequestOptions & { body?: string } = {},
): Promise<{ status: number; body: string; headers: http.IncomingHttpHeaders }> {
  return new Promise((resolve, reject) => {
    const url = new URL(urlPath, BASE);
    const { body, ...reqOpts } = options;
    if (body && !reqOpts.headers) {
      reqOpts.headers = { 'Content-Type': 'application/json' };
    }
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
    console.log(`\u2705 ${name}`);
    passed++;
  } else {
    console.log(`\u274C ${name}${detail ? ` \u2014 ${detail}` : ''}`);
    failed++;
  }
}

/**
 * Parse response body supporting both array and {tasks:[...],pagination:{...}} formats.
 */
function parseTasks(body: string): { tasks: any[]; pagination?: any } {
  try {
    const parsed = JSON.parse(body);
    if (Array.isArray(parsed)) {
      return { tasks: parsed };
    }
    return {
      tasks: parsed.tasks || [],
      pagination: parsed.pagination || parsed.meta,
    };
  } catch {
    return { tasks: [] };
  }
}

const TOTAL_TESTS = 8;

async function main() {
  console.log('Milestone 2: Filtering, Pagination, and Sorting');
  console.log('\u2500'.repeat(50));

  try {
    const mod = await import('../src/index');
    const app = mod.app || mod.default;
    if (!app) {
      console.log('\u274C Could not import app from src/index.ts');
      console.log(`\nResults: 0 passed, ${TOTAL_TESTS} failed`);
      process.exit(1);
    }
    server = app.listen(PORT);
    await new Promise(r => setTimeout(r, 500));
  } catch (err) {
    console.log(`\u274C Failed to start server: ${err}`);
    console.log(`\nResults: 0 passed, ${TOTAL_TESTS} failed`);
    process.exit(1);
  }

  try {
    // Seed test data: create 5 tasks with varying status, priority, titles
    const seedTasks = [
      { title: 'Deploy to production', status: 'pending', priority: 3 },
      { title: 'Write unit tests', status: 'in_progress', priority: 2 },
      { title: 'Fix login bug', status: 'done', priority: 3 },
      { title: 'Update README', status: 'pending', priority: 0 },
      { title: 'Deploy staging server', status: 'pending', priority: 1 },
    ];

    for (const task of seedTasks) {
      await request('/tasks', {
        method: 'POST',
        body: JSON.stringify(task),
      });
    }

    // Test 1: Filter by status
    const statusRes = await request('/tasks?status=pending');
    const statusData = parseTasks(statusRes.body);
    const allPending = statusData.tasks.every((t: any) => t.status === 'pending');
    assert('GET /tasks?status=pending filters by status',
      statusRes.status === 200 && statusData.tasks.length === 3 && allPending,
      `Status: ${statusRes.status}, count: ${statusData.tasks.length}, allPending: ${allPending}`);

    // Test 2: Filter by priority
    const priorityRes = await request('/tasks?priority=3');
    const priorityData = parseTasks(priorityRes.body);
    const allP3 = priorityData.tasks.every((t: any) => t.priority === 3 || t.priority === '3');
    assert('GET /tasks?priority=2 filters by priority',
      priorityRes.status === 200 && priorityData.tasks.length === 2 && allP3,
      `Status: ${priorityRes.status}, count: ${priorityData.tasks.length}`);

    // Test 3: Search by title (case-insensitive)
    const searchRes = await request('/tasks?search=deploy');
    const searchData = parseTasks(searchRes.body);
    const allMatchDeploy = searchData.tasks.every((t: any) =>
      t.title.toLowerCase().includes('deploy'));
    assert('GET /tasks?search=deploy does case-insensitive search',
      searchRes.status === 200 && searchData.tasks.length === 2 && allMatchDeploy,
      `Status: ${searchRes.status}, count: ${searchData.tasks.length}`);

    // Test 4: Pagination
    const pageRes = await request('/tasks?page=1&limit=2');
    const pageData = parseTasks(pageRes.body);
    assert('GET /tasks?page=1&limit=2 returns paginated results',
      pageRes.status === 200 && pageData.tasks.length === 2,
      `Status: ${pageRes.status}, count: ${pageData.tasks.length}`);

    // Test 5: Pagination metadata
    const hasPagination = pageData.pagination &&
      typeof pageData.pagination.page !== 'undefined' &&
      typeof pageData.pagination.total !== 'undefined' &&
      typeof pageData.pagination.totalPages !== 'undefined';
    assert('Pagination response includes metadata',
      hasPagination === true,
      `Pagination: ${JSON.stringify(pageData.pagination)}`);

    // Test 6: Sort by priority descending
    const sortRes = await request('/tasks?sort=priority_desc');
    const sortData = parseTasks(sortRes.body);
    let isSorted = true;
    for (let i = 1; i < sortData.tasks.length; i++) {
      if (Number(sortData.tasks[i].priority) > Number(sortData.tasks[i - 1].priority)) {
        isSorted = false;
        break;
      }
    }
    assert('GET /tasks?sort=priority_desc sorts correctly',
      sortRes.status === 200 && sortData.tasks.length >= 5 && isSorted,
      `Status: ${sortRes.status}, sorted: ${isSorted}, count: ${sortData.tasks.length}`);

    // Test 7: Multiple filters combine (status=pending AND search=deploy)
    const comboRes = await request('/tasks?status=pending&search=deploy');
    const comboData = parseTasks(comboRes.body);
    const comboCorrect = comboData.tasks.every((t: any) =>
      t.status === 'pending' && t.title.toLowerCase().includes('deploy'));
    assert('Multiple filters combine correctly',
      comboRes.status === 200 && comboData.tasks.length === 2 && comboCorrect,
      `Status: ${comboRes.status}, count: ${comboData.tasks.length}`);

    // Test 8: Health endpoint still works (regression)
    const healthRes = await request('/health');
    assert('GET /health still returns 200',
      healthRes.status === 200,
      `Got ${healthRes.status}`);

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
