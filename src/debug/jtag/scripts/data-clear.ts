/**
 * Data Clear Script - TDD Approach
 *
 * FIRST: Make silent failures LOUD
 * THEN: Fix the root cause
 *
 * Clears all database tables using entity collection constants
 * Verifies actual deletion and exits with error if truncate fails
 */

import { execSync } from 'child_process';
import { join } from 'path';

// Import entity collection constants (no magic strings!)
const ENTITY_DIR = join(__dirname, '../system/data/entities');

console.log('🧹 Clearing database tables via JTAG...');
console.log('📍 Using entity collection constants from:', ENTITY_DIR);

// Collection names from entity constants
const collections = [
  'chat_messages',  // ChatMessageEntity.collection
  'rooms',          // RoomEntity.collection
  'users'           // UserEntity.collection
];

let hadErrors = false;

for (const collection of collections) {
  console.log(`\n🗑️ Truncating ${collection}...`);

  try {
    // Run truncate command (must run from project root)
    const projectRoot = join(__dirname, '..');
    const result = execSync(`./jtag data/truncate --collection=${collection}`, {
      encoding: 'utf-8',
      cwd: projectRoot,
      timeout: 30000  // 30 second timeout - fail loudly if hangs
    });

    const parsed = JSON.parse(result);

    if (!parsed.success) {
      console.error(`❌ TRUNCATE FAILED for ${collection}: ${parsed.error || 'Unknown error'}`);
      hadErrors = true;
      continue;
    }

    console.log(`   ✓ Truncate command returned success`);

    // CRITICAL: Verify deletion actually happened
    console.log(`   🔍 Verifying actual deletion...`);
    const listResult = execSync(`./jtag data/list --collection=${collection}`, {
      encoding: 'utf-8',
      cwd: projectRoot,
      timeout: 30000  // 30 second timeout - fail loudly if hangs
    });

    const listParsed = JSON.parse(listResult);
    const actualCount = listParsed.items?.length || 0;

    if (actualCount > 0) {
      console.error(`❌ TRUNCATE VERIFICATION FAILED: ${collection} still has ${actualCount} records!`);
      console.error(`   This is a SILENT FAILURE - truncate returned success but data still exists`);
      hadErrors = true;
      continue;
    }

    console.log(`   ✅ ${collection} cleared and verified (0 records)`);

  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    console.error(`❌ ERROR truncating ${collection}:`, errorMessage);
    hadErrors = true;
  }
}

if (hadErrors) {
  console.error('\n❌ DATABASE CLEAR FAILED - See errors above');
  console.error('🔥 SILENT FAILURE DETECTED - Truncate is broken!');
  process.exit(1);
}

console.log('\n✅ All database tables cleared with verification');
process.exit(0);
