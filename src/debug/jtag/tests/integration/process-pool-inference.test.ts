#!/usr/bin/env tsx
/**
 * ProcessPool Inference Integration Test
 * ========================================
 *
 * Tests the ACTUAL PRODUCTION INFERENCE FLOW:
 * ProcessPool.executeInference() -> IPC -> inference-worker.ts -> CandleAdapter -> Response
 *
 * This tests what production actually uses, not just lifecycle management.
 * Candle is the ONLY local inference path.
 */

import * as path from 'path';
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { ProcessPool } from '../../system/genome/server/ProcessPool';

describe('ProcessPool Inference Integration Tests', () => {
  let pool: ProcessPool;
  const workerPath = path.join(__dirname, '../../system/genome/server/inference-worker.ts');

  beforeEach(async () => {
    pool = new ProcessPool(workerPath, {
      hotPoolSize: 1,
      warmPoolSize: 2,
      minProcesses: 1,
      maxProcesses: 3,
      healthCheckIntervalMs: 5000,
      maxIdleTimeMs: 30000,
      maxMemoryMB: 1024,
      maxRequestsPerProcess: 100,
      maxErrorsBeforeTerminate: 5,
      processTimeoutMs: 60000, // 60 second timeout for inference
    });

    await pool.initialize();
    console.log('✅ ProcessPool initialized for inference testing');
  });

  afterEach(async () => {
    if (pool) {
      await pool.shutdown();
      console.log('✅ ProcessPool shutdown complete');
    }
  });

  test('should execute inference through Candle adapter', async () => {
    console.log('\n🧪 TEST: Execute inference through ProcessPool (Candle)');
    console.log('================================================');

    const startTime = Date.now();

    const result = await pool.executeInference({
      prompt: 'Say "Hello from ProcessPool test!" and nothing else.',
      provider: 'candle',
      model: 'llama3.2:1b',
      temperature: 0.7,
      maxTokens: 50,
    });

    const responseTime = Date.now() - startTime;

    console.log(`✅ Inference complete in ${responseTime}ms`);
    console.log(`📤 Result: ${result}`);

    expect(result).toBeDefined();
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
    expect(responseTime).toBeLessThan(30000); // Should complete within 30 seconds
  }, 60000); // 60 second test timeout

  test('should handle multiple concurrent inference requests', async () => {
    console.log('\n🧪 TEST: Multiple concurrent inference requests');
    console.log('===============================================');

    const prompts = [
      'Count to 3',
      'Name a color',
      'Say hi',
    ];

    const promises = prompts.map(prompt =>
      pool.executeInference({
        prompt,
        provider: 'candle',
        model: 'llama3.2:1b',
        temperature: 0.7,
        maxTokens: 20,
      })
    );

    const startTime = Date.now();
    const results = await Promise.all(promises);
    const totalTime = Date.now() - startTime;

    console.log(`✅ All ${results.length} inferences complete in ${totalTime}ms`);

    for (let i = 0; i < results.length; i++) {
      console.log(`   ${i + 1}. "${prompts[i]}" -> "${results[i].substring(0, 50)}..."`);
      expect(results[i]).toBeDefined();
      expect(typeof results[i]).toBe('string');
    }

    const stats = pool.getStats();
    console.log(`📊 Pool stats: ${stats.totalRequests} requests, ${stats.totalErrors} errors`);

    expect(stats.totalRequests).toBeGreaterThanOrEqual(3);
  }, 120000); // 2 minute test timeout

  test('should reuse idle processes for subsequent requests', async () => {
    console.log('\n🧪 TEST: Process reuse for sequential requests');
    console.log('==============================================');

    // First request
    const result1 = await pool.executeInference({
      prompt: 'Say "first"',
      provider: 'candle',
      model: 'llama3.2:1b',
      maxTokens: 10,
    });

    const statsAfterFirst = pool.getStats();
    const processesAfterFirst = statsAfterFirst.total;

    console.log(`✅ First request complete, ${processesAfterFirst} processes in pool`);

    // Second request - should reuse existing process
    const result2 = await pool.executeInference({
      prompt: 'Say "second"',
      provider: 'candle',
      model: 'llama3.2:1b',
      maxTokens: 10,
    });

    const statsAfterSecond = pool.getStats();
    const processesAfterSecond = statsAfterSecond.total;

    console.log(`✅ Second request complete, ${processesAfterSecond} processes in pool`);
    console.log(`📊 Requests: ${statsAfterSecond.totalRequests}`);

    expect(result1).toBeDefined();
    expect(result2).toBeDefined();
    expect(processesAfterSecond).toBeLessThanOrEqual(processesAfterFirst + 1); // Should not spawn many new processes
    expect(statsAfterSecond.totalRequests).toBe(2);
  }, 120000);

  test('should handle inference errors gracefully', async () => {
    console.log('\n🧪 TEST: Error handling in inference');
    console.log('====================================');

    try {
      await pool.executeInference({
        prompt: 'test',
        provider: 'nonexistent-provider',
        model: 'fake-model',
        maxTokens: 10,
      });

      // Should not reach here
      expect(true).toBe(false);
    } catch (error) {
      console.log(`✅ Error caught as expected: ${error instanceof Error ? error.message : String(error)}`);
      expect(error).toBeDefined();
    }

    // Pool should still be functional after error
    const result = await pool.executeInference({
      prompt: 'Say "recovered"',
      provider: 'candle',
      model: 'llama3.2:1b',
      maxTokens: 10,
    });

    console.log('✅ Pool still functional after error');
    expect(result).toBeDefined();
  }, 120000);
});

// Run if executed directly
if (require.main === module) {
  console.log('🧬 PROCESSPOOL INFERENCE INTEGRATION TESTS');
  console.log('==========================================\n');
}
