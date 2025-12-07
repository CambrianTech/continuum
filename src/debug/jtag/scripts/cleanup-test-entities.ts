#!/usr/bin/env tsx
/**
 * Cleanup Test Entities Script
 *
 * Removes test entities left over from failed integration tests.
 * Uses CLI commands to avoid deadlock during startup.
 *
 * Usage:
 *   npm run data:cleanup              # DRY RUN (safe - shows what would be deleted)
 *   npm run data:cleanup -- --delete  # ACTUAL DELETION (requires confirmation)
 */

import { spawnSync } from 'child_process';
import { UserEntity } from '../system/data/entities/UserEntity';
import { RoomEntity } from '../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../system/data/entities/ChatMessageEntity';
import { isTestUser, isTestRoom, isTestMessage } from '../tests/shared/TestEntityConstants';
import { extractJSONOrThrow } from './shared/json-extraction';
import * as readline from 'readline';

/**
 * Old uniqueId formats that should be cleaned up
 */
const OLD_BROKEN_UNIQUE_IDS = [
  'human-joel',
  'primary-human',
  'claude-code',
  'general-ai',
  'persona-helper-001',
  'persona-teacher-001',
  'persona-codereview-001',
];

/**
 * Helper to execute JTAG CLI command and extract JSON response
 */
function executeCommand(commandStr: string): any {
  try {
    // Parse command string into args array, handling quoted strings properly
    const args: string[] = [];
    let current = '';
    let inQuotes = false;

    for (let i = 0; i < commandStr.length; i++) {
      const char = commandStr[i];

      if (char === "'" || char === '"') {
        inQuotes = !inQuotes;
      } else if (char === ' ' && !inQuotes) {
        if (current) {
          args.push(current);
          current = '';
        }
      } else {
        current += char;
      }
    }

    if (current) {
      args.push(current);
    }

    const result = spawnSync('./jtag', args, {
      encoding: 'utf-8',
      maxBuffer: 50 * 1024 * 1024, // 50MB buffer
      shell: false
    });

    if (result.error) {
      throw result.error;
    }

    const output = result.stdout + result.stderr;
    return extractJSONOrThrow(output, `command: ./jtag ${commandStr}`);
  } catch (error) {
    console.error(`Failed to execute: ./jtag ${commandStr}`);
    throw error;
  }
}

/**
 * Delete an entity by ID
 */
function deleteEntity(collection: string, id: string, displayInfo: string): boolean {
  try {
    console.log(`   🗑️  Deleting ${displayInfo}...`);
    const result = executeCommand(`data/delete --collection=${collection} --id=${id}`);

    if (result.success) {
      console.log(`   ✅ Deleted successfully`);
      return true;
    } else {
      // Show full result object for debugging if error is generic
      if (!result.error || result.error === 'Unknown error') {
        console.error(`   ❌ Delete failed - Full response:`, JSON.stringify(result, null, 2));
      } else {
        console.error(`   ❌ Delete failed: ${result.error}`);
      }
      return false;
    }
  } catch (error) {
    console.error(`   ❌ Delete failed with exception:`, error);
    return false;
  }
}

/**
 * Prompt user for confirmation
 */
async function confirmDeletion(count: number): Promise<boolean> {
  if (count === 0) {
    return false; // Nothing to delete
  }

  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout
  });

  return new Promise((resolve) => {
    console.log(`\n⚠️  WARNING: You are about to delete ${count} test entities`);
    console.log(`⚠️  This action CANNOT be undone!`);
    rl.question('\n❓ Type "DELETE" (all caps) to confirm deletion: ', (answer) => {
      rl.close();
      resolve(answer === 'DELETE');
    });
  });
}

async function cleanupTestEntities(deleteMode: boolean): Promise<void> {
  if (deleteMode) {
    console.log('🔍 Scanning for test entities (DELETE MODE)...');
  } else {
    console.log('🔍 Scanning for test entities (DRY RUN - not deleting)...');
  }

  try {
    let foundCount = 0;
    const entitiesToDelete: Array<{ collection: string; id: string; displayInfo: string }> = [];

    // Check test users - use verbose=false to reduce payload size
    console.log('\n📋 Checking users...');
    const usersResult = executeCommand(`data/list --collection=${UserEntity.collection} --limit=100 --verbose=false`);

    if (usersResult.success && Array.isArray(usersResult.items)) {
      console.log(`   Found ${usersResult.items.length} users (first 100)`);

      for (const user of usersResult.items) {
        // Check if test user
        if (isTestUser(user)) {
          const displayInfo = `test user: ${user.displayName} (${user.uniqueId})`;
          console.log(`   ❌ ${deleteMode ? 'WILL DELETE' : 'WOULD DELETE'} ${displayInfo} [id: ${user.id}]`);
          foundCount++;
          entitiesToDelete.push({
            collection: UserEntity.collection,
            id: user.id,
            displayInfo
          });
        }

        // Check for old broken uniqueId formats
        if (user.uniqueId && OLD_BROKEN_UNIQUE_IDS.includes(user.uniqueId)) {
          const displayInfo = `old broken uniqueId: ${user.displayName} (${user.uniqueId})`;
          console.log(`   ❌ ${deleteMode ? 'WILL DELETE' : 'WOULD DELETE'} ${displayInfo} [id: ${user.id}]`);
          foundCount++;
          entitiesToDelete.push({
            collection: UserEntity.collection,
            id: user.id,
            displayInfo
          });
        }
      }
    }

    // Check test rooms - use verbose=false to reduce payload size
    console.log('\n📋 Checking rooms...');
    const roomsResult = executeCommand(`data/list --collection=${RoomEntity.collection} --limit=100 --verbose=false`);

    if (roomsResult.success && Array.isArray(roomsResult.items)) {
      console.log(`   Found ${roomsResult.items.length} rooms (first 100)`);

      for (const room of roomsResult.items) {
        if (isTestRoom(room)) {
          const displayInfo = `test room: ${room.displayName}`;
          console.log(`   ❌ ${deleteMode ? 'WILL DELETE' : 'WOULD DELETE'} ${displayInfo} [id: ${room.id}]`);
          foundCount++;
          entitiesToDelete.push({
            collection: RoomEntity.collection,
            id: room.id,
            displayInfo
          });
        }
      }
    }

    // TODO: Message checking disabled - requires full content field which causes buffer truncation
    // Messages should be cleaned up by test cleanup hooks anyway
    // If needed, could implement batched checking with smaller limits
    console.log('\n📋 Checking messages... (SKIPPED - would require large payload)');

    console.log(`\n✅ Scan complete: found ${foundCount} test entities`);

    // If delete mode, ask for confirmation and delete
    if (deleteMode && foundCount > 0) {
      const confirmed = await confirmDeletion(foundCount);

      if (confirmed) {
        console.log(`\n🗑️  Starting deletion of ${foundCount} entities...\n`);

        let successCount = 0;
        let failCount = 0;

        for (const entity of entitiesToDelete) {
          if (deleteEntity(entity.collection, entity.id, entity.displayInfo)) {
            successCount++;
          } else {
            failCount++;
          }
        }

        console.log(`\n✅ Deletion complete: ${successCount} succeeded, ${failCount} failed`);
      } else {
        console.log(`\n❌ Deletion cancelled by user`);
      }
    } else if (!deleteMode && foundCount > 0) {
      console.log(`\n💡 This was a DRY RUN - no entities were deleted`);
      console.log(`💡 To actually delete, run: npm run data:cleanup -- --delete`);
    } else if (foundCount === 0) {
      console.log(`\n💡 No test entities found - database is clean!`);
    }

  } catch (error) {
    console.error(`❌ Scan failed:`, error);
    process.exit(1);
  }
}

// Parse command line arguments
const args = process.argv.slice(2);
const deleteMode = args.includes('--delete');

// Run cleanup
cleanupTestEntities(deleteMode);
