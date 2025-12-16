/**
 * Archive Worker - Real Data Test
 *
 * Tests archiving actual rows from chat_messages
 */

import * as net from 'net';
import * as fs from 'fs';
import { Database } from 'better-sqlite3';

interface ArchiveRequest {
  command: 'archive';
  task_id: string;
  collection: string;
  source_handle: string;
  dest_handle: string;
  batch_size: number;
}

interface ArchiveResponse {
  status: 'queued';
  task_id: string;
  queue_position: number;
}

async function testArchive() {
  console.log('🧪 Archive Worker Real Data Test\n');

  const primaryDb = '.continuum/jtag/data/database.sqlite';
  const archiveDb = '.continuum/jtag/data/archive/database-001.sqlite';
  const workerSocket = '/tmp/jtag-archive-worker.sock';

  // 1. Check initial counts
  console.log('1️⃣  Checking initial database state...');
  const primaryDbConn = new (require('better-sqlite3'))(primaryDb, { readonly: true });
  const archiveDbConn = new (require('better-sqlite3'))(archiveDb, { readonly: true });

  const primaryCountBefore = primaryDbConn.prepare('SELECT COUNT(*) as count FROM chat_messages').get() as { count: number };
  const archiveCountBefore = archiveDbConn.prepare('SELECT COUNT(*) as count FROM chat_messages').get() as { count: number };

  console.log(`   Primary DB: ${primaryCountBefore.count} rows`);
  console.log(`   Archive DB: ${archiveCountBefore.count} rows`);

  primaryDbConn.close();
  archiveDbConn.close();

  // 2. Send archive task
  console.log('\n2️⃣  Sending archive task to Rust worker...');

  const client = net.connect(workerSocket);

  const request: ArchiveRequest = {
    command: 'archive',
    task_id: 'test-real-data-001',
    collection: 'chat_messages',
    source_handle: 'primary',
    dest_handle: 'archive',
    batch_size: 10  // Archive 10 rows
  };

  client.write(JSON.stringify(request) + '\n');

  // Wait for response
  const response = await new Promise<ArchiveResponse>((resolve, reject) => {
    client.on('data', (data) => {
      const response = JSON.parse(data.toString());
      resolve(response);
    });

    client.on('error', reject);
    setTimeout(() => reject(new Error('Timeout waiting for response')), 5000);
  });

  console.log(`   Response:`, response);
  console.log(`   ✅ Task queued at position ${response.queue_position}`);

  client.end();

  // 3. Wait for archiving to complete
  console.log('\n3️⃣  Waiting for archiving to complete...');
  await new Promise(resolve => setTimeout(resolve, 3000));

  // 4. Check final counts
  console.log('\n4️⃣  Checking final database state...');
  const primaryDbConn2 = new (require('better-sqlite3'))(primaryDb, { readonly: true });
  const archiveDbConn2 = new (require('better-sqlite3'))(archiveDb, { readonly: true });

  const primaryCountAfter = primaryDbConn2.prepare('SELECT COUNT(*) as count FROM chat_messages').get() as { count: number };
  const archiveCountAfter = archiveDbConn2.prepare('SELECT COUNT(*) as count FROM chat_messages').get() as { count: number };

  console.log(`   Primary DB: ${primaryCountAfter.count} rows (was ${primaryCountBefore.count})`);
  console.log(`   Archive DB: ${archiveCountAfter.count} rows (was ${archiveCountBefore.count})`);

  const archivedCount = primaryCountBefore.count - primaryCountAfter.count;
  const addedToArchive = archiveCountAfter.count - archiveCountBefore.count;

  if (archivedCount === addedToArchive && archivedCount === 10) {
    console.log(`   ✅ Successfully archived ${archivedCount} rows`);
    console.log('\n✅ TEST PASSED - Real data archiving works!\n');
  } else {
    console.log(`   ❌ Mismatch: archived ${archivedCount} from primary, added ${addedToArchive} to archive`);
    console.log('\n❌ TEST FAILED\n');
    process.exit(1);
  }

  primaryDbConn2.close();
  archiveDbConn2.close();
}

testArchive().catch(error => {
  console.error('❌ Test error:', error);
  process.exit(1);
});
