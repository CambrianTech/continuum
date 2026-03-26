#!/usr/bin/env tsx
/**
 * Milestone 4: Collaborative Whiteboard
 *
 * Tests:
 *  1. Client receives whiteboard/joined ack after sending whiteboard/join
 *  2. Draw operation is broadcast to other board members (not sender)
 *  3. Client outside the board does not receive draw events
 *  4. New joiner receives whiteboard/sync with all prior operations
 *  5. whiteboard/clear resets history and broadcasts to all members
 *  6. GET /whiteboard/:boardId/history returns operations array
 *  7. Two different boardIds operate independently
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
      try { resolve(JSON.parse(data.toString())); } catch { resolve(data.toString()); }
    });
  });
}

function wsNoMessage(ws: WebSocket, waitMs = 600): Promise<boolean> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => { ws.removeAllListeners('message'); resolve(true); }, waitMs);
    ws.once('message', () => { clearTimeout(timer); resolve(false); });
  });
}

function wsSend(ws: WebSocket, msg: object): void {
  ws.send(JSON.stringify(msg));
}

// Filter messages to find a specific type, with timeout
function wsWaitForType(ws: WebSocket, type: string, timeoutMs = 3000): Promise<any> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      ws.removeListener('message', handler);
      reject(new Error(`Timeout waiting for message type '${type}'`));
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

const TOTAL_TESTS = 7;

async function main() {
  console.log('Milestone 4: Collaborative Whiteboard');
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
  const BOARD_A = 'board-alpha';
  const BOARD_B = 'board-beta';

  try {
    // Connect three clients
    const ws1 = await wsConnect(); // will join board A
    const ws2 = await wsConnect(); // will join board A
    const ws3 = await wsConnect(); // will NOT join any board (outsider)
    const ws4 = await wsConnect(); // will join board B
    openSockets.push(ws1, ws2, ws3, ws4);

    // Do hello handshake for all (server may require it)
    wsSend(ws1, { type: 'hello', clientId: 'wb-c1' });
    wsSend(ws2, { type: 'hello', clientId: 'wb-c2' });
    wsSend(ws3, { type: 'hello', clientId: 'wb-c3' });
    wsSend(ws4, { type: 'hello', clientId: 'wb-c4' });
    await new Promise((r) => setTimeout(r, 300));
    // Drain welcome messages (some may have already arrived)
    for (const ws of [ws1, ws2, ws3, ws4]) {
      ws.removeAllListeners('message');
    }

    // Test 1: Join board receives ack
    wsSend(ws1, { type: 'whiteboard/join', boardId: BOARD_A });
    let joinedMsg: any;
    try {
      joinedMsg = await wsWaitForType(ws1, 'whiteboard/joined', 3000);
      assert(
        'Client receives whiteboard/joined ack after joining a board',
        joinedMsg && joinedMsg.boardId === BOARD_A,
        `Received: ${JSON.stringify(joinedMsg)}`,
      );
    } catch (err) {
      assert('Client receives whiteboard/joined ack after joining a board', false, String(err));
      joinedMsg = null;
    }

    // ws2 also joins board A
    wsSend(ws2, { type: 'whiteboard/join', boardId: BOARD_A });
    await new Promise((r) => setTimeout(r, 300));
    // drain sync message ws2 may receive (board is still empty)
    ws2.removeAllListeners('message');

    // Test 2: Draw broadcast to other board members (not sender)
    const drawOp = { tool: 'pen', x: 100, y: 200, color: '#ff0000', size: 2 };
    wsSend(ws1, { type: 'whiteboard/draw', boardId: BOARD_A, operation: drawOp });

    // ws2 should receive the draw event
    let drawReceived: any;
    try {
      drawReceived = await wsWaitForType(ws2, 'whiteboard/draw', 3000);
      assert(
        'Draw operation broadcast to other board members',
        drawReceived &&
          drawReceived.boardId === BOARD_A &&
          drawReceived.operation &&
          drawReceived.operation.tool === 'pen',
        `Received: ${JSON.stringify(drawReceived)}`,
      );
    } catch (err) {
      assert('Draw operation broadcast to other board members', false, String(err));
    }

    // Test 3: Outsider (ws3) does NOT receive draw events from board A
    wsSend(ws1, { type: 'whiteboard/draw', boardId: BOARD_A, operation: { tool: 'rect', x: 50, y: 50 } });
    const noEvent = await wsNoMessage(ws3, 700);
    assert(
      'Client outside the board does not receive draw events',
      noEvent,
      'ws3 received a message it should not have',
    );

    // Test 4: New joiner receives whiteboard/sync with prior operations
    const ws5 = await wsConnect();
    openSockets.push(ws5);
    wsSend(ws5, { type: 'hello', clientId: 'wb-c5' });
    await new Promise((r) => setTimeout(r, 200));
    ws5.removeAllListeners('message');

    wsSend(ws5, { type: 'whiteboard/join', boardId: BOARD_A });
    let syncMsg: any;
    try {
      // May receive whiteboard/joined first, then whiteboard/sync — or combined
      const msgs: any[] = [];
      for (let i = 0; i < 3; i++) {
        try {
          const m = await wsNextMessage(ws5, 2000);
          msgs.push(m);
          if (m.type === 'whiteboard/sync') break;
        } catch { break; }
      }
      syncMsg = msgs.find((m) => m.type === 'whiteboard/sync');
      assert(
        'New joiner receives whiteboard/sync with all prior operations',
        syncMsg &&
          syncMsg.boardId === BOARD_A &&
          Array.isArray(syncMsg.operations) &&
          syncMsg.operations.length >= 1,
        `Received: ${JSON.stringify(syncMsg)}`,
      );
    } catch (err) {
      assert('New joiner receives whiteboard/sync with all prior operations', false, String(err));
    }

    // Test 5: whiteboard/clear broadcasts to board members and resets history
    wsSend(ws1, { type: 'whiteboard/clear', boardId: BOARD_A });
    let clearMsg: any;
    try {
      clearMsg = await wsWaitForType(ws2, 'whiteboard/clear', 3000);
      assert(
        'whiteboard/clear broadcast to all board members',
        clearMsg && clearMsg.boardId === BOARD_A,
        `Received: ${JSON.stringify(clearMsg)}`,
      );
    } catch (err) {
      assert('whiteboard/clear broadcast to all board members', false, String(err));
    }

    // Test 6: GET /whiteboard/:boardId/history returns array (should be empty after clear)
    const histRes = await request(`/whiteboard/${BOARD_A}/history`);
    let histBody: any;
    try {
      histBody = JSON.parse(histRes.body);
    } catch {}
    assert(
      'GET /whiteboard/:boardId/history returns operations array',
      histRes.status === 200 && Array.isArray(histBody),
      `Status: ${histRes.status}, body: ${histRes.body.slice(0, 100)}`,
    );

    // Test 7: Board B is independent from board A
    wsSend(ws4, { type: 'whiteboard/join', boardId: BOARD_B });
    await new Promise((r) => setTimeout(r, 300));
    ws4.removeAllListeners('message');

    // Draw on board A — ws4 (board B) should NOT receive it
    wsSend(ws1, { type: 'whiteboard/draw', boardId: BOARD_A, operation: { tool: 'line', x: 10, y: 10 } });
    const boardIndependent = await wsNoMessage(ws4, 700);
    assert(
      'Two different boardIds operate independently (no cross-contamination)',
      boardIndependent,
      'ws4 (board B) received an event from board A',
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
