#!/usr/bin/env tsx
/**
 * Milestone 3: WebSocket Real-time Layer
 *
 * Tests:
 *  1. WebSocket connection succeeds at ws://localhost:3458
 *  2. Server sends {type:'welcome', clientId} after hello handshake
 *  3. Welcome message includes the correct clientId
 *  4. Multiple clients can connect simultaneously
 *  5. GET /ws/stats returns {connectedClients: N}
 *  6. Disconnected client is removed and count decrements
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

function wsNextMessage(ws: WebSocket, timeoutMs = 3000): Promise<any> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('WS message timeout')), timeoutMs);
    ws.once('message', (data) => {
      clearTimeout(timer);
      try {
        resolve(JSON.parse(data.toString()));
      } catch {
        resolve(data.toString());
      }
    });
  });
}

function wsSend(ws: WebSocket, msg: object): void {
  ws.send(JSON.stringify(msg));
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
  console.log('Milestone 3: WebSocket Real-time Layer');
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
    // Test 1: WebSocket connection succeeds
    let ws1: WebSocket;
    try {
      ws1 = await wsConnect();
      openSockets.push(ws1);
      assert('WebSocket connection succeeds at ws://localhost:3458', true);
    } catch (err) {
      assert('WebSocket connection succeeds at ws://localhost:3458', false, String(err));
      console.log(`\nResults: ${passed} passed, ${TOTAL_TESTS - passed} failed`);
      httpServer?.close();
      process.exit(1);
    }

    // Test 2: Server sends welcome after hello
    const clientId1 = 'client-alpha';
    wsSend(ws1!, { type: 'hello', clientId: clientId1 });
    let welcome: any;
    try {
      welcome = await wsNextMessage(ws1!);
      assert(
        "Server responds with {type:'welcome'} after hello",
        welcome && welcome.type === 'welcome',
        `Received: ${JSON.stringify(welcome)}`,
      );
    } catch (err) {
      assert("Server responds with {type:'welcome'} after hello", false, String(err));
      welcome = null;
    }

    // Test 3: Welcome includes correct clientId
    assert(
      'Welcome message includes the correct clientId',
      welcome && welcome.clientId === clientId1,
      `Expected clientId '${clientId1}', got: ${welcome?.clientId}`,
    );

    // Test 4: Multiple clients can connect
    let ws2: WebSocket;
    let ws3: WebSocket;
    try {
      ws2 = await wsConnect();
      ws3 = await wsConnect();
      openSockets.push(ws2, ws3);
      wsSend(ws2, { type: 'hello', clientId: 'client-beta' });
      wsSend(ws3, { type: 'hello', clientId: 'client-gamma' });
      await wsNextMessage(ws2);
      await wsNextMessage(ws3);
      assert('Multiple clients can connect simultaneously', true);
    } catch (err) {
      assert('Multiple clients can connect simultaneously', false, String(err));
    }

    // Test 5: GET /ws/stats returns connectedClients
    await new Promise((r) => setTimeout(r, 200));
    const statsRes = await request('/ws/stats');
    let statsBody: any = {};
    try {
      statsBody = JSON.parse(statsRes.body);
    } catch {}
    assert(
      'GET /ws/stats returns {connectedClients: N}',
      statsRes.status === 200 && typeof statsBody.connectedClients === 'number' && statsBody.connectedClients >= 1,
      `Status: ${statsRes.status}, body: ${statsRes.body}`,
    );

    const countBefore = statsBody.connectedClients as number;

    // Test 6: Disconnected client decrements count
    ws1!.close();
    await new Promise((r) => setTimeout(r, 400));
    const statsAfterRes = await request('/ws/stats');
    let statsAfter: any = {};
    try {
      statsAfter = JSON.parse(statsAfterRes.body);
    } catch {}
    assert(
      'Disconnected client is removed and connectedClients decrements',
      statsAfterRes.status === 200 &&
        typeof statsAfter.connectedClients === 'number' &&
        statsAfter.connectedClients < countBefore,
      `Before: ${countBefore}, After: ${statsAfter.connectedClients}`,
    );
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
