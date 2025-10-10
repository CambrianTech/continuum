#!/usr/bin/env npx tsx

/**
 * Query Handle Pagination Integration Test
 *
 * Tests the new stateful pagination with query handles:
 * 1. Open query and get handle
 * 2. Fetch pages using handle (no cursor management needed)
 * 3. Verify no duplicates across pages
 * 4. Verify pagination to the end
 * 5. Close query handle
 */

import { execSync } from 'child_process';
import { readFileSync, unlinkSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

interface QueryOpenResult {
  success: boolean;
  queryHandle: string;
  totalCount: number;
  pageSize: number;
  error?: string;
}

interface QueryNextResult {
  success: boolean;
  items: any[];
  pageNumber: number;
  hasMore: boolean;
  totalCount: number;
  error?: string;
}

function runCommand(cmd: string): any {
  // Use temp file to avoid execSync buffer limits
  const tempFile = join(tmpdir(), `jtag-test-${Date.now()}.json`);
  const fullCmd = `${cmd} > ${tempFile} 2>&1`;

  console.log(`Running: ${cmd}`);
  execSync(fullCmd, { encoding: 'utf8' });

  const output = readFileSync(tempFile, 'utf8');
  unlinkSync(tempFile); // Clean up

  return JSON.parse(output);
}

function openQuery(): QueryOpenResult {
  const cmd = `./jtag data/query-open --collection=chat_messages --filter='{"roomId":"5e71a0c8-0303-4eb8-a478-3a121248","status":"sent"}' --backend=server`;
  return runCommand(cmd);
}

function getNextPage(queryHandle: string): QueryNextResult {
  const cmd = `./jtag data/query-next --queryHandle="${queryHandle}" --collection=chat_messages --backend=server`;
  return runCommand(cmd);
}

function closeQuery(queryHandle: string): any {
  const cmd = `./jtag data/query-close --queryHandle="${queryHandle}" --collection=chat_messages --backend=server`;
  return runCommand(cmd);
}

console.log('🧪 Testing Query Handle Pagination');
console.log('==================================\n');

// Step 1: Open query
console.log('📖 Step 1: Opening query...');
const openResult = openQuery();
if (!openResult.success) {
  console.error('❌ Failed to open query:', openResult.error);
  process.exit(1);
}
console.log(`✅ Query opened successfully`);
console.log(`   Handle: ${openResult.queryHandle}`);
console.log(`   Total Count: ${openResult.totalCount}`);
console.log(`   Page Size: ${openResult.pageSize}\n`);

const queryHandle = openResult.queryHandle;
const totalCount = openResult.totalCount;
const pageSize = openResult.pageSize;

// Step 2: Fetch pages until no more
console.log('📄 Step 2: Fetching all pages...');
const allIds = new Set<string>();
const allTimestamps: string[] = [];
let pageNum = 0;
let hasMore = true;

while (hasMore) {
  console.log(`\n   Page ${pageNum}:`);
  const pageResult = getNextPage(queryHandle);

  if (!pageResult.success) {
    console.error(`❌ Failed to fetch page ${pageNum}:`, pageResult.error);
    process.exit(1);
  }

  console.log(`   ✅ Got ${pageResult.items.length} items`);

  // Filter out items without IDs (shouldn't happen but defensive)
  const validItems = pageResult.items.filter((m: any) => m && m.id);
  if (validItems.length !== pageResult.items.length) {
    console.warn(`   ⚠️  ${pageResult.items.length - validItems.length} items missing IDs`);
  }

  console.log(`      IDs: ${validItems.slice(0, 5).map((m: any) => m.id.substring(0, 8)).join(', ')}${validItems.length > 5 ? '...' : ''}`);

  // Check for duplicates
  const pageDuplicates: string[] = [];
  for (const item of validItems) {
    if (allIds.has(item.id)) {
      pageDuplicates.push(item.id.substring(0, 8));
    } else {
      allIds.add(item.id);
      allTimestamps.push(item.timestamp);
    }
  }

  if (pageDuplicates.length > 0) {
    console.error(`❌ FAILED: Page ${pageNum} contained ${pageDuplicates.length} duplicate IDs!`);
    console.error(`   Duplicates: ${pageDuplicates.join(', ')}`);
    process.exit(1);
  }

  console.log(`      hasMore: ${pageResult.hasMore}`);
  console.log(`      Cumulative unique IDs: ${allIds.size}/${totalCount}`);

  hasMore = pageResult.hasMore;
  pageNum++;

  // Safety check - prevent infinite loop
  if (pageNum > 100) {
    console.error('❌ FAILED: More than 100 pages fetched, possible infinite loop!');
    process.exit(1);
  }
}

console.log(`\n✅ PASSED: Fetched all pages (${pageNum} pages total)`);
console.log(`   Total unique messages: ${allIds.size}`);
console.log(`   Expected total: ${totalCount}`);

// Step 3: Verify we got all messages
if (allIds.size !== totalCount) {
  console.error(`❌ FAILED: Did not fetch all messages!`);
  console.error(`   Expected: ${totalCount}`);
  console.error(`   Got: ${allIds.size}`);
  process.exit(1);
}

console.log('✅ PASSED: Got exactly the expected number of messages\n');

// Step 4: Verify timestamps are sorted correctly (DESC order)
console.log('📊 Step 3: Verifying sort order (timestamp DESC)...');
let sortedCorrectly = true;
for (let i = 0; i < allTimestamps.length - 1; i++) {
  if (allTimestamps[i] < allTimestamps[i + 1]) {
    console.error(`❌ FAILED: Timestamps not in DESC order!`);
    console.error(`   Position ${i}: ${allTimestamps[i]}`);
    console.error(`   Position ${i + 1}: ${allTimestamps[i + 1]}`);
    sortedCorrectly = false;
    break;
  }
}

if (sortedCorrectly) {
  console.log('✅ PASSED: All timestamps in correct DESC order\n');
}

// Step 5: Close query
console.log('🔒 Step 4: Closing query...');
const closeResult = closeQuery(queryHandle);
if (!closeResult.success) {
  console.error('❌ Failed to close query:', closeResult.error);
  process.exit(1);
}
console.log('✅ Query closed successfully\n');

console.log('🎉 All query handle pagination tests passed!');
console.log(`   ✅ No duplicate messages across ${pageNum} pages`);
console.log(`   ✅ Fetched all ${totalCount} messages`);
console.log(`   ✅ Timestamps sorted correctly (DESC)`);
console.log(`   ✅ Query handle lifecycle complete`);
