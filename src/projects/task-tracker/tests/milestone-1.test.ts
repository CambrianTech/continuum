#!/usr/bin/env tsx
/**
 * Milestone 1: Task CRUD Operations
 *
 * Tests:
 * 1. POST /tasks creates a task with title
 * 2. POST /tasks without title returns 400
 * 3. POST /tasks with invalid status returns 400
 * 4. GET /tasks returns array of tasks
 * 5. GET /tasks/:id returns single task
 * 6. GET /tasks/:id with unknown id returns 404
 * 7. PUT /tasks/:id updates fields
 * 8. DELETE /tasks/:id returns 204
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

const TOTAL_TESTS = 8;

async function main() {
  console.log('Milestone 1: Task CRUD Operations');
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
    // Test 1: POST /tasks creates a task
    const createRes = await request('/tasks', {
      method: 'POST',
      body: JSON.stringify({ title: 'Write tests', description: 'Unit tests for the API', priority: 2 }),
    });
    let createdTask: any = {};
    try { createdTask = JSON.parse(createRes.body); } catch { /* empty */ }
    assert('POST /tasks creates task (201)',
      createRes.status === 201 &&
      createdTask.title === 'Write tests' &&
      typeof createdTask.id !== 'undefined' &&
      typeof createdTask.createdAt !== 'undefined',
      `Status: ${createRes.status}, body: ${createRes.body.slice(0, 200)}`);

    const taskId = createdTask.id;

    // Test 2: POST /tasks without title returns 400
    const noTitleRes = await request('/tasks', {
      method: 'POST',
      body: JSON.stringify({ description: 'No title provided' }),
    });
    assert('POST /tasks without title returns 400',
      noTitleRes.status === 400,
      `Got ${noTitleRes.status}`);

    // Test 3: POST /tasks with invalid status returns 400
    const badStatusRes = await request('/tasks', {
      method: 'POST',
      body: JSON.stringify({ title: 'Bad task', status: 'invalid_status' }),
    });
    assert('POST /tasks with invalid status returns 400',
      badStatusRes.status === 400,
      `Got ${badStatusRes.status}`);

    // Test 4: GET /tasks returns array
    // Create another task first for a richer list
    await request('/tasks', {
      method: 'POST',
      body: JSON.stringify({ title: 'Deploy app', status: 'pending', priority: 3 }),
    });
    const listRes = await request('/tasks');
    let tasks: any[] = [];
    try {
      const parsed = JSON.parse(listRes.body);
      // Support both array directly or {tasks: [...]} format
      tasks = Array.isArray(parsed) ? parsed : (parsed.tasks || []);
    } catch { /* empty */ }
    assert('GET /tasks returns array with tasks',
      listRes.status === 200 && tasks.length >= 2,
      `Status: ${listRes.status}, count: ${tasks.length}`);

    // Test 5: GET /tasks/:id returns single task
    if (taskId) {
      const singleRes = await request(`/tasks/${taskId}`);
      let singleTask: any = {};
      try { singleTask = JSON.parse(singleRes.body); } catch { /* empty */ }
      assert('GET /tasks/:id returns single task',
        singleRes.status === 200 && (singleTask.id === taskId || String(singleTask.id) === String(taskId)),
        `Status: ${singleRes.status}, body: ${singleRes.body.slice(0, 200)}`);
    } else {
      assert('GET /tasks/:id returns single task', false, 'No task ID from create');
    }

    // Test 6: GET /tasks/:id with unknown id returns 404
    const notFoundRes = await request('/tasks/99999');
    assert('GET /tasks/:id unknown returns 404',
      notFoundRes.status === 404,
      `Got ${notFoundRes.status}`);

    // Test 7: PUT /tasks/:id updates fields
    if (taskId) {
      const updateRes = await request(`/tasks/${taskId}`, {
        method: 'PUT',
        body: JSON.stringify({ status: 'in_progress', priority: 3 }),
      });
      let updatedTask: any = {};
      try { updatedTask = JSON.parse(updateRes.body); } catch { /* empty */ }
      assert('PUT /tasks/:id updates fields',
        updateRes.status === 200 &&
        updatedTask.status === 'in_progress' &&
        (updatedTask.priority === 3 || updatedTask.priority === '3'),
        `Status: ${updateRes.status}, body: ${updateRes.body.slice(0, 200)}`);
    } else {
      assert('PUT /tasks/:id updates fields', false, 'No task ID from create');
    }

    // Test 8: DELETE /tasks/:id returns 204
    if (taskId) {
      const deleteRes = await request(`/tasks/${taskId}`, { method: 'DELETE' });
      assert('DELETE /tasks/:id returns 204',
        deleteRes.status === 204,
        `Got ${deleteRes.status}`);

      // Verify it's actually gone
      const afterDelete = await request(`/tasks/${taskId}`);
      if (afterDelete.status !== 404) {
        console.log(`  (warning: task still exists after DELETE, got ${afterDelete.status})`);
      }
    } else {
      assert('DELETE /tasks/:id returns 204', false, 'No task ID from create');
    }

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
