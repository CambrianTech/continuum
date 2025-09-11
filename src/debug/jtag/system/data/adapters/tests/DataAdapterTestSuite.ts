#!/usr/bin/env npx tsx
/**
 * Comprehensive Data Adapter Test Suite
 * 
 * Tests all data adapters with strict Rust-like typing validation
 * Ensures professional data architecture works correctly
 */

import { SQLiteAdapter } from '../SQLiteAdapter';
import { JsonFileAdapter } from '../JsonFileAdapter';
import { HybridAdapter, type HybridAdapterConfig } from '../HybridAdapter';
import type { 
  BaseEntity,
  DataOperationContext,
  QueryOptions,
  ISOString
} from '../../domains/CoreTypes';
import type { DataAdapter } from '../../services/DataService';
import * as fs from 'fs/promises';
import * as path from 'path';

/**
 * Test Entity Type with Strict Typing
 */
interface TestEntity extends BaseEntity {
  readonly name: string;
  readonly email: string;
  readonly age: number;
  readonly active: boolean;
  readonly tags: readonly string[];
}

/**
 * Test Context with Rust-like Immutability
 */
const createTestContext = (): DataOperationContext => ({
  sessionId: 'test-session-123' as any,
  timestamp: new Date().toISOString() as ISOString,
  source: 'adapter-test'
});

/**
 * Test Data Factory
 */
const createTestEntity = (overrides: Partial<Omit<TestEntity, keyof BaseEntity>> = {}): Omit<TestEntity, keyof BaseEntity> => ({
  name: 'Test User',
  email: 'test@example.com',
  age: 30,
  active: true,
  tags: ['test', 'user'],
  ...overrides
});

/**
 * Adapter Test Results
 */
interface AdapterTestResult {
  readonly adapterName: string;
  readonly testsRun: number;
  readonly testsPassed: number;
  readonly testsFailed: number;
  readonly errors: readonly string[];
  readonly duration: number;
}

/**
 * Universal Adapter Test Runner
 */
class DataAdapterTestRunner {
  private results: AdapterTestResult[] = [];

  /**
   * Run comprehensive test suite on adapter
   */
  async testAdapter(adapter: DataAdapter, testName: string): Promise<AdapterTestResult> {
    const startTime = Date.now();
    const errors: string[] = [];
    let testsRun = 0;
    let testsPassed = 0;

    console.log(`\n🧪 Testing ${testName} (${adapter.name})...`);

    try {
      // Test 1: Initialization
      testsRun++;
      console.log(`  📋 Test 1: Adapter initialization`);
      const initResult = await adapter.initialize();
      if (!initResult.success) {
        errors.push(`Initialization failed: ${initResult.error.message}`);
      } else {
        testsPassed++;
        console.log(`    ✅ Initialization successful`);
      }

      // Test 2: Create Entity
      testsRun++;
      console.log(`  📋 Test 2: Create entity with strict typing`);
      const createData = createTestEntity({ name: 'Alice Smith', email: 'alice@example.com' });
      const createResult = await adapter.create<TestEntity>('users', createData, createTestContext());
      
      if (!createResult.success) {
        errors.push(`Create failed: ${createResult.error.message}`);
      } else {
        // Validate strict typing
        const entity = createResult.data;
        if (!entity.id || !entity.createdAt || !entity.updatedAt || entity.version !== 1) {
          errors.push(`Created entity missing BaseEntity properties`);
        } else if (entity.name !== 'Alice Smith' || entity.email !== 'alice@example.com') {
          errors.push(`Created entity data mismatch`);
        } else {
          testsPassed++;
          console.log(`    ✅ Entity created with ID: ${entity.id}`);
        }
      }

      let entityId = createResult.success ? createResult.data.id : 'test-id';

      // Test 3: Read Entity
      testsRun++;
      console.log(`  📋 Test 3: Read entity by ID`);
      const readResult = await adapter.read<TestEntity>('users', entityId, createTestContext());
      
      if (!readResult.success) {
        errors.push(`Read failed: ${readResult.error.message}`);
      } else if (!readResult.data) {
        errors.push(`Read returned null for existing entity`);
      } else {
        const entity = readResult.data;
        if (entity.id !== entityId || entity.name !== 'Alice Smith') {
          errors.push(`Read entity data mismatch`);
        } else {
          testsPassed++;
          console.log(`    ✅ Entity read successfully`);
        }
      }

      // Test 4: Update Entity
      testsRun++;
      console.log(`  📋 Test 4: Update entity with version increment`);
      const updateData: Partial<TestEntity> = { age: 35, active: false };
      const updateResult = await adapter.update<TestEntity>('users', entityId, updateData, createTestContext());
      
      if (!updateResult.success) {
        errors.push(`Update failed: ${updateResult.error.message}`);
      } else {
        const entity = updateResult.data;
        if (entity.age !== 35 || entity.active !== false || entity.version !== 2) {
          errors.push(`Updated entity validation failed`);
        } else {
          testsPassed++;
          console.log(`    ✅ Entity updated, version: ${entity.version}`);
        }
      }

      // Test 5: Create Multiple Entities for List Tests
      testsRun++;
      console.log(`  📋 Test 5: Create multiple entities for list testing`);
      const createPromises = [];
      for (let i = 0; i < 3; i++) {
        const data = createTestEntity({
          name: `User ${i}`,
          email: `user${i}@example.com`,
          age: 20 + i * 5
        });
        createPromises.push(adapter.create<TestEntity>('users', data, createTestContext()));
      }

      const createResults = await Promise.all(createPromises);
      const allCreated = createResults.every(r => r.success);
      
      if (!allCreated) {
        errors.push(`Multiple entity creation failed`);
      } else {
        testsPassed++;
        console.log(`    ✅ Multiple entities created successfully`);
      }

      // Test 6: List Entities with Pagination
      testsRun++;
      console.log(`  📋 Test 6: List entities with pagination and sorting`);
      const listOptions: QueryOptions<TestEntity> = {
        orderBy: [{ field: 'name', direction: 'ASC' }],
        limit: 2,
        offset: 0
      };
      
      const listResult = await adapter.list<TestEntity>('users', listOptions, createTestContext());
      
      if (!listResult.success) {
        errors.push(`List failed: ${listResult.error.message}`);
      } else {
        const entities = listResult.data;
        if (entities.length !== 2) {
          errors.push(`List pagination failed: expected 2, got ${entities.length}`);
        } else {
          testsPassed++;
          console.log(`    ✅ List with pagination returned ${entities.length} entities`);
        }
      }

      // Test 7: Query with Filters
      testsRun++;
      console.log(`  📋 Test 7: Query entities with filters`);
      const queryResult = await adapter.query<TestEntity>('users', { active: false }, {}, createTestContext());
      
      if (!queryResult.success) {
        errors.push(`Query failed: ${queryResult.error.message}`);
      } else {
        const entities = queryResult.data;
        const allInactive = entities.every(e => e.active === false);
        if (!allInactive) {
          errors.push(`Query filter failed: found active entities`);
        } else {
          testsPassed++;
          console.log(`    ✅ Query with filters returned ${entities.length} inactive entities`);
        }
      }

      // Test 8: Count Entities
      testsRun++;
      console.log(`  📋 Test 8: Count entities with filters`);
      const countResult = await adapter.count('users', { active: true }, createTestContext());
      
      if (!countResult.success) {
        errors.push(`Count failed: ${countResult.error.message}`);
      } else {
        const count = countResult.data;
        if (typeof count !== 'number' || count < 0) {
          errors.push(`Count returned invalid value: ${count}`);
        } else {
          testsPassed++;
          console.log(`    ✅ Count returned ${count} active entities`);
        }
      }

      // Test 9: Delete Entity
      testsRun++;
      console.log(`  📋 Test 9: Delete entity`);
      const deleteResult = await adapter.delete('users', entityId, createTestContext());
      
      if (!deleteResult.success) {
        errors.push(`Delete failed: ${deleteResult.error.message}`);
      } else {
        const deleted = deleteResult.data;
        if (!deleted) {
          errors.push(`Delete returned false for existing entity`);
        } else {
          testsPassed++;
          console.log(`    ✅ Entity deleted successfully`);
        }
      }

      // Test 10: Read Deleted Entity (Should Return Null)
      testsRun++;
      console.log(`  📋 Test 10: Verify deleted entity is gone`);
      const readDeletedResult = await adapter.read<TestEntity>('users', entityId, createTestContext());
      
      if (!readDeletedResult.success) {
        errors.push(`Read deleted entity check failed: ${readDeletedResult.error.message}`);
      } else if (readDeletedResult.data !== null) {
        errors.push(`Deleted entity still exists`);
      } else {
        testsPassed++;
        console.log(`    ✅ Deleted entity correctly returns null`);
      }

      // Test 11: Close Adapter
      testsRun++;
      console.log(`  📋 Test 11: Close adapter`);
      const closeResult = await adapter.close();
      
      if (!closeResult.success) {
        errors.push(`Close failed: ${closeResult.error.message}`);
      } else {
        testsPassed++;
        console.log(`    ✅ Adapter closed successfully`);
      }

    } catch (error: any) {
      errors.push(`Unexpected error: ${error.message}`);
    }

    const duration = Date.now() - startTime;
    const testsFailed = testsRun - testsPassed;
    
    const result: AdapterTestResult = {
      adapterName: testName,
      testsRun,
      testsPassed,
      testsFailed,
      errors,
      duration
    };

    this.results.push(result);

    // Print results
    console.log(`\n📊 ${testName} Test Results:`);
    console.log(`   Tests Run: ${testsRun}`);
    console.log(`   Passed: ${testsPassed}`);
    console.log(`   Failed: ${testsFailed}`);
    console.log(`   Duration: ${duration}ms`);
    
    if (errors.length > 0) {
      console.log(`   ❌ Errors:`);
      errors.forEach(error => console.log(`      - ${error}`));
    } else {
      console.log(`   ✅ All tests passed!`);
    }

    return result;
  }

  /**
   * Print comprehensive test summary
   */
  printSummary(): void {
    console.log(`\n🎉 DATA ADAPTER TEST SUMMARY`);
    console.log(`===========================`);
    
    let totalTests = 0;
    let totalPassed = 0;
    let totalFailed = 0;
    let totalDuration = 0;
    
    for (const result of this.results) {
      totalTests += result.testsRun;
      totalPassed += result.testsPassed;
      totalFailed += result.testsFailed;
      totalDuration += result.duration;
      
      const status = result.testsFailed === 0 ? '✅' : '❌';
      console.log(`${status} ${result.adapterName}: ${result.testsPassed}/${result.testsRun} passed (${result.duration}ms)`);
    }
    
    console.log(`\n📈 Overall Results:`);
    console.log(`   Total Tests: ${totalTests}`);
    console.log(`   Passed: ${totalPassed}`);
    console.log(`   Failed: ${totalFailed}`);
    console.log(`   Success Rate: ${((totalPassed / totalTests) * 100).toFixed(1)}%`);
    console.log(`   Total Duration: ${totalDuration}ms`);
    
    if (totalFailed === 0) {
      console.log(`\n🎉 ALL ADAPTER TESTS PASSED! Professional data architecture is working correctly.`);
    } else {
      console.log(`\n❌ Some tests failed. Check adapter implementations.`);
    }
  }
}

/**
 * Setup Test Environment
 */
async function setupTestEnvironment(): Promise<void> {
  // Clean up any existing test data
  const testDbPath = './.test-database';
  const testJsonPath = './.test-json-database';
  
  try {
    await fs.rm(testDbPath, { recursive: true, force: true });
    await fs.rm(testJsonPath, { recursive: true, force: true });
  } catch (error) {
    // Ignore cleanup errors
  }
}

/**
 * Main Test Runner
 */
async function runAdapterTests(): Promise<void> {
  console.log('🚀 STARTING COMPREHENSIVE DATA ADAPTER TESTS');
  console.log('============================================');
  console.log('Testing professional data architecture with Rust-like typing');
  
  await setupTestEnvironment();
  
  const testRunner = new DataAdapterTestRunner();

  try {
    // Test 1: SQLite Adapter
    const sqliteAdapter = new SQLiteAdapter('./.test-database/test.db');
    await testRunner.testAdapter(sqliteAdapter, 'SQLite Adapter');

    // Test 2: JSON File Adapter  
    const jsonAdapter = new JsonFileAdapter('./.test-json-database');
    await testRunner.testAdapter(jsonAdapter, 'JSON File Adapter');

    // Test 3: Hybrid Adapter (JSON + SQLite)
    const hybridConfig: HybridAdapterConfig = {
      readAdapters: [jsonAdapter],
      writeAdapter: sqliteAdapter,
      migration: {
        autoMigrate: true,
        migrateOnWrite: true
      }
    };
    const hybridAdapter = new HybridAdapter(hybridConfig);
    await testRunner.testAdapter(hybridAdapter, 'Hybrid Adapter');

  } catch (error: any) {
    console.error('❌ Test runner error:', error.message);
  }

  // Print final summary
  testRunner.printSummary();
}

// Run tests if this file is executed directly
if (require.main === module) {
  runAdapterTests().catch(console.error);
}

export { runAdapterTests, DataAdapterTestRunner };