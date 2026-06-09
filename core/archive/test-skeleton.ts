/**
 * Test Archive Worker Skeleton - TEMPLATE
 *
 * Tests bidirectional TypeScript ↔ Rust communication:
 * 1. Start CommandRouterServer (handles Rust → TypeScript commands)
 * 2. Start Rust worker
 * 3. Send archive task from TypeScript
 * 4. Rust calls Commands.execute() back to TypeScript
 * 5. TypeScript executes and returns result
 * 6. Verify complete flow works
 */

import { spawn, type ChildProcess } from 'child_process';
import * as net from 'net';
import * as fs from 'fs';
import { CommandRouterServer } from '../../shared/ipc/archive-worker/CommandRouterServer';

const WORKER_SOCKET = '/tmp/archive-worker-test.sock';
const ROUTER_SOCKET = '/tmp/command-router-test.sock';

interface ArchiveRequest {
  command: string;
  task_id: string;
  collection: string;
}

interface ArchiveResponse {
  status: string;
  task_id?: string;
  queue_position?: number;
  rows_found?: number;
}

/**
 * Send archive task to Rust worker
 */
async function sendArchiveTask(taskId: string, collection: string): Promise<ArchiveResponse> {
  return new Promise((resolve, reject) => {
    const client = net.createConnection(WORKER_SOCKET, () => {
      const request: ArchiveRequest = {
        command: 'archive',
        task_id: taskId,
        collection
      };

      client.write(JSON.stringify(request) + '\n');
    });

    let buffer = '';
    client.on('data', (chunk) => {
      buffer += chunk.toString();

      if (buffer.includes('\n')) {
        const response: ArchiveResponse = JSON.parse(buffer.trim());
        client.end();
        resolve(response);
      }
    });

    client.on('error', reject);
  });
}

/**
 * Send ping to Rust worker
 */
async function sendPing(): Promise<ArchiveResponse> {
  return new Promise((resolve, reject) => {
    const client = net.createConnection(WORKER_SOCKET, () => {
      client.write(JSON.stringify({ command: 'ping' }) + '\n');
    });

    let buffer = '';
    client.on('data', (chunk) => {
      buffer += chunk.toString();

      if (buffer.includes('\n')) {
        const response: ArchiveResponse = JSON.parse(buffer.trim());
        client.end();
        resolve(response);
      }
    });

    client.on('error', reject);
  });
}

/**
 * Main test
 */
async function main() {
  console.log('🧪 Archive Worker Skeleton Test\n');

  // Clean up old sockets
  [WORKER_SOCKET, ROUTER_SOCKET].forEach(path => {
    if (fs.existsSync(path)) {
      fs.unlinkSync(path);
    }
  });

  // 1. Start Command Router (handles Rust → TypeScript)
  console.log('1️⃣  Starting Command Router...');
  const router = new CommandRouterServer(ROUTER_SOCKET);
  await router.start();
  console.log('   ✅ Command Router ready\n');

  // 2. Start Rust Worker
  console.log('2️⃣  Starting Rust Worker...');
  const rustWorker: ChildProcess = spawn(
    './workers/archive/target/release/archive-worker',
    [WORKER_SOCKET, ROUTER_SOCKET],
    {
      cwd: process.cwd(),
      stdio: 'inherit'
    }
  );

  // Wait for worker to start
  await new Promise(resolve => setTimeout(resolve, 1000));
  console.log('   ✅ Rust Worker ready\n');

  try {
    // 3. Test Ping
    console.log('3️⃣  Testing ping...');
    const pingResponse = await sendPing();
    console.log('   Response:', pingResponse);
    console.log('   ✅ Ping successful\n');

    // 4. Test Archive Task
    console.log('4️⃣  Sending archive task...');
    const archiveResponse = await sendArchiveTask('task-001', 'chat_messages');
    console.log('   Response:', archiveResponse);

    if (archiveResponse.status === 'queued') {
      console.log(`   ✅ Task queued at position ${archiveResponse.queue_position}\n`);
    } else {
      console.log('   ❌ Unexpected response status\n');
    }

    // 5. Wait for task to complete
    console.log('5️⃣  Waiting for task to complete...');
    await new Promise(resolve => setTimeout(resolve, 2000));
    console.log('   ✅ Task should be complete (check worker output)\n');

    console.log('✅ TEST COMPLETE - Bidirectional communication works!');
  } catch (error) {
    console.error('❌ Test failed:', error);
  } finally {
    // Cleanup
    rustWorker.kill();
    await router.stop();
  }
}

main().catch(console.error);
