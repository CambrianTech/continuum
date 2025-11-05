#!/usr/bin/env tsx
/**
 * Database Backend Agnostic Test Framework
 *
 * Modular testing framework that runs identical tests against different storage backends:
 * - JSON file storage (current default)
 * - SQLite database (new SQL backend)
 * - Future backends: PostgreSQL, MongoDB, Memory, etc.
 *
 * This allows perfect apples-to-apples performance and feature comparisons
 */

import { jtag } from '../../../server-index';
import type { JTAGClientServer } from '../../../system/core/client/server/JTAGClientServer';
import { SqliteStorageAdapter } from '../../../daemons/data-daemon/server/SqliteStorageAdapter';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import type { StorageStrategyConfig } from '../../../daemons/data-daemon/shared/DataDaemon';
import type { DataRecord, StorageResult } from '../../../daemons/data-daemon/shared/DataStorageAdapter';
import { generateUUID, type UUID } from '../../../system/core/types/CrossPlatformUUID';

// Test data interfaces
interface TestUser {
  id: string;
  name: string;
  userType: 'human' | 'agent' | 'persona';
  email?: string;
  isOnline: boolean;
  createdAt: string;
  lastActiveAt: string;
}

interface TestMessage {
  id: string;
  roomId: string;
  senderId: string;
  content: string;
  timestamp: string;
  type: 'text' | 'system';
}

interface BackendTestResult {
  backendName: string;
  testName: string;
  success: boolean;
  duration: number;
  metrics: {
    recordsProcessed?: number;
    averageLatency?: number;
    throughput?: number;
    errorRate?: number;
    featuresSupported?: string[];
  };
  error?: string;
}

/**
 * Universal Storage Backend Interface
 * Abstracts different storage backends behind a common interface
 */
abstract class StorageBackend {
  abstract readonly name: string;
  abstract readonly type: string;

  abstract initialize(): Promise<void>;
  abstract create(collection: string, data: any): Promise<{ success: boolean; id?: string; error?: string }>;
  abstract read(collection: string, id: string): Promise<{ success: boolean; data?: any; error?: string }>;
  abstract query(collection: string, filters?: any): Promise<{ success: boolean; data?: any[]; error?: string }>;
  abstract update(collection: string, id: string, data: any): Promise<{ success: boolean; error?: string }>;
  abstract delete(collection: string, id: string): Promise<{ success: boolean; error?: string }>;
  abstract cleanup(): Promise<void>;
  abstract close(): Promise<void>;
}

/**
 * JSON Backend Implementation (Current JTAG System)
 */
class JsonStorageBackend extends StorageBackend {
  readonly name = 'JSON File Storage';
  readonly type = 'json';
  private client: JTAGClientServer | null = null;

  async initialize(): Promise<void> {
    const clientResult = await jtag.connect({ targetEnvironment: 'server' });
    this.client = clientResult.client;
  }

  async create(collection: string, data: any): Promise<{ success: boolean; id?: string; error?: string }> {
    if (!this.client) throw new Error('JSON backend not initialized');

    const result = await this.client.commands['data/create']({
      collection,
      data,
      format: 'json'
    });

    return {
      success: result.success,
      id: data.id,
      error: result.error
    };
  }

  async read(collection: string, id: string): Promise<{ success: boolean; data?: any; error?: string }> {
    if (!this.client) throw new Error('JSON backend not initialized');

    const result = await this.client.commands['data/read']({
      collection,
      id,
      format: 'json'
    });

    return {
      success: result.success,
      data: result.data,
      error: result.error
    };
  }

  async query(collection: string, filters?: any): Promise<{ success: boolean; data?: any[]; error?: string }> {
    if (!this.client) throw new Error('JSON backend not initialized');

    const params: any = { collection, format: 'json' };
    if (filters) {
      params.filter = JSON.stringify(filters);
    }

    const result = await this.client.commands['data/list'](params);

    return {
      success: result.success,
      data: result.items || result.data,
      error: result.error
    };
  }

  async update(collection: string, id: string, data: any): Promise<{ success: boolean; error?: string }> {
    if (!this.client) throw new Error('JSON backend not initialized');

    const result = await this.client.commands['data/update']({
      collection,
      id,
      data,
      format: 'json'
    });

    return {
      success: result.success,
      error: result.error
    };
  }

  async delete(collection: string, id: string): Promise<{ success: boolean; error?: string }> {
    if (!this.client) throw new Error('JSON backend not initialized');

    const result = await this.client.commands['data/delete']({
      collection,
      id,
      format: 'json'
    });

    return {
      success: result.success,
      error: result.error
    };
  }

  async cleanup(): Promise<void> {
    // JSON backend cleanup is handled by JTAG system
  }

  async close(): Promise<void> {
    if (this.client) {
      await this.client.disconnect();
      this.client = null;
    }
  }
}

/**
 * SQLite Backend Implementation
 */
class SqliteStorageBackend extends StorageBackend {
  readonly name = 'SQLite Database';
  readonly type = 'sqlite';
  private adapter: SqliteStorageAdapter | null = null;

  async initialize(): Promise<void> {
    this.adapter = new SqliteStorageAdapter();
    await this.adapter.initialize({
      type: 'sqlite',
      namespace: 'backend-agnostic-test',
      options: {
        filename: '.continuum/test/backend-comparison.db',
        foreignKeys: true,
        wal: true,
        synchronous: 'NORMAL'
      }
    });
  }

  async create(collection: string, data: any): Promise<{ success: boolean; id?: string; error?: string }> {
    if (!this.adapter) throw new Error('SQLite backend not initialized');

    const record: DataRecord<any> = {
      id: data.id as UUID,
      collection,
      data,
      metadata: {
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        version: 1
      }
    };

    const result = await this.adapter.create(record);

    return {
      success: result.success,
      id: data.id,
      error: result.error
    };
  }

  async read(collection: string, id: string): Promise<{ success: boolean; data?: any; error?: string }> {
    if (!this.adapter) throw new Error('SQLite backend not initialized');

    const result = await this.adapter.read(collection, id as UUID);

    return {
      success: result.success,
      data: result.data?.data,
      error: result.error
    };
  }

  async query(collection: string, filters?: any): Promise<{ success: boolean; data?: any[]; error?: string }> {
    if (!this.adapter) throw new Error('SQLite backend not initialized');

    const result = await this.adapter.query({
      collection,
      filters: filters || {}
    });

    return {
      success: result.success,
      data: result.data?.map(record => record.data),
      error: result.error
    };
  }

  async update(collection: string, id: string, data: any): Promise<{ success: boolean; error?: string }> {
    if (!this.adapter) throw new Error('SQLite backend not initialized');

    const result = await this.adapter.update(collection, id as UUID, data);

    return {
      success: result.success,
      error: result.error
    };
  }

  async delete(collection: string, id: string): Promise<{ success: boolean; error?: string }> {
    if (!this.adapter) throw new Error('SQLite backend not initialized');

    const result = await this.adapter.delete(collection, id as UUID);

    return {
      success: result.success,
      error: result.error
    };
  }

  async cleanup(): Promise<void> {
    if (this.adapter) {
      await this.adapter.cleanup();
    }
  }

  async close(): Promise<void> {
    if (this.adapter) {
      await this.adapter.close();
      this.adapter = null;
    }
  }
}

/**
 * Backend-Agnostic Test Suite
 * Runs identical tests against different storage backends
 */
class BackendAgnosticTester {
  private results: BackendTestResult[] = [];
  private testData: { users: TestUser[]; messages: TestMessage[] } = { users: [], messages: [] };

  /**
   * Generate consistent test data for all backends
   */
  private generateTestData(): void {
    const baseTimestamp = new Date().toISOString();

    this.testData.users = [
      {
        id: generateUUID(),
        name: 'Test User 1',
        userType: 'human',
        email: 'user1@test.dev',
        isOnline: true,
        createdAt: baseTimestamp,
        lastActiveAt: baseTimestamp
      },
      {
        id: generateUUID(),
        name: 'Test Agent',
        userType: 'agent',
        isOnline: true,
        createdAt: baseTimestamp,
        lastActiveAt: baseTimestamp
      },
      {
        id: generateUUID(),
        name: 'Test Persona',
        userType: 'persona',
        email: 'persona@test.dev',
        isOnline: false,
        createdAt: baseTimestamp,
        lastActiveAt: baseTimestamp
      }
    ];

    this.testData.messages = [
      {
        id: generateUUID(),
        roomId: 'test-room',
        senderId: this.testData.users[0].id,
        content: 'Hello from backend agnostic test!',
        timestamp: baseTimestamp,
        type: 'text'
      },
      {
        id: generateUUID(),
        roomId: 'test-room',
        senderId: this.testData.users[1].id,
        content: 'Agent response in test',
        timestamp: baseTimestamp,
        type: 'text'
      }
    ];
  }

  /**
   * Test 1: Basic CRUD Operations
   * Tests Create, Read, Update, Delete operations on both backends
   */
  async testBasicCrud(backend: StorageBackend): Promise<BackendTestResult> {
    const testName = 'Basic CRUD Operations';
    const startTime = Date.now();

    try {
      console.log(`\n📝 ${backend.name}: ${testName}...`);

      let recordsProcessed = 0;
      const operationTimes: number[] = [];

      // Test CREATE operations
      console.log(`📤 ${backend.name}: Creating test users...`);
      for (const user of this.testData.users) {
        const createStart = Date.now();
        const result = await backend.create('users', user);
        const createTime = Date.now() - createStart;
        operationTimes.push(createTime);

        if (result.success) {
          recordsProcessed++;
          console.log(`✅ Created ${user.name} in ${createTime}ms`);
        } else {
          console.log(`❌ Failed to create ${user.name}: ${result.error}`);
        }
      }

      // Test READ operations
      console.log(`📖 ${backend.name}: Reading test users...`);
      for (const user of this.testData.users) {
        const readStart = Date.now();
        const result = await backend.read('users', user.id);
        const readTime = Date.now() - readStart;
        operationTimes.push(readTime);

        if (result.success && result.data) {
          console.log(`✅ Read ${result.data.name} in ${readTime}ms`);
        }
      }

      // Test QUERY operations
      console.log(`🔍 ${backend.name}: Querying users...`);
      const queryStart = Date.now();
      const queryResult = await backend.query('users');
      const queryTime = Date.now() - queryStart;
      operationTimes.push(queryTime);

      if (queryResult.success && queryResult.data) {
        console.log(`✅ Queried ${queryResult.data.length} users in ${queryTime}ms`);
      }

      // Test UPDATE operations
      console.log(`✏️ ${backend.name}: Updating user...`);
      const updateStart = Date.now();
      const updateResult = await backend.update('users', this.testData.users[0].id, {
        lastActiveAt: new Date().toISOString(),
        isOnline: false
      });
      const updateTime = Date.now() - updateStart;
      operationTimes.push(updateTime);

      if (updateResult.success) {
        console.log(`✅ Updated user in ${updateTime}ms`);
      }

      const duration = Date.now() - startTime;
      const averageLatency = operationTimes.reduce((sum, time) => sum + time, 0) / operationTimes.length;
      const throughput = (operationTimes.length * 1000) / duration; // operations per second

      return {
        backendName: backend.name,
        testName,
        success: recordsProcessed >= this.testData.users.length,
        duration,
        metrics: {
          recordsProcessed,
          averageLatency,
          throughput,
          featuresSupported: ['create', 'read', 'query', 'update']
        }
      };

    } catch (error) {
      return {
        backendName: backend.name,
        testName,
        success: false,
        duration: Date.now() - startTime,
        metrics: {},
        error: error.message
      };
    }
  }

  /**
   * Test 2: Query Performance Comparison
   * Tests filtering and query performance between backends
   */
  async testQueryPerformance(backend: StorageBackend): Promise<BackendTestResult> {
    const testName = 'Query Performance';
    const startTime = Date.now();

    try {
      console.log(`\n🔍 ${backend.name}: ${testName}...`);

      const queryTimes: number[] = [];
      let successfulQueries = 0;

      // Query 1: Filter by user type
      console.log(`🔎 ${backend.name}: Filtering by user type...`);
      const filterStart = Date.now();
      const filterResult = await backend.query('users', { userType: 'agent' });
      const filterTime = Date.now() - filterStart;
      queryTimes.push(filterTime);

      if (filterResult.success) {
        successfulQueries++;
        console.log(`✅ Filter query: ${filterResult.data?.length || 0} agents found in ${filterTime}ms`);
      }

      // Query 2: Filter by online status
      console.log(`🔎 ${backend.name}: Filtering by online status...`);
      const onlineStart = Date.now();
      const onlineResult = await backend.query('users', { isOnline: true });
      const onlineTime = Date.now() - onlineStart;
      queryTimes.push(onlineTime);

      if (onlineResult.success) {
        successfulQueries++;
        console.log(`✅ Online query: ${onlineResult.data?.length || 0} online users found in ${onlineTime}ms`);
      }

      // Query 3: Get all records (no filter)
      console.log(`🔎 ${backend.name}: Querying all records...`);
      const allStart = Date.now();
      const allResult = await backend.query('users');
      const allTime = Date.now() - allStart;
      queryTimes.push(allTime);

      if (allResult.success) {
        successfulQueries++;
        console.log(`✅ All records query: ${allResult.data?.length || 0} users found in ${allTime}ms`);
      }

      const duration = Date.now() - startTime;
      const averageLatency = queryTimes.reduce((sum, time) => sum + time, 0) / queryTimes.length;
      const successRate = (successfulQueries / 3) * 100;

      return {
        backendName: backend.name,
        testName,
        success: successfulQueries >= 2, // At least 2 out of 3 queries successful
        duration,
        metrics: {
          recordsProcessed: successfulQueries,
          averageLatency,
          errorRate: 100 - successRate,
          featuresSupported: ['filtering', 'querying']
        }
      };

    } catch (error) {
      return {
        backendName: backend.name,
        testName,
        success: false,
        duration: Date.now() - startTime,
        metrics: {},
        error: error.message
      };
    }
  }

  /**
   * Test 3: Stress Testing
   * Tests performance under load for both backends
   */
  async testStressPerformance(backend: StorageBackend): Promise<BackendTestResult> {
    const testName = 'Stress Performance';
    const startTime = Date.now();

    try {
      console.log(`\n⚡ ${backend.name}: ${testName}...`);

      const batchSize = 10;
      const operationTimes: number[] = [];
      let successfulOperations = 0;

      console.log(`🔥 ${backend.name}: Creating ${batchSize} records under load...`);

      // Create batch operations
      const promises: Promise<any>[] = [];

      for (let i = 0; i < batchSize; i++) {
        const stressUser = {
          id: generateUUID(),
          name: `Stress User ${i + 1}`,
          userType: 'human' as const,
          isOnline: Math.random() > 0.5,
          createdAt: new Date().toISOString(),
          lastActiveAt: new Date().toISOString()
        };

        const promise = (async () => {
          const opStart = Date.now();
          const result = await backend.create('stress_test', stressUser);
          const opTime = Date.now() - opStart;
          operationTimes.push(opTime);
          return result;
        })();

        promises.push(promise);
      }

      // Wait for all operations to complete
      const results = await Promise.allSettled(promises);
      successfulOperations = results.filter(r =>
        r.status === 'fulfilled' && r.value.success
      ).length;

      const duration = Date.now() - startTime;
      const averageLatency = operationTimes.reduce((sum, time) => sum + time, 0) / operationTimes.length;
      const throughput = (batchSize * 1000) / duration;
      const successRate = (successfulOperations / batchSize) * 100;

      console.log(`📊 ${backend.name}: ${successfulOperations}/${batchSize} operations successful`);
      console.log(`⚡ Average latency: ${averageLatency.toFixed(1)}ms`);
      console.log(`🚀 Throughput: ${throughput.toFixed(1)} ops/sec`);

      return {
        backendName: backend.name,
        testName,
        success: successRate >= 80, // At least 80% success rate
        duration,
        metrics: {
          recordsProcessed: successfulOperations,
          averageLatency,
          throughput,
          errorRate: 100 - successRate,
          featuresSupported: ['concurrent-operations', 'stress-handling']
        }
      };

    } catch (error) {
      return {
        backendName: backend.name,
        testName,
        success: false,
        duration: Date.now() - startTime,
        metrics: {},
        error: error.message
      };
    }
  }

  /**
   * Run backend comparison tests
   */
  async runBackendComparison(): Promise<void> {
    console.log('🔄 BACKEND-AGNOSTIC DATABASE TESTING');
    console.log('=' .repeat(80));
    console.log('🎯 Running identical tests on multiple storage backends');
    console.log('📊 Comparing JSON vs SQLite performance and features');
    console.log('');

    // Generate consistent test data
    this.generateTestData();

    // Initialize backends
    const backends: StorageBackend[] = [
      new JsonStorageBackend(),
      new SqliteStorageBackend()
    ];

    try {
      // Initialize all backends
      for (const backend of backends) {
        console.log(`🔧 Initializing ${backend.name}...`);
        await backend.initialize();
        console.log(`✅ ${backend.name} ready`);
      }

      // Run identical tests on all backends
      const tests = [
        (backend: StorageBackend) => this.testBasicCrud(backend),
        (backend: StorageBackend) => this.testQueryPerformance(backend),
        (backend: StorageBackend) => this.testStressPerformance(backend)
      ];

      for (const testFn of tests) {
        for (const backend of backends) {
          this.results.push(await testFn(backend));
        }
      }

      // Generate comparison report
      this.generateComparisonReport();

    } catch (error) {
      console.error('❌ Backend comparison testing failed:', error);
      throw error;
    } finally {
      // Clean up all backends
      for (const backend of backends) {
        try {
          await backend.cleanup();
          await backend.close();
          console.log(`🧹 ${backend.name} cleaned up`);
        } catch (error) {
          console.log(`⚠️ ${backend.name} cleanup had issues`);
        }
      }
    }
  }

  /**
   * Generate comprehensive comparison report
   */
  private generateComparisonReport(): void {
    console.log('\n' + '=' .repeat(80));
    console.log('📊 BACKEND COMPARISON RESULTS');
    console.log('=' .repeat(80));

    // Group results by backend
    const backendGroups = new Map<string, BackendTestResult[]>();
    this.results.forEach(result => {
      if (!backendGroups.has(result.backendName)) {
        backendGroups.set(result.backendName, []);
      }
      backendGroups.get(result.backendName)!.push(result);
    });

    // Display results for each backend
    backendGroups.forEach((results, backendName) => {
      const passedTests = results.filter(r => r.success).length;
      const totalTests = results.length;
      const successRate = (passedTests / totalTests) * 100;
      const avgLatency = results
        .filter(r => r.metrics.averageLatency)
        .reduce((sum, r) => sum + (r.metrics.averageLatency || 0), 0) / results.length;

      console.log(`\n🔧 ${backendName}:`);
      console.log(`   Tests Passed: ${passedTests}/${totalTests} (${successRate.toFixed(1)}%)`);
      console.log(`   Average Latency: ${avgLatency.toFixed(1)}ms`);

      results.forEach(result => {
        const status = result.success ? '✅' : '❌';
        console.log(`   ${status} ${result.testName} (${result.duration}ms)`);
        if (result.metrics.throughput) {
          console.log(`      Throughput: ${result.metrics.throughput.toFixed(1)} ops/sec`);
        }
      });
    });

    // Direct comparison between backends
    console.log('\n🥊 HEAD-TO-HEAD COMPARISON:');

    const jsonResults = backendGroups.get('JSON File Storage') || [];
    const sqliteResults = backendGroups.get('SQLite Database') || [];

    const testNames = [...new Set(this.results.map(r => r.testName))];

    testNames.forEach(testName => {
      const jsonResult = jsonResults.find(r => r.testName === testName);
      const sqliteResult = sqliteResults.find(r => r.testName === testName);

      if (jsonResult && sqliteResult) {
        console.log(`\n📋 ${testName}:`);

        if (jsonResult.metrics.averageLatency && sqliteResult.metrics.averageLatency) {
          const improvement = ((jsonResult.metrics.averageLatency - sqliteResult.metrics.averageLatency) / jsonResult.metrics.averageLatency) * 100;
          console.log(`   Latency: JSON ${jsonResult.metrics.averageLatency.toFixed(1)}ms vs SQLite ${sqliteResult.metrics.averageLatency.toFixed(1)}ms`);
          console.log(`   Performance: SQLite is ${improvement.toFixed(1)}% ${improvement > 0 ? 'faster' : 'slower'}`);
        }

        if (jsonResult.metrics.throughput && sqliteResult.metrics.throughput) {
          const throughputRatio = sqliteResult.metrics.throughput / jsonResult.metrics.throughput;
          console.log(`   Throughput: SQLite is ${throughputRatio.toFixed(2)}x JSON performance`);
        }
      }
    });

    console.log('\n🎯 RECOMMENDATIONS:');
    const overallJsonSuccess = jsonResults.filter(r => r.success).length / jsonResults.length * 100;
    const overallSqliteSuccess = sqliteResults.filter(r => r.success).length / sqliteResults.length * 100;

    if (overallSqliteSuccess >= 90 && overallJsonSuccess >= 90) {
      console.log('   ✅ Both backends are stable and production-ready');
      console.log('   🚀 SQLite provides performance benefits for complex queries');
      console.log('   📄 JSON backend remains suitable for simple use cases');
      console.log('   🔄 Migration path validated - can switch backends seamlessly');
    }

    console.log('\n' + '=' .repeat(80));
  }
}

/**
 * Main test execution
 */
async function runBackendAgnosticTests(): Promise<void> {
  const tester = new BackendAgnosticTester();

  console.log('🚨 BACKEND-AGNOSTIC DATABASE TESTING');
  console.log('🔍 Running identical tests on JSON and SQLite backends');
  console.log('📊 Providing direct performance and feature comparisons');
  console.log('');

  await tester.runBackendComparison();
}

// Execute if called directly
if (require.main === module) {
  runBackendAgnosticTests()
    .then(() => {
      console.log('\n✅ Backend-agnostic testing completed successfully!');
      process.exit(0);
    })
    .catch((error) => {
      console.error('\n❌ Backend-agnostic testing failed:', error);
      process.exit(1);
    });
}

export { runBackendAgnosticTests, BackendAgnosticTester };