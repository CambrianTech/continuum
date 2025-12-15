#!/usr/bin/env npx tsx
/**
 * Multi-Database Handles Integration Test
 *
 * Tests the complete multi-database handle system:
 * - data/open command
 * - data/close command
 * - data/list-handles command
 * - DatabaseHandleRegistry
 *
 * This demonstrates that the training data pipeline can:
 * 1. Open training databases
 * 2. Operate on them independently
 * 3. Close them when done
 * 4. List all open handles
 */

import { Commands } from '../../system/core/shared/Commands';
import type { DataOpenResult } from '../../commands/data/open/shared/DataOpenTypes';
import type { DataCloseResult } from '../../commands/data/close/shared/DataCloseTypes';
import type { DataListHandlesResult } from '../../commands/data/list-handles/shared/DataListHandlesTypes';
import * as fs from 'fs/promises';
import * as path from 'path';
import { tmpdir } from 'os';

console.log('🧪 MULTI-DATABASE HANDLES INTEGRATION TEST');
console.log('===========================================\n');

async function runTests(): Promise<void> {
  const tempDirs: string[] = [];

  try {
    console.log('📋 Starting test suite (using Commands.execute directly)\n');

    // Test 1: List handles before opening any
    console.log('📋 TEST 1: List handles (should only show default)');
    const listResult1 = await Commands.execute<DataListHandlesResult>(DATA_COMMANDS.LIST_HANDLES, {});
    console.log(`  Handles: ${listResult1.handles.length}`);
    console.log(`  Default handle exists: ${listResult1.handles.some(h => h.isDefault)}`);
    
    if (!listResult1.success || listResult1.handles.length !== 1 || !listResult1.handles[0].isDefault) {
      throw new Error('Expected exactly one handle (default)');
    }
    console.log('✅ TEST 1 PASSED\n');

    // Test 2: Open first training database
    console.log('🔌 TEST 2: Open first training database');
    const tempDir1 = path.join(tmpdir(), `jtag-test-db1-${Date.now()}`);
    tempDirs.push(tempDir1);
    await fs.mkdir(tempDir1, { recursive: true });
    const dbPath1 = path.join(tempDir1, 'training1.sqlite');

    const openResult1 = await Commands.execute<DataOpenResult>(DATA_COMMANDS.OPEN, {
      adapter: 'sqlite',
      config: {
        path: dbPath1,
        mode: 'create'
      }
    });

    console.log(`  Success: ${openResult1.success}`);
    console.log(`  Handle: ${openResult1.dbHandle}`);
    console.log(`  Adapter: ${openResult1.adapter}`);

    if (!openResult1.success || !openResult1.dbHandle) {
      throw new Error(`Failed to open database: ${openResult1.error}`);
    }
    const handle1 = openResult1.dbHandle;
    console.log('✅ TEST 2 PASSED\n');

    // Test 3: Open second training database
    console.log('🔌 TEST 3: Open second training database');
    const tempDir2 = path.join(tmpdir(), `jtag-test-db2-${Date.now()}`);
    tempDirs.push(tempDir2);
    await fs.mkdir(tempDir2, { recursive: true });
    const dbPath2 = path.join(tempDir2, 'training2.sqlite');

    const openResult2 = await Commands.execute<DataOpenResult>(DATA_COMMANDS.OPEN, {
      adapter: 'sqlite',
      config: {
        path: dbPath2,
        mode: 'create'
      }
    });

    if (!openResult2.success || !openResult2.dbHandle) {
      throw new Error(`Failed to open second database: ${openResult2.error}`);
    }
    const handle2 = openResult2.dbHandle;
    console.log(`  Handle: ${handle2}`);
    console.log('✅ TEST 3 PASSED\n');

    // Test 4: List handles (should show 3: default + 2 training)
    console.log('📋 TEST 4: List handles (should show 3 total)');
    const listResult2 = await Commands.execute<DataListHandlesResult>(DATA_COMMANDS.LIST_HANDLES, {});
    console.log(`  Total handles: ${listResult2.handles.length}`);
    console.log(`  Default: ${listResult2.handles.filter(h => h.isDefault).length}`);
    console.log(`  Training DBs: ${listResult2.handles.filter(h => !h.isDefault).length}`);
    
    listResult2.handles.forEach(h => {
      console.log(`    - ${h.handle} (${h.adapter}, default: ${h.isDefault})`);
    });

    if (listResult2.handles.length !== 3) {
      throw new Error(`Expected 3 handles, got ${listResult2.handles.length}`);
    }
    console.log('✅ TEST 4 PASSED\n');

    // Test 5: Close first training database
    console.log('🔌 TEST 5: Close first training database');
    const closeResult1 = await Commands.execute<DataCloseResult>(DATA_COMMANDS.CLOSE, {
      dbHandle: handle1
    });

    console.log(`  Success: ${closeResult1.success}`);
    if (!closeResult1.success) {
      throw new Error(`Failed to close database: ${closeResult1.error}`);
    }
    console.log('✅ TEST 5 PASSED\n');

    // Test 6: List handles (should show 2: default + 1 training)
    console.log('📋 TEST 6: List handles after closing first DB');
    const listResult3 = await Commands.execute<DataListHandlesResult>(DATA_COMMANDS.LIST_HANDLES, {});
    console.log(`  Total handles: ${listResult3.handles.length}`);
    
    if (listResult3.handles.length !== 2) {
      throw new Error(`Expected 2 handles, got ${listResult3.handles.length}`);
    }
    console.log('✅ TEST 6 PASSED\n');

    // Test 7: Try to close default handle (should fail)
    console.log('🔌 TEST 7: Try to close default handle (should fail)');
    const closeDefaultResult = await Commands.execute<DataCloseResult>(DATA_COMMANDS.CLOSE, {
      dbHandle: 'default'
    });

    console.log(`  Success: ${closeDefaultResult.success}`);
    console.log(`  Error: ${closeDefaultResult.error}`);
    
    if (closeDefaultResult.success) {
      throw new Error('Should not be able to close default handle');
    }
    console.log('✅ TEST 7 PASSED\n');

    // Test 8: Close second training database
    console.log('🔌 TEST 8: Close second training database');
    const closeResult2 = await Commands.execute<DataCloseResult>(DATA_COMMANDS.CLOSE, {
      dbHandle: handle2
    });

    if (!closeResult2.success) {
      throw new Error(`Failed to close second database: ${closeResult2.error}`);
    }
    console.log('✅ TEST 8 PASSED\n');

    // Test 9: Final handle list (should only show default)
    console.log('📋 TEST 9: Final handle list (back to default only)');
    const listResult4 = await Commands.execute<DataListHandlesResult>(DATA_COMMANDS.LIST_HANDLES, {});
    console.log(`  Total handles: ${listResult4.handles.length}`);
    
    if (listResult4.handles.length !== 1 || !listResult4.handles[0].isDefault) {
      throw new Error('Should only have default handle remaining');
    }
    console.log('✅ TEST 9 PASSED\n');

    console.log('🎉 ALL TESTS PASSED!');
    console.log('\n📊 SUMMARY:');
    console.log('  ✅ data/open command works');
    console.log('  ✅ data/close command works');
    console.log('  ✅ data/list-handles command works');
    console.log('  ✅ Multiple databases can be opened');
    console.log('  ✅ Default handle cannot be closed');
    console.log('  ✅ DatabaseHandleRegistry manages handles correctly');
    console.log('\n🚀 Multi-database system ready for training pipeline!');

  } catch (error) {
    console.error('\n❌ TEST FAILED:', error);
    throw error;
  } finally {
    // Remove temp directories
    for (const dir of tempDirs) {
      try {
        await fs.rm(dir, { recursive: true, force: true });
      } catch (error) {
        console.warn(`⚠️  Failed to clean up ${dir}:`, error);
      }
    }
  }
}

// Run tests
runTests()
  .then(() => {
    console.log('\n✅ Integration test completed successfully');
    process.exit(0);
  })
  .catch((error) => {
    console.error('\n❌ Integration test failed:', error);
    process.exit(1);
  });
