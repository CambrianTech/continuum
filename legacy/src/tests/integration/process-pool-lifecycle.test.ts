/**
 * ProcessPool Integration Tests
 *
 * Tests the complete lifecycle of genome inference worker processes:
 * - Process spawning (hot/warm/cold tiers)
 * - IPC communication
 * - Health monitoring
 * - Graceful termination
 * - Pool statistics
 *
 * Run with: ./jtag test tests/integration/process-pool-lifecycle.test.ts
 */

import * as path from 'path';
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { ProcessPool } from '../../system/genome/server/ProcessPool';

describe('ProcessPool Lifecycle Integration Tests', () => {
  let pool: ProcessPool;
  const workerPath = path.join(__dirname, '../../system/genome/server/inference-worker.ts');

  beforeEach(async () => {
    // Create fresh pool for each test
    pool = new ProcessPool(workerPath, {
      hotPoolSize: 2,
      warmPoolSize: 5,
      minProcesses: 1,
      maxProcesses: 5,
      healthCheckIntervalMs: 1000,
      maxIdleTimeMs: 5000,
      maxMemoryMB: 512,
      maxRequestsPerProcess: 10,
      maxErrorsBeforeTerminate: 3,
      processTimeoutMs: 5000,
    });
  });

  afterEach(async () => {
    // Clean up after each test
    if (pool) {
      await pool.shutdown();
    }
  });

  test('should initialize pool with minimum processes', async () => {
    await pool.initialize();

    const stats = pool.getStats();
    expect(stats.total).toBeGreaterThanOrEqual(1);
    expect(stats.byState.idle + stats.byState.ready).toBeGreaterThanOrEqual(1);
  }, 10000);

  test('should spawn hot tier process successfully', async () => {
    await pool.initialize();

    const process = await pool.spawnProcess('hot');

    expect(process).toBeTruthy();
    expect(process?.poolTier).toBe('hot');
    expect(process?.pid).toBeGreaterThan(0);
    expect(process?.state).toMatch(/idle|ready/);

    const stats = pool.getStats();
    expect(stats.byTier.hot).toBeGreaterThanOrEqual(1);
  }, 10000);

  test('should spawn warm tier process successfully', async () => {
    await pool.initialize();

    const process = await pool.spawnProcess('warm');

    expect(process).toBeTruthy();
    expect(process?.poolTier).toBe('warm');
    expect(process?.pid).toBeGreaterThan(0);

    const stats = pool.getStats();
    expect(stats.byTier.warm).toBeGreaterThanOrEqual(1);
  }, 10000);

  test('should spawn cold tier process successfully', async () => {
    await pool.initialize();

    const process = await pool.spawnProcess('cold');

    expect(process).toBeTruthy();
    expect(process?.poolTier).toBe('cold');
    expect(process?.pid).toBeGreaterThan(0);
  }, 10000);

  test('should terminate process gracefully', async () => {
    await pool.initialize();

    const process = await pool.spawnProcess('warm');
    expect(process).toBeTruthy();

    const processId = process!.id;
    const success = await pool.terminateProcess(processId, 'test-cleanup');

    expect(success).toBe(true);

    const stats = pool.getStats();
    expect(stats.total).toBeLessThan(2); // Should have fewer processes now
  }, 10000);

  test('should respect maximum process limit', async () => {
    await pool.initialize();

    // Try to spawn more processes than maxProcesses (5)
    const promises: Promise<any>[] = [];
    for (let i = 0; i < 10; i++) {
      promises.push(pool.spawnProcess('warm'));
    }

    const results = await Promise.all(promises);
    const successfulSpawns = results.filter(p => p !== null).length;

    const stats = pool.getStats();
    expect(stats.total).toBeLessThanOrEqual(5); // Max limit enforced
    expect(successfulSpawns).toBeLessThanOrEqual(5);
  }, 15000);

  test('should track process statistics correctly', async () => {
    await pool.initialize();

    await pool.spawnProcess('hot');
    await pool.spawnProcess('warm');

    const stats = pool.getStats();

    expect(stats.total).toBeGreaterThanOrEqual(2);
    expect(stats.byTier).toBeDefined();
    expect(stats.byState).toBeDefined();
    expect(stats.totalRequests).toBeDefined();
    expect(stats.totalErrors).toBeDefined();
  }, 10000);

  test('should shutdown all processes gracefully', async () => {
    await pool.initialize();

    await pool.spawnProcess('hot');
    await pool.spawnProcess('warm');
    await pool.spawnProcess('cold');

    const statsBefore = pool.getStats();
    expect(statsBefore.total).toBeGreaterThanOrEqual(3);

    await pool.shutdown();

    const statsAfter = pool.getStats();
    expect(statsAfter.total).toBe(0);
  }, 15000);

  test('should handle process crashes without affecting pool', async () => {
    await pool.initialize();

    const process = await pool.spawnProcess('warm');
    expect(process).toBeTruthy();

    // Force kill the process to simulate crash
    process!.process.kill('SIGKILL');

    // Wait for pool to detect and handle crash
    await new Promise(resolve => setTimeout(resolve, 2000));

    // Pool should still be functional
    const newProcess = await pool.spawnProcess('warm');
    expect(newProcess).toBeTruthy();
  }, 10000);

  test('should spawn process within timeout', async () => {
    const startTime = Date.now();
    await pool.initialize();

    const process = await pool.spawnProcess('warm');
    const spawnTime = Date.now() - startTime;

    expect(process).toBeTruthy();
    expect(spawnTime).toBeLessThan(5000); // Should spawn within 5 seconds
  }, 10000);

  test('should maintain minimum processes via health check', async () => {
    await pool.initialize();

    const initialStats = pool.getStats();
    expect(initialStats.total).toBeGreaterThanOrEqual(1);

    // Terminate all processes
    const processIds = Array.from((pool as any).processes.keys());
    for (const id of processIds) {
      await pool.terminateProcess(id, 'test');
    }

    // Wait for health check to spawn new process
    await new Promise(resolve => setTimeout(resolve, 2000));

    const finalStats = pool.getStats();
    expect(finalStats.total).toBeGreaterThanOrEqual(1); // Should maintain minimum
  }, 12000);

  test('should track process by tier correctly', async () => {
    await pool.initialize();

    await pool.spawnProcess('hot');
    await pool.spawnProcess('hot');
    await pool.spawnProcess('warm');

    const stats = pool.getStats();

    expect(stats.byTier.hot).toBeGreaterThanOrEqual(2);
    expect(stats.byTier.warm).toBeGreaterThanOrEqual(1);
  }, 10000);

  test('should track process states correctly', async () => {
    await pool.initialize();

    const stats = pool.getStats();

    // All processes should be in idle or ready state initially
    const healthyStates = stats.byState.idle + stats.byState.ready;
    expect(healthyStates).toBe(stats.total);
    expect(stats.byState.unhealthy).toBe(0);
  }, 10000);

  test('should emit process-spawned event', async () => {
    let spawnedEvent = false;
    pool.on('process-spawned', (process) => {
      spawnedEvent = true;
      expect(process).toBeDefined();
      expect(process.pid).toBeGreaterThan(0);
    });

    await pool.initialize();

    expect(spawnedEvent).toBe(true);
  }, 10000);

  test('should emit process-terminated event', async () => {
    await pool.initialize();

    const process = await pool.spawnProcess('warm');
    expect(process).toBeTruthy();

    let terminatedEvent = false;
    pool.on('process-terminated', (proc, reason) => {
      terminatedEvent = true;
      expect(reason).toBe('test-termination');
    });

    await pool.terminateProcess(process!.id, 'test-termination');

    expect(terminatedEvent).toBe(true);
  }, 10000);

  test('should handle concurrent spawns correctly', async () => {
    await pool.initialize();

    // Spawn multiple processes concurrently
    const promises = [
      pool.spawnProcess('hot'),
      pool.spawnProcess('warm'),
      pool.spawnProcess('cold'),
    ];

    const results = await Promise.all(promises);

    const successfulSpawns = results.filter(p => p !== null).length;
    expect(successfulSpawns).toBeGreaterThanOrEqual(2); // At least 2 should succeed
  }, 15000);

  test('should return null when max processes reached', async () => {
    await pool.initialize();

    // Fill up to max
    for (let i = 0; i < 5; i++) {
      await pool.spawnProcess('warm');
    }

    // This should fail (max is 5)
    const result = await pool.spawnProcess('warm');
    expect(result).toBeNull();
  }, 15000);
});

// Run tests
console.log('🧪 Starting ProcessPool Integration Tests...');
