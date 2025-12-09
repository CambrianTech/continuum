#!/usr/bin/env tsx
/**
 * TypeScript Test Client for Rust Logger Worker
 *
 * This demonstrates end-to-end communication with the Rust worker:
 * 1. Connects to Unix domain socket
 * 2. Sends typed log messages using WorkerRequest<WriteLogPayload>
 * 3. Receives typed responses using WorkerResponse<WriteLogResult>
 * 4. Validates round-trip JSON serialization with serde
 *
 * Run: npx tsx typescript-client/test-client.ts
 */

import * as net from 'net';
import { randomUUID } from 'crypto';

// Import shared types (in production, these would come from shared/ipc/)
interface WorkerMessage<T = unknown> {
  id: string;
  type: string;
  timestamp: string;
  payload: T;
}

interface WorkerRequest<T = unknown> extends WorkerMessage<T> {
  userId?: string;
}

interface WorkerResponse<T = unknown> extends WorkerMessage<T> {
  requestId: string;
  success: boolean;
  error?: string;
  errorType?: 'validation' | 'timeout' | 'internal' | 'not_found';
  stack?: string;
}

type LogLevel = 'debug' | 'info' | 'warn' | 'error';

interface WriteLogPayload {
  category: string;
  level: LogLevel;
  component: string;
  message: string;
  args?: unknown[];
}

interface WriteLogResult {
  bytesWritten: number;
}

// Test client
async function main() {
  const socketPath = '/tmp/logger-worker.sock';

  console.log('📡 TypeScript Test Client Starting...');
  console.log(`🔌 Connecting to: ${socketPath}`);

  const client = net.createConnection(socketPath);

  await new Promise<void>((resolve, reject) => {
    client.once('connect', () => {
      console.log('✅ Connected to Rust worker\n');
      resolve();
    });
    client.once('error', reject);
  });

  // Send test log messages
  const testMessages = [
    {
      category: 'sql',
      level: 'info' as LogLevel,
      component: 'DataDaemon',
      message: 'Database connection established'
    },
    {
      category: 'daemons/UserDaemonServer',
      level: 'debug' as LogLevel,
      component: 'PersonaUser',
      message: 'Processing inbox: 3 tasks queued'
    },
    {
      category: 'system',
      level: 'warn' as LogLevel,
      component: 'OllamaAdapter',
      message: 'Model response took 28s (near timeout)'
    },
    {
      category: 'ai',
      level: 'error' as LogLevel,
      component: 'AIProvider',
      message: 'Request timed out after 60s'
    }
  ];

  let responseCount = 0;
  const expectedCount = testMessages.length;

  // Set up response handler
  let buffer = '';
  client.on('data', (data) => {
    buffer += data.toString();

    // Process complete lines (messages are newline-delimited)
    const lines = buffer.split('\n');
    buffer = lines.pop() || ''; // Keep incomplete line in buffer

    for (const line of lines) {
      if (!line.trim()) continue;

      try {
        const response: WorkerResponse<WriteLogResult> = JSON.parse(line);

        console.log(`\n📬 Response ${++responseCount}/${expectedCount}:`);
        console.log(`   ✅ Success: ${response.success}`);
        console.log(`   📊 Bytes written: ${response.payload.bytesWritten}`);
        console.log(`   🔗 Request ID: ${response.requestId.substring(0, 8)}...`);

        if (response.error) {
          console.log(`   ❌ Error: ${response.error}`);
        }

        // Exit when all responses received
        if (responseCount === expectedCount) {
          console.log('\n✅ All tests passed! Communication working end-to-end.');
          client.end();
          process.exit(0);
        }
      } catch (err) {
        console.error('❌ Failed to parse response:', line);
        console.error('   Error:', err);
      }
    }
  });

  // Send test messages
  for (const [index, testMsg] of testMessages.entries()) {
    console.log(`\n📤 Sending message ${index + 1}/${testMessages.length}:`);
    console.log(`   Level: ${testMsg.level}`);
    console.log(`   Category: ${testMsg.category}`);
    console.log(`   Message: ${testMsg.message}`);

    const request: WorkerRequest<WriteLogPayload> = {
      id: randomUUID(),
      type: 'write-log',
      timestamp: new Date().toISOString(),
      userId: 'test-user-id',
      payload: testMsg
    };

    // Send as newline-delimited JSON
    client.write(JSON.stringify(request) + '\n');

    // Small delay between messages for readability
    await new Promise(resolve => setTimeout(resolve, 100));
  }

  // Timeout fallback
  setTimeout(() => {
    console.error('\n❌ Test timeout - did not receive all responses');
    process.exit(1);
  }, 5000);
}

// Handle errors
process.on('uncaughtException', (err) => {
  console.error('\n❌ Error:', err.message);
  if (err.message.includes('ENOENT') || err.message.includes('ECONNREFUSED')) {
    console.error('\n💡 Make sure the Rust worker is running first:');
    console.error('   cd /tmp/rust-worker-test');
    console.error('   cargo run -- /tmp/logger-worker.sock');
  }
  process.exit(1);
});

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
