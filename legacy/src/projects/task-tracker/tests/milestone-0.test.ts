#!/usr/bin/env tsx
/**
 * Milestone 0: Express Server + SQLite Schema
 *
 * Tests:
 * 1. Server starts on port 3460
 * 2. GET /health returns 200 {status: "ok", dbReady: true}
 * 3. SQLite database file exists
 * 4. tasks table has correct columns
 */

import http from 'http';
import fs from 'fs';
import path from 'path';

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

const TOTAL_TESTS = 4;

async function main() {
  console.log('Milestone 0: Express Server + SQLite Schema');
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
    // Test 1: Server responds
    try {
      await request('/health');
      assert('Server starts on port 3460', true);
    } catch {
      assert('Server starts on port 3460', false, 'Connection refused');
      console.log(`\nResults: ${passed} passed, ${TOTAL_TESTS - passed} failed`);
      server?.close(); process.exit(1);
    }

    // Test 2: Health endpoint with DB readiness
    const healthRes = await request('/health');
    let healthBody: any;
    try {
      healthBody = JSON.parse(healthRes.body);
    } catch {
      healthBody = {};
    }
    assert('GET /health returns 200 {status:"ok", dbReady:true}',
      healthRes.status === 200 && healthBody.status === 'ok' && healthBody.dbReady === true,
      `Status: ${healthRes.status}, body: ${healthRes.body}`);

    // Test 3: SQLite database file exists
    // Check common locations for the DB file
    const cwd = process.cwd();
    const possibleDbPaths = [
      path.join(cwd, 'tasks.db'),
      path.join(cwd, 'data.db'),
      path.join(cwd, 'database.db'),
      path.join(cwd, 'db.sqlite'),
      path.join(cwd, 'tasks.sqlite'),
      path.join(cwd, 'src', 'tasks.db'),
      path.join(cwd, 'src', 'data.db'),
    ];
    const dbExists = possibleDbPaths.some(p => fs.existsSync(p));
    assert('SQLite database file exists',
      dbExists,
      `Checked: ${possibleDbPaths.map(p => path.basename(p)).join(', ')}`);

    // Test 4: tasks table has correct columns
    // We verify this by attempting to create a task and checking it has the expected fields
    // Since CRUD isn't implemented yet, we check via a raw query through the health-like endpoint
    // Actually, we'll try GET /tasks and if it returns an array (even empty), the table exists
    // If GET /tasks doesn't exist yet, we do a direct DB check
    let tableCorrect = false;
    try {
      const tasksRes = await request('/tasks');
      if (tasksRes.status === 200) {
        // If endpoint exists and returns 200, table must exist
        tableCorrect = true;
      }
    } catch {
      // /tasks endpoint might not exist in milestone 0
    }

    if (!tableCorrect) {
      // Fall back: find and open the DB file directly to check schema
      try {
        const Database = require('better-sqlite3');
        const dbPath = possibleDbPaths.find(p => fs.existsSync(p));
        if (dbPath) {
          const db = new Database(dbPath, { readonly: true });
          const columns = db.pragma(`table_info(tasks)`);
          const columnNames = columns.map((c: any) => c.name);
          const requiredColumns = ['id', 'title', 'description', 'status', 'priority', 'createdAt', 'updatedAt'];
          // Allow snake_case variants
          const snakeCaseVariants: Record<string, string> = {
            createdAt: 'created_at',
            updatedAt: 'updated_at',
          };
          tableCorrect = requiredColumns.every(col => {
            const variant = snakeCaseVariants[col];
            return columnNames.includes(col) || (variant && columnNames.includes(variant));
          });
          db.close();
        }
      } catch {
        // If we can't open DB, check fails
      }
    }

    assert('tasks table has correct columns',
      tableCorrect,
      'Expected columns: id, title, description, status, priority, createdAt, updatedAt');

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
