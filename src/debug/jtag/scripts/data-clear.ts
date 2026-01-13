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
import { writeFileSync, existsSync, mkdirSync } from 'fs';
import { DATA_COMMANDS } from '../commands/data/shared/DataCommandConstants';

// Import entity collection constants (no magic strings!)
const ENTITY_DIR = join(__dirname, '../system/data/entities');

console.log('🧹 Clearing database tables via JTAG...');
console.log('📍 Using entity collection constants from:', ENTITY_DIR);

// Collection names from entity constants
// NOTE: Users are NOT cleared to preserve persona UUIDs!
// Persona longterm.db files are stored per-persona and reference the user's UUID.
// If we clear users and reseed, they get NEW UUIDs, orphaning all their memories.
// This was the root cause of the "orphaned memories" bug (2905 memories unreachable).
const collections = [
  'chat_messages',  // ChatMessageEntity.collection
  'rooms'           // RoomEntity.collection
  // 'users' - INTENTIONALLY EXCLUDED to preserve persona UUIDs and their memories
];

let hadErrors = false;

for (const collection of collections) {
  console.log(`\n🗑️ Truncating ${collection}...`);

  try {
    // Run truncate command (must run from project root)
    const projectRoot = join(__dirname, '..');
    const result = execSync(`./jtag ${DATA_COMMANDS.TRUNCATE} --collection=${collection}`, {
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
    const listResult = execSync(`./jtag ${DATA_COMMANDS.LIST} --collection=${collection}`, {
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

// Clear session metadata files to prevent stale entityIds from persisting
// BUG PREVENTION: Session metadata stores entityIds that become invalid after reseed
// If not cleared, browser will try to load rooms using old UUIDs that no longer exist
console.log('\n🧹 Clearing session metadata files...');
const exampleDirs = ['examples/widget-ui', 'examples/test-bench'];
const projectRoot = join(__dirname, '..');

for (const exampleDir of exampleDirs) {
  const metadataDir = join(projectRoot, exampleDir, '.continuum/jtag/sessions');
  const metadataPath = join(metadataDir, 'metadata.json');

  try {
    // Ensure directory exists
    if (!existsSync(metadataDir)) {
      mkdirSync(metadataDir, { recursive: true });
    }

    const emptyMetadata = {
      projectContext: join(projectRoot, exampleDir),
      sessions: [],
      lastUpdated: new Date().toISOString(),
      version: "1.0.0"
    };

    writeFileSync(metadataPath, JSON.stringify(emptyMetadata, null, 2));
    console.log(`   ✅ Cleared: ${metadataPath}`);
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    console.warn(`   ⚠️ Could not clear ${metadataPath}: ${errorMessage}`);
    // Don't fail the whole script for metadata clearing - it's cleanup, not critical
  }
}

console.log('\n✅ Data clear complete');
process.exit(0);
