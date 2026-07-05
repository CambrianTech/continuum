#!/usr/bin/env tsx
/**
 * Milestone 6: Dashboard and Integration
 *
 * Tests:
 *  1. GET /dashboard returns all three sections: ecommerce, whiteboard, realtime
 *  2. ecommerce section has totalOrders, totalRevenue, topProducts array
 *  3. whiteboard section has activeSessions and totalOperations
 *  4. realtime section has connectedClients
 *  5. GET /dashboard/activity returns array of last 20 cross-system events with type, timestamp, summary
 *  6. WS dashboard/subscribe client receives dashboard/update push when a new order is placed
 */

import http from 'http';
import WebSocket from 'ws';

const PORT = 3458;
const BASE = `http://localhost:${PORT}`;
const WS_URL = `ws://localhost:${PORT}`;

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

function wsConnect(): Promise<WebSocket> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(WS_URL);
    const timer = setTimeout(() => { ws.terminate(); reject(new Error('WS connect timeout')); }, 5000);
    ws.on('open', () => { clearTimeout(timer); resolve(ws); });
    ws.on('error', (err) => { clearTimeout(timer); reject(err); });
  });
}

function wsSend(ws: WebSocket, msg: object): void {
  ws.send(JSON.stringify(msg));
}

function wsWaitForType(ws: WebSocket, type: string, timeoutMs = 5000): Promise<any> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      ws.removeListener('message', handler);
      reject(new Error(`Timeout waiting for '${type}'`));
    }, timeoutMs);
    function handler(data: WebSocket.RawData) {
      try {
        const msg = JSON.parse(data.toString());
        if (msg.type === type) {
          clearTimeout(timer);
          ws.removeListener('message', handler);
          resolve(msg);
        }
      } catch {}
    }
    ws.on('message', handler);
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
  console.log('Milestone 6: Dashboard and Integration');
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

  const openSockets: WebSocket[] = [];

  try {
    // Setup: register user, login, get products and session ready
    await request('/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'eve@test.com', password: 'pass123', name: 'Eve' }),
    });
    const loginRes = await request('/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'eve@test.com', password: 'pass123' }),
    });
    let token = '';
    try { token = JSON.parse(loginRes.body).token || ''; } catch {}

    const productsRes = await request('/products');
    let products: any[] = [];
    try { products = JSON.parse(productsRes.body); } catch {}
    const p1 = products.find((p: any) => p.inStock !== false && (p.stockCount === undefined || p.stockCount > 0)) || products[0];
    const sessionId = 'dashboard-session-001';

    // Pre-populate some activity: join a whiteboard, draw something
    const wbClient = await wsConnect();
    openSockets.push(wbClient);
    wsSend(wbClient, { type: 'hello', clientId: 'dash-wb-1' });
    await new Promise((r) => setTimeout(r, 200));
    wbClient.removeAllListeners('message');
    wsSend(wbClient, { type: 'whiteboard/join', boardId: 'dash-board' });
    await new Promise((r) => setTimeout(r, 200));
    wbClient.removeAllListeners('message');
    wsSend(wbClient, { type: 'whiteboard/draw', boardId: 'dash-board', operation: { tool: 'pen', x: 10, y: 20 } });
    await new Promise((r) => setTimeout(r, 200));

    // Place an order to generate activity
    await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': sessionId },
      body: JSON.stringify({ productId: p1.id, quantity: 1 }),
    });
    await request('/orders', {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}`, 'X-Session-Id': sessionId },
    });

    // Test 1: GET /dashboard returns all three sections
    const dashRes = await request('/dashboard');
    let dash: any = {};
    try { dash = JSON.parse(dashRes.body); } catch {}
    assert(
      'GET /dashboard returns all three sections (ecommerce, whiteboard, realtime)',
      dashRes.status === 200 && dash.ecommerce && dash.whiteboard && dash.realtime,
      `Status: ${dashRes.status}, sections: ${Object.keys(dash).join(', ')}`,
    );

    // Test 2: ecommerce section shape
    const ec = dash.ecommerce || {};
    assert(
      'ecommerce section has totalOrders, totalRevenue, and topProducts array',
      typeof ec.totalOrders === 'number' &&
        typeof ec.totalRevenue === 'number' &&
        Array.isArray(ec.topProducts),
      `ecommerce: ${JSON.stringify(ec)}`,
    );

    // Test 3: whiteboard section shape
    const wb = dash.whiteboard || {};
    assert(
      'whiteboard section has activeSessions and totalOperations',
      typeof wb.activeSessions === 'number' && typeof wb.totalOperations === 'number',
      `whiteboard: ${JSON.stringify(wb)}`,
    );

    // Test 4: realtime section shape
    const rt = dash.realtime || {};
    assert(
      'realtime section has connectedClients',
      typeof rt.connectedClients === 'number',
      `realtime: ${JSON.stringify(rt)}`,
    );

    // Test 5: GET /dashboard/activity returns recent events
    const activityRes = await request('/dashboard/activity');
    let activity: any[] = [];
    try { activity = JSON.parse(activityRes.body); } catch {}
    const hasActivity =
      activityRes.status === 200 &&
      Array.isArray(activity) &&
      activity.length >= 1 &&
      activity.every(
        (e: any) =>
          typeof e.type === 'string' &&
          (typeof e.timestamp === 'string' || typeof e.timestamp === 'number') &&
          typeof e.summary === 'string',
      );
    assert(
      'GET /dashboard/activity returns recent events with {type, timestamp, summary}',
      hasActivity,
      `Status: ${activityRes.status}, count: ${activity.length}, sample: ${JSON.stringify(activity[0])}`,
    );

    // Test 6: WS dashboard/subscribe receives update when new order placed
    const dashWs = await wsConnect();
    openSockets.push(dashWs);
    wsSend(dashWs, { type: 'hello', clientId: 'dash-subscriber' });
    await new Promise((r) => setTimeout(r, 200));
    dashWs.removeAllListeners('message');

    // Subscribe to dashboard updates
    wsSend(dashWs, { type: 'dashboard/subscribe' });
    await new Promise((r) => setTimeout(r, 200));

    // Place another order — this should trigger a dashboard/update push
    const sessionId2 = 'dashboard-session-002';
    await request('/cart/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Session-Id': sessionId2 },
      body: JSON.stringify({ productId: p1.id, quantity: 1 }),
    });
    await request('/orders', {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}`, 'X-Session-Id': sessionId2 },
    });

    let dashUpdate: any;
    try {
      dashUpdate = await wsWaitForType(dashWs, 'dashboard/update', 4000);
      assert(
        'WS dashboard/subscribe receives dashboard/update when a new order is placed',
        dashUpdate && dashUpdate.data,
        `Received: ${JSON.stringify(dashUpdate)}`,
      );
    } catch (err) {
      assert(
        'WS dashboard/subscribe receives dashboard/update when a new order is placed',
        false,
        String(err),
      );
    }
  } finally {
    for (const ws of openSockets) {
      try { ws.terminate(); } catch {}
    }
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
