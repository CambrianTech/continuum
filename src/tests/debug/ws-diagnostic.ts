/**
 * Raw WebSocket diagnostic — bypasses ALL JTAGClient machinery
 * Tests if the server actually responds to a session/create request
 *
 * Run: npx tsx tests/debug/ws-diagnostic.ts
 */

import WebSocket from 'ws';

const WS_PORT = 9001;
const URL = `ws://localhost:${WS_PORT}`;

console.log(`\n=== WebSocket Diagnostic ===`);
console.log(`Connecting to ${URL}...\n`);

const ws = new WebSocket(URL);
let messageCount = 0;

ws.on('open', () => {
  console.log(`✅ Connected to ${URL}`);

  // Send a raw session/create request — same format RemoteConnection uses
  const correlationId = `client_${Date.now()}_diagnostic`;
  const requestMessage = {
    messageType: 'request',
    context: { environment: 'server', uuid: 'diagnostic-test' },
    origin: 'server',
    endpoint: 'server/commands/session/create',
    correlationId,
    payload: {
      context: { environment: 'server', uuid: 'diagnostic-test' },
      sessionId: '00000000-0000-0000-0000-000000000000',
      category: 'user',
      displayName: 'Diagnostic Test',
      userId: undefined,
      isShared: true,
      connectionContext: {
        clientType: 'cli',
        identity: { uniqueId: '@cli-diagnostic' }
      }
    }
  };

  console.log(`📤 Sending session/create with correlationId: ${correlationId}`);
  console.log(`   endpoint: ${requestMessage.endpoint}`);
  ws.send(JSON.stringify(requestMessage));

  // Also try a simple ping command (with a REAL sessionId from a working session)
  setTimeout(() => {
    const pingCorrelationId = `client_${Date.now()}_ping`;
    const pingMessage = {
      messageType: 'request',
      context: { environment: 'server', uuid: 'diagnostic-test' },
      origin: 'server',
      endpoint: 'server/commands/ping',
      correlationId: pingCorrelationId,
      payload: {
        context: { environment: 'server', uuid: 'diagnostic-test' },
        sessionId: '00000000-0000-0000-0000-000000000000'
      }
    };
    console.log(`\n📤 Sending ping with correlationId: ${pingCorrelationId}`);
    ws.send(JSON.stringify(pingMessage));
  }, 1000);

  // Try a list command (should also work since it goes through CommandDaemon)
  setTimeout(() => {
    const listCorrelationId = `client_${Date.now()}_list`;
    const listMessage = {
      messageType: 'request',
      context: { environment: 'server', uuid: 'diagnostic-test' },
      origin: 'server',
      endpoint: 'server/commands/list',
      correlationId: listCorrelationId,
      payload: {
        context: { environment: 'server', uuid: 'diagnostic-test' },
        sessionId: '00000000-0000-0000-0000-000000000000'
      }
    };
    console.log(`\n📤 Sending list with correlationId: ${listCorrelationId}`);
    ws.send(JSON.stringify(listMessage));
  }, 2000);

  // Timeout - if no response in 10s, something is wrong
  setTimeout(() => {
    if (messageCount === 0) {
      console.log(`\n❌ TIMEOUT: No messages received in 10 seconds`);
      console.log(`   The server is NOT sending responses back on the WebSocket`);
    } else {
      console.log(`\n✅ Received ${messageCount} message(s) total`);
    }
    ws.close();
    process.exit(messageCount === 0 ? 1 : 0);
  }, 10000);
});

ws.on('message', (data) => {
  messageCount++;
  const raw = data.toString();
  try {
    const msg = JSON.parse(raw);
    console.log(`\n📥 Received message #${messageCount}:`);
    console.log(`   messageType: ${msg.messageType}`);
    console.log(`   correlationId: ${msg.correlationId}`);
    console.log(`   endpoint: ${msg.endpoint}`);
    console.log(`   origin: ${msg.origin}`);
    if (msg.payload) {
      console.log(`   payload.success: ${msg.payload?.success}`);
      console.log(`   payload.error: ${msg.payload?.error}`);
      if (msg.payload?.session) {
        console.log(`   session.sessionId: ${msg.payload.session.sessionId}`);
        console.log(`   session.userId: ${msg.payload.session.userId}`);
      }
      if (msg.payload?.commandResult) {
        console.log(`   commandResult.success: ${msg.payload.commandResult.success}`);
      }
    }
  } catch {
    console.log(`📥 Raw message #${messageCount}: ${raw.substring(0, 200)}...`);
  }
});

ws.on('error', (err) => {
  console.log(`❌ WebSocket error: ${err.message}`);
  if (err.message.includes('ECONNREFUSED')) {
    console.log(`   Server is not running on port ${WS_PORT}`);
    console.log(`   Run: cd src/debug/jtag && npm start`);
  }
  process.exit(1);
});

ws.on('close', (code, reason) => {
  console.log(`\n🔌 WebSocket closed (code: ${code}, reason: ${reason.toString() || 'none'})`);
});
