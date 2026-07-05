#!/usr/bin/env npx tsx

/**
 * Cursor Pagination Integration Test
 *
 * Tests that cursor pagination works correctly end-to-end:
 * 1. Query without cursor returns first N messages
 * 2. Query with cursor from result 1 returns NEXT N messages (no overlap)
 * 3. Repeating with cursor should never return duplicates
 */

import { execSync } from 'child_process';

function runDataQuery(cursor?: string): any {
  const cursorArg = cursor ? `--cursor='{"field":"timestamp","value":"${cursor}","direction":"before"}'` : '';
  const cmd = `./jtag data/list --collection=chat_messages --filter='{"roomId":"5e71a0c8-0303-4eb8-a478-3a121248","status":"sent"}' --orderBy='[{"field":"timestamp","direction":"desc"}]' --limit=5 ${cursorArg}`;

  console.log(`Running: ${cmd}`);
  const output = execSync(cmd, { encoding: 'utf8' });
  return JSON.parse(output);
}

console.log('🧪 Testing Cursor Pagination Integration');
console.log('=========================================\n');

// Query 1: Get first 5 messages
console.log('📋 Query 1: No cursor, get first 5 messages');
const result1 = runDataQuery();
if (!result1.success) {
  console.error('❌ Query 1 failed:', result1.error);
  process.exit(1);
}
console.log(`✅ Got ${result1.items.length} messages`);
console.log(`   IDs: ${result1.items.map((m: any) => m.id.substring(0, 8)).join(', ')}`);
console.log(`   Timestamps: ${result1.items.map((m: any) => m.timestamp).join(', ')}`);

const cursor1 = result1.items[result1.items.length - 1].timestamp;
console.log(`   Cursor for next query: ${cursor1}\n`);

// Query 2: Get next 5 messages using cursor
console.log('📋 Query 2: With cursor, get next 5 messages');
const result2 = runDataQuery(cursor1);
if (!result2.success) {
  console.error('❌ Query 2 failed:', result2.error);
  process.exit(1);
}
console.log(`✅ Got ${result2.items.length} messages`);
console.log(`   IDs: ${result2.items.map((m: any) => m.id.substring(0, 8)).join(', ')}`);
console.log(`   Timestamps: ${result2.items.map((m: any) => m.timestamp).join(', ')}\n`);

// Check for overlap
const ids1 = new Set(result1.items.map((m: any) => m.id));
const ids2 = new Set(result2.items.map((m: any) => m.id));
const overlap = [...ids1].filter(id => ids2.has(id));

if (overlap.length > 0) {
  console.error('❌ FAILED: Queries returned overlapping messages!');
  console.error(`   Duplicate IDs: ${overlap.map(id => id.substring(0, 8)).join(', ')}`);
  process.exit(1);
}

console.log('✅ PASSED: No overlap between queries');
console.log(`   Query 1: ${ids1.size} unique messages`);
console.log(`   Query 2: ${ids2.size} unique messages`);
console.log(`   Combined: ${ids1.size + ids2.size} total unique messages\n`);

// Query 3: Verify cursor consistency
console.log('📋 Query 3: Same cursor again (should get same results as Query 2)');
const result3 = runDataQuery(cursor1);
const ids3 = new Set(result3.items.map((m: any) => m.id));

const sameAsQuery2 = ids3.size === ids2.size && [...ids3].every(id => ids2.has(id));
if (!sameAsQuery2) {
  console.error('❌ FAILED: Query 3 returned different results than Query 2!');
  console.error(`   Query 2 IDs: ${[...ids2].map(id => id.substring(0, 8)).join(', ')}`);
  console.error(`   Query 3 IDs: ${[...ids3].map(id => id.substring(0, 8)).join(', ')}`);
  process.exit(1);
}

console.log('✅ PASSED: Query 3 returned same results as Query 2 (cursor is stable)\n');

console.log('🎉 All cursor pagination tests passed!');
