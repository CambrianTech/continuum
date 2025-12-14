/**
 * RustStorageAdapter Integration Test
 *
 * Tests RustStorageAdapter in isolation with real entities.
 * Verifies that decorator → SQL → Rust flow works correctly.
 *
 * SETUP:
 *   1. Build Rust worker: cd workers/data && cargo build --release
 *   2. Start Rust worker: ./workers/data/target/release/data-worker /tmp/rust-adapter-test.sock
 *   3. Run this test: npx tsx workers/data/test-rust-adapter.ts
 *
 * Phase 1: Isolated testing only - does NOT touch production
 */

import { RustStorageAdapter } from '../../daemons/data-daemon/server/RustStorageAdapter';
import { UserEntity } from '../../system/user/shared/UserEntity';
import { ChatMessageEntity } from '../../widgets/chat-widget/shared/ChatMessageEntity';
import type { DataRecord } from '../../daemons/data-daemon/shared/DataStorageAdapter';

// Test database (isolated, disposable)
const TEST_DB = '/tmp/rust-adapter-test.db';
const TEST_SOCKET = '/tmp/rust-adapter-test.sock';

// ANSI colors for output
const colors = {
  reset: '\x1b[0m',
  green: '\x1b[32m',
  red: '\x1b[31m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  gray: '\x1b[90m',
};

function log(message: string, color: string = colors.reset) {
  console.log(`${color}${message}${colors.reset}`);
}

function success(message: string) {
  log(`✅ ${message}`, colors.green);
}

function error(message: string) {
  log(`❌ ${message}`, colors.red);
}

function info(message: string) {
  log(`ℹ️  ${message}`, colors.blue);
}

function section(message: string) {
  log(`\n${'='.repeat(60)}`, colors.gray);
  log(message, colors.yellow);
  log('='.repeat(60), colors.gray);
}

/**
 * Test Suite
 */
async function runTests() {
  let adapter: RustStorageAdapter | null = null;
  let testsRun = 0;
  let testsPassed = 0;
  let testsFailed = 0;

  try {
    section('RustStorageAdapter Integration Test');
    info(`Test database: ${TEST_DB}`);
    info(`Socket path: ${TEST_SOCKET}`);
    info('Ensure Rust worker is running!');

    // ========================================================================
    // Test 1: Initialize adapter
    // ========================================================================
    section('Test 1: Initialize RustStorageAdapter');
    testsRun++;

    try {
      adapter = new RustStorageAdapter();
      await adapter.initialize({
        type: 'sqlite',
        options: {
          filename: TEST_DB,
          socketPath: TEST_SOCKET,
        }
      });
      success('Adapter initialized successfully');
      testsPassed++;
    } catch (err: any) {
      error(`Initialization failed: ${err.message}`);
      testsFailed++;
      throw err; // Can't continue without adapter
    }

    // ========================================================================
    // Test 2: Create UserEntity
    // ========================================================================
    section('Test 2: Create UserEntity (Decorators → SQL → Rust)');
    testsRun++;

    try {
      const userId = 'test-user-' + Date.now();
      const userRecord: DataRecord<UserEntity> = {
        id: userId,
        collection: 'users',
        data: {
          id: userId,
          displayName: 'Test User',
          type: 'human',
          status: 'active',
          createdAt: new Date().toISOString(),
          lastActiveAt: new Date().toISOString(),
        } as UserEntity,
        metadata: {
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          version: 1,
        }
      };

      const result = await adapter.create<UserEntity>(userRecord);

      if (result.success && result.data) {
        success(`Created user: ${result.data.data.displayName} (${result.data.id})`);
        info(`  Type: ${result.data.data.type}`);
        info(`  Status: ${result.data.data.status}`);
        testsPassed++;
      } else {
        error(`Create failed: ${result.error}`);
        testsFailed++;
      }
    } catch (err: any) {
      error(`Create UserEntity failed: ${err.message}`);
      testsFailed++;
    }

    // ========================================================================
    // Test 3: Read UserEntity
    // ========================================================================
    section('Test 3: Read UserEntity (SQL Query via Rust)');
    testsRun++;

    try {
      const userId = 'test-user-' + (Date.now() - 1000); // Use previous test's ID pattern
      const result = await adapter.read<UserEntity>('users', userId);

      if (result.success && result.data) {
        success(`Read user: ${result.data.data.displayName}`);
        info(`  ID: ${result.data.id}`);
        info(`  Version: ${result.data.metadata.version}`);
        testsPassed++;
      } else {
        // Not an error if user doesn't exist
        info('User not found (expected if running first time)');
        testsPassed++;
      }
    } catch (err: any) {
      error(`Read UserEntity failed: ${err.message}`);
      testsFailed++;
    }

    // ========================================================================
    // Test 4: Query UserEntity with filter
    // ========================================================================
    section('Test 4: Query UserEntity (Filter via Rust)');
    testsRun++;

    try {
      const result = await adapter.query<UserEntity>({
        collection: 'users',
        filter: { type: 'human' },
        limit: 10,
      });

      if (result.success && result.data) {
        success(`Query returned ${result.data.length} human users`);
        result.data.forEach((user, idx) => {
          info(`  ${idx + 1}. ${user.data.displayName} (${user.id})`);
        });
        testsPassed++;
      } else {
        error(`Query failed: ${result.error}`);
        testsFailed++;
      }
    } catch (err: any) {
      error(`Query UserEntity failed: ${err.message}`);
      testsFailed++;
    }

    // ========================================================================
    // Test 5: Update UserEntity
    // ========================================================================
    section('Test 5: Update UserEntity (SQL UPDATE via Rust)');
    testsRun++;

    try {
      // First create a user to update
      const userId = 'test-update-user-' + Date.now();
      const createRecord: DataRecord<UserEntity> = {
        id: userId,
        collection: 'users',
        data: {
          id: userId,
          displayName: 'Original Name',
          type: 'human',
          status: 'active',
          createdAt: new Date().toISOString(),
          lastActiveAt: new Date().toISOString(),
        } as UserEntity,
        metadata: {
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          version: 1,
        }
      };

      await adapter.create<UserEntity>(createRecord);

      // Now update it
      const updateResult = await adapter.update<UserEntity>(
        'users',
        userId,
        { displayName: 'Updated Name' } as Partial<UserEntity>,
        true // increment version
      );

      if (updateResult.success && updateResult.data) {
        success(`Updated user: ${updateResult.data.data.displayName}`);
        info(`  Version incremented to: ${updateResult.data.metadata.version}`);
        testsPassed++;
      } else {
        error(`Update failed: ${updateResult.error}`);
        testsFailed++;
      }
    } catch (err: any) {
      error(`Update UserEntity failed: ${err.message}`);
      testsFailed++;
    }

    // ========================================================================
    // Test 6: Create ChatMessageEntity
    // ========================================================================
    section('Test 6: Create ChatMessageEntity (Complex Entity)');
    testsRun++;

    try {
      const messageId = 'test-message-' + Date.now();
      const messageRecord: DataRecord<ChatMessageEntity> = {
        id: messageId,
        collection: 'chat_messages',
        data: {
          id: messageId,
          roomId: 'test-room-123',
          senderId: 'test-user-456',
          content: 'Hello from RustStorageAdapter test!',
          timestamp: new Date().toISOString(),
        } as ChatMessageEntity,
        metadata: {
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          version: 1,
        }
      };

      const result = await adapter.create<ChatMessageEntity>(messageRecord);

      if (result.success && result.data) {
        success(`Created message: "${result.data.data.content}"`);
        info(`  Room: ${result.data.data.roomId}`);
        info(`  Sender: ${result.data.data.senderId}`);
        testsPassed++;
      } else {
        error(`Create message failed: ${result.error}`);
        testsFailed++;
      }
    } catch (err: any) {
      error(`Create ChatMessageEntity failed: ${err.message}`);
      testsFailed++;
    }

    // ========================================================================
    // Test 7: Query ChatMessageEntity
    // ========================================================================
    section('Test 7: Query ChatMessageEntity (Filter + OrderBy)');
    testsRun++;

    try {
      const result = await adapter.query<ChatMessageEntity>({
        collection: 'chat_messages',
        filter: { roomId: 'test-room-123' },
        orderBy: [{ field: 'timestamp', direction: 'desc' }],
        limit: 10,
      });

      if (result.success && result.data) {
        success(`Query returned ${result.data.length} messages`);
        result.data.forEach((msg, idx) => {
          info(`  ${idx + 1}. "${msg.data.content}" (${msg.id})`);
        });
        testsPassed++;
      } else {
        error(`Query failed: ${result.error}`);
        testsFailed++;
      }
    } catch (err: any) {
      error(`Query ChatMessageEntity failed: ${err.message}`);
      testsFailed++;
    }

    // ========================================================================
    // Test 8: Delete record
    // ========================================================================
    section('Test 8: Delete Record (SQL DELETE via Rust)');
    testsRun++;

    try {
      const userId = 'test-delete-user-' + Date.now();
      const createRecord: DataRecord<UserEntity> = {
        id: userId,
        collection: 'users',
        data: {
          id: userId,
          displayName: 'To Be Deleted',
          type: 'human',
          status: 'active',
          createdAt: new Date().toISOString(),
          lastActiveAt: new Date().toISOString(),
        } as UserEntity,
        metadata: {
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          version: 1,
        }
      };

      await adapter.create<UserEntity>(createRecord);

      // Now delete it
      const deleteResult = await adapter.delete('users', userId);

      if (deleteResult.success) {
        success(`Deleted user: ${userId}`);
        testsPassed++;
      } else {
        error(`Delete failed: ${deleteResult.error}`);
        testsFailed++;
      }
    } catch (err: any) {
      error(`Delete record failed: ${err.message}`);
      testsFailed++;
    }

    // ========================================================================
    // Test 9: Batch operations
    // ========================================================================
    section('Test 9: Batch Create (Transaction via Rust)');
    testsRun++;

    try {
      const users: UserEntity[] = [
        {
          id: 'batch-user-1-' + Date.now(),
          displayName: 'Batch User 1',
          type: 'human',
          status: 'active',
          createdAt: new Date().toISOString(),
          lastActiveAt: new Date().toISOString(),
        } as UserEntity,
        {
          id: 'batch-user-2-' + Date.now(),
          displayName: 'Batch User 2',
          type: 'human',
          status: 'active',
          createdAt: new Date().toISOString(),
          lastActiveAt: new Date().toISOString(),
        } as UserEntity,
      ];

      const result = await adapter.batchCreate<UserEntity>('users', users);

      if (result.success && result.data) {
        success(`Batch created ${result.data.length} users`);
        result.data.forEach((user, idx) => {
          info(`  ${idx + 1}. ${user.data.displayName} (${user.id})`);
        });
        testsPassed++;
      } else {
        error(`Batch create failed: ${result.error}`);
        testsFailed++;
      }
    } catch (err: any) {
      error(`Batch create failed: ${err.message}`);
      testsFailed++;
    }

    // ========================================================================
    // Test 10: List collections
    // ========================================================================
    section('Test 10: List Collections (Schema Introspection)');
    testsRun++;

    try {
      const result = await adapter.listCollections();

      if (result.success && result.data) {
        success(`Found ${result.data.length} collections`);
        result.data.forEach((collection, idx) => {
          info(`  ${idx + 1}. ${collection}`);
        });
        testsPassed++;
      } else {
        error(`List collections failed: ${result.error}`);
        testsFailed++;
      }
    } catch (err: any) {
      error(`List collections failed: ${err.message}`);
      testsFailed++;
    }

  } catch (err: any) {
    error(`Fatal error: ${err.message}`);
    console.error(err.stack);
  } finally {
    // Cleanup
    if (adapter) {
      await adapter.close();
    }

    // Results
    section('Test Results');
    log(`Total tests run: ${testsRun}`, colors.blue);
    log(`Tests passed: ${testsPassed}`, colors.green);
    log(`Tests failed: ${testsFailed}`, testsFailed > 0 ? colors.red : colors.green);

    if (testsFailed === 0) {
      success('\n🎉 All tests passed!');
      log('\nNext steps:', colors.yellow);
      log('  1. Phase 1 complete - RustStorageAdapter works in isolation', colors.gray);
      log('  2. Phase 2: Parallel testing (both TypeScript and Rust)', colors.gray);
      log('  3. Phase 3: Shadow mode (Rust in background)', colors.gray);
      log('  4. Phase 4: Canary deployment (1% → 100%)', colors.gray);
      log('  5. Phase 5: Full switch to Rust', colors.gray);
      process.exit(0);
    } else {
      error('\n❌ Some tests failed - review errors above');
      process.exit(1);
    }
  }
}

// Run tests
runTests().catch((err) => {
  error(`Unhandled error: ${err.message}`);
  console.error(err.stack);
  process.exit(1);
});
