#!/usr/bin/env tsx
/**
 * Training Data Pipeline Integration Test
 *
 * TDD test that validates training database creation with actual data.
 * Tests the complete workflow:
 * 1. Open training database with data/open
 * 2. Create TrainingExampleEntity records with data/create using dbHandle
 * 3. Query database to verify data exists
 * 4. Validate schema is correct for MLX training
 * 5. Verify database is ready for training pipeline
 * 6. Clean up (close database, delete temp files)
 *
 * This test SHOULD FAIL until:
 * - training/import command is implemented
 * - TrainingExampleEntity properly stores data
 * - Multi-database operations work end-to-end
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { Commands } from '../../system/core/shared/Commands';
import type { DataOpenResult } from '../../commands/data/open/shared/DataOpenTypes';
import type { DataCreateResult } from '../../commands/data/create/shared/DataCreateTypes';
import type { DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { DataCloseResult } from '../../commands/data/close/shared/DataCloseTypes';
import type { DbHandle } from '../../daemons/data-daemon/server/DatabaseHandleRegistry';
import type { TrainingMessage } from '../../daemons/data-daemon/shared/entities/TrainingExampleEntity';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

describe('Training Data Pipeline Integration Test', () => {
  let tempDbPath: string;
  let dbHandle: DbHandle;

  beforeAll(() => {
    // Create temporary database path
    const tmpDir = os.tmpdir();
    tempDbPath = path.join(tmpDir, `training-test-${Date.now()}.sqlite`);
    console.log(`📝 Test database path: ${tempDbPath}`);
  });

  afterAll(async () => {
    // Clean up: close database handle if still open
    if (dbHandle && dbHandle !== 'default') {
      try {
        await Commands.execute<DataCloseResult>('data/close', {
          dbHandle
        });
        console.log(`✅ Closed database handle: ${dbHandle}`);
      } catch (err) {
        console.warn(`⚠️ Failed to close handle: ${err}`);
      }
    }

    // Delete temporary database file
    if (fs.existsSync(tempDbPath)) {
      fs.unlinkSync(tempDbPath);
      console.log(`🗑️ Deleted test database: ${tempDbPath}`);
    }
  });

  it('should create training database and validate MLX-ready data', { timeout: 60000 }, async () => {
    // Step 1: Open training database
    console.log('\n🔧 Step 1: Opening training database...');
    const openResult = await Commands.execute<DataOpenResult>('data/open', {
      adapter: 'sqlite',
      config: {
        path: tempDbPath,
        mode: 'create'
      }
    });

    expect(openResult.success).toBe(true);
    expect(openResult.dbHandle).toBeDefined();
    expect(openResult.dbHandle).not.toBe('default');
    dbHandle = openResult.dbHandle;
    console.log(`✅ Database opened with handle: ${dbHandle}`);

    // Step 2: Create training examples using TrainingExampleEntity
    console.log('\n🔧 Step 2: Creating training examples...');

    const trainingExamples = [
      {
        messages: [
          { role: 'system', content: 'You are a helpful AI assistant.' },
          { role: 'user', content: 'What is TypeScript?' },
          { role: 'assistant', content: 'TypeScript is a typed superset of JavaScript that compiles to plain JavaScript.' }
        ] as readonly TrainingMessage[],
        messageCount: 3,
        totalTokens: 45,
        metadata: { source: 'integration-test', difficulty: 'easy' }
      },
      {
        messages: [
          { role: 'system', content: 'You are a code expert.' },
          { role: 'user', content: 'How do I use async/await?' },
          { role: 'assistant', content: 'async/await provides a cleaner syntax for working with Promises. Use async before a function to make it return a Promise, and await to pause execution until a Promise resolves.' }
        ] as readonly TrainingMessage[],
        messageCount: 3,
        totalTokens: 67,
        metadata: { source: 'integration-test', difficulty: 'medium' }
      },
      {
        messages: [
          { role: 'system', content: 'You are a database expert.' },
          { role: 'user', content: 'What is an index?' },
          { role: 'assistant', content: 'A database index is a data structure that improves the speed of data retrieval operations on a table at the cost of additional writes and storage space.' }
        ] as readonly TrainingMessage[],
        messageCount: 3,
        totalTokens: 52,
        metadata: { source: 'integration-test', difficulty: 'medium' }
      }
    ];

    const createdIds: string[] = [];

    for (let i = 0; i < trainingExamples.length; i++) {
      const example = trainingExamples[i];
      console.log(`  Creating example ${i + 1}/${trainingExamples.length}...`);

      const createResult = await Commands.execute<DataCreateResult>('data/create', {
        collection: 'training_examples',
        data: example,
        dbHandle
      });

      expect(createResult.success).toBe(true);
      expect(createResult.data).toBeDefined();
      expect(createResult.data?.id).toBeDefined();
      createdIds.push(createResult.data!.id);
      console.log(`  ✅ Created example with ID: ${createResult.data!.id}`);
    }

    console.log(`✅ Created ${trainingExamples.length} training examples`);

    // Step 3: Query database to verify data exists
    console.log('\n🔧 Step 3: Querying database to verify data...');

    const listResult = await Commands.execute<DataListResult>(DATA_COMMANDS.LIST, {
      collection: 'training_examples',
      dbHandle
    });

    expect(listResult.success).toBe(true);
    expect(listResult.data).toBeDefined();
    expect(Array.isArray(listResult.data)).toBe(true);
    expect(listResult.data!.length).toBe(trainingExamples.length);
    console.log(`✅ Found ${listResult.data!.length} training examples in database`);

    // Step 4: Validate schema is correct for MLX training
    console.log('\n🔧 Step 4: Validating MLX training schema...');

    for (let i = 0; i < listResult.data!.length; i++) {
      const entity = listResult.data![i];
      console.log(`  Validating entity ${i + 1}/${listResult.data!.length}...`);

      // MLX requires: id, messages array, messageCount, totalTokens
      expect(entity.id).toBeDefined();
      expect(typeof entity.id).toBe('string');

      expect(entity.messages).toBeDefined();
      expect(Array.isArray(entity.messages)).toBe(true);
      expect(entity.messages.length).toBeGreaterThan(0);

      // Validate each message has role and content
      for (const msg of entity.messages) {
        expect(msg.role).toBeDefined();
        expect(['system', 'user', 'assistant']).toContain(msg.role);
        expect(msg.content).toBeDefined();
        expect(typeof msg.content).toBe('string');
      }

      expect(entity.messageCount).toBeDefined();
      expect(typeof entity.messageCount).toBe('number');
      expect(entity.messageCount).toBe(entity.messages.length);

      expect(entity.totalTokens).toBeDefined();
      expect(typeof entity.totalTokens).toBe('number');
      expect(entity.totalTokens).toBeGreaterThan(0);

      expect(entity.metadata).toBeDefined();
      expect(typeof entity.metadata).toBe('object');

      console.log(`  ✅ Entity ${entity.id} is MLX-compatible`);
    }

    console.log('✅ All entities are MLX training-ready');

    // Step 5: Verify database statistics
    console.log('\n🔧 Step 5: Verifying database statistics...');

    const totalTokens = listResult.data!.reduce((sum, entity) => sum + entity.totalTokens, 0);
    const averageTokens = totalTokens / listResult.data!.length;

    console.log(`  📊 Total examples: ${listResult.data!.length}`);
    console.log(`  📊 Total tokens: ${totalTokens}`);
    console.log(`  📊 Average tokens per example: ${averageTokens.toFixed(2)}`);

    expect(totalTokens).toBeGreaterThan(0);
    expect(averageTokens).toBeGreaterThan(0);
    console.log('✅ Database statistics validated');

    // Step 6: Verify file exists on disk
    console.log('\n🔧 Step 6: Verifying database file...');

    expect(fs.existsSync(tempDbPath)).toBe(true);
    const stats = fs.statSync(tempDbPath);
    expect(stats.size).toBeGreaterThan(0);
    console.log(`✅ Database file exists: ${tempDbPath} (${stats.size} bytes)`);

    // Step 7: Close database handle
    console.log('\n🔧 Step 7: Closing database handle...');

    const closeResult = await Commands.execute<DataCloseResult>('data/close', {
      dbHandle
    });

    expect(closeResult.success).toBe(true);
    console.log(`✅ Database handle closed: ${dbHandle}`);

    // Clear handle so afterAll doesn't try to close again
    dbHandle = 'default';

    console.log('\n✅ Training data pipeline test completed successfully');
    console.log('🎯 Database is ready for MLX fine-tuning');
  });
});
