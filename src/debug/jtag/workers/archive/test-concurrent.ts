/**
 * Archive Worker - Concurrent Processing Test
 *
 * Tests archiving from 3 collections simultaneously
 */

import * as net from 'net';
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

async function sendArchiveTask(taskId: string, collection: string, batchSize: number): Promise<ArchiveResponse> {
  const workerSocket = '/tmp/jtag-archive-worker.sock';
  const client = net.connect(workerSocket);

  const request: ArchiveRequest = {
    command: 'archive',
    task_id: taskId,
    collection,
    source_handle: 'primary',
    dest_handle: 'archive',
    batch_size: batchSize
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

  client.end();
  return response;
}

async function testConcurrent() {
  console.log('🧪 Archive Worker Concurrent Processing Test\n');

  const primaryDb = '.continuum/jtag/data/database.sqlite';
  const archiveDb = '.continuum/jtag/data/archive/database-001.sqlite';

  const collections = [
    { name: 'chat_messages', batchSize: 5 },
    { name: 'ai_generations', batchSize: 8 }
  ];

  // 1. Check initial counts
  console.log('1️⃣  Checking initial database state...');
  const primaryDbConn = new (require('better-sqlite3'))(primaryDb, { readonly: true });
  const archiveDbConn = new (require('better-sqlite3'))(archiveDb, { readonly: true });

  const initialCounts: Record<string, { primary: number; archive: number }> = {};

  for (const col of collections) {
    const primaryCount = primaryDbConn.prepare(`SELECT COUNT(*) as count FROM ${col.name}`).get() as { count: number };
    const archiveCount = archiveDbConn.prepare(`SELECT COUNT(*) as count FROM ${col.name}`).get() as { count: number };

    initialCounts[col.name] = {
      primary: primaryCount.count,
      archive: archiveCount.count
    };

    console.log(`   ${col.name}:`);
    console.log(`     Primary: ${primaryCount.count} rows`);
    console.log(`     Archive: ${archiveCount.count} rows`);
  }

  primaryDbConn.close();
  archiveDbConn.close();

  // 2. Send 2 archive tasks concurrently
  console.log('\n2️⃣  Sending 2 archive tasks concurrently...');

  const tasks = await Promise.all(
    collections.map((col, i) =>
      sendArchiveTask(`concurrent-test-${i + 1}`, col.name, col.batchSize)
    )
  );

  tasks.forEach((response, i) => {
    console.log(`   Task ${i + 1} (${collections[i].name}): queued at position ${response.queue_position}`);
  });

  console.log('   ✅ All tasks queued');

  // 3. Wait for processing
  console.log('\n3️⃣  Waiting for concurrent processing to complete...');
  await new Promise(resolve => setTimeout(resolve, 5000));

  // 4. Check final counts
  console.log('\n4️⃣  Checking final database state...');
  const primaryDbConn2 = new (require('better-sqlite3'))(primaryDb, { readonly: true });
  const archiveDbConn2 = new (require('better-sqlite3'))(archiveDb, { readonly: true });

  let allSuccess = true;

  for (const col of collections) {
    const primaryCountAfter = primaryDbConn2.prepare(`SELECT COUNT(*) as count FROM ${col.name}`).get() as { count: number };
    const archiveCountAfter = archiveDbConn2.prepare(`SELECT COUNT(*) as count FROM ${col.name}`).get() as { count: number };

    const archivedCount = initialCounts[col.name].primary - primaryCountAfter.count;
    const addedToArchive = archiveCountAfter.count - initialCounts[col.name].archive;

    console.log(`   ${col.name}:`);
    console.log(`     Primary: ${primaryCountAfter.count} (was ${initialCounts[col.name].primary}, archived ${archivedCount})`);
    console.log(`     Archive: ${archiveCountAfter.count} (was ${initialCounts[col.name].archive}, added ${addedToArchive})`);

    if (archivedCount === addedToArchive && archivedCount === col.batchSize) {
      console.log(`     ✅ Correct (${col.batchSize} rows)`);
    } else {
      console.log(`     ❌ Mismatch (expected ${col.batchSize}, got ${archivedCount})`);
      allSuccess = false;
    }
  }

  primaryDbConn2.close();
  archiveDbConn2.close();

  if (allSuccess) {
    console.log('\n✅ TEST PASSED - Concurrent processing works!\n');
  } else {
    console.log('\n❌ TEST FAILED\n');
    process.exit(1);
  }
}

testConcurrent().catch(error => {
  console.error('❌ Test error:', error);
  process.exit(1);
});
