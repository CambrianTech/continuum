/**
 * ORM Concurrent Stress Test
 *
 * Simulates the real-world load pattern that causes cascading timeouts:
 * 15 personas all firing data/create + data/list + data/query simultaneously.
 *
 * This reproduces the head-of-line blocking caused by:
 * 1. SQLite single-writer bottleneck (all creates serialized)
 * 2. IPC pool exhaustion (12 connections for 15+ concurrent callers)
 * 3. Query semaphore starvation (was 4, now 16)
 *
 * Run: npx vitest tests/integration/orm-concurrent-stress.test.ts
 * Requires: npm start (system must be running)
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { ORMRustClient } from '../../daemons/data-daemon/server/ORMRustClient';
import type { BaseEntity } from '../../system/data/entities/BaseEntity';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

interface StressEntity extends BaseEntity {
  personaId: string;
  content: string;
  category: string;
  priority: number;
  createdAt: string;
}

/** Simulates one persona's typical data burst during chat processing */
async function simulatePersonaBurst(
  client: ORMRustClient,
  personaIndex: number,
  collection: string
): Promise<{ creates: number; queries: number; errors: string[]; elapsed: number }> {
  const start = Date.now();
  const errors: string[] = [];
  let creates = 0;
  let queries = 0;
  const personaId = `persona-${personaIndex}`;
  const ids: string[] = [];

  // Each persona does: 2 creates + 2 queries + 1 count (typical RAG cycle)
  const operations = [
    // Create a chat message
    async () => {
      const id = generateUUID();
      ids.push(id);
      const result = await client.store<StressEntity>(collection, {
        id,
        personaId,
        content: `Message from persona ${personaIndex} at ${Date.now()}`,
        category: 'chat',
        priority: Math.random(),
        createdAt: new Date().toISOString(),
      });
      if (result.success) creates++;
      else errors.push(`create1: ${result.error}`);
    },
    // Create a RAG context entry
    async () => {
      const id = generateUUID();
      ids.push(id);
      const result = await client.store<StressEntity>(collection, {
        id,
        personaId,
        content: `RAG context for persona ${personaIndex}`,
        category: 'rag',
        priority: 0.5,
        createdAt: new Date().toISOString(),
      });
      if (result.success) creates++;
      else errors.push(`create2: ${result.error}`);
    },
    // Query recent messages (typical chat history fetch)
    async () => {
      const result = await client.query<StressEntity>({
        collection,
        filter: { personaId },
        limit: 20,
        orderBy: [{ field: 'createdAt', direction: 'desc' }],
      });
      if (result.success) queries++;
      else errors.push(`query1: ${result.error}`);
    },
    // Query by category (typical RAG lookup)
    async () => {
      const result = await client.query<StressEntity>({
        collection,
        filter: { category: 'chat' },
        limit: 10,
      });
      if (result.success) queries++;
      else errors.push(`query2: ${result.error}`);
    },
    // Count operation
    async () => {
      const result = await client.count({
        collection,
        filter: { personaId },
      });
      if (result.success) queries++;
      else errors.push(`count: ${result.error}`);
    },
  ];

  // Fire all operations concurrently (this is what personas actually do)
  await Promise.allSettled(operations.map(op => op()));

  return { creates, queries, errors, elapsed: Date.now() - start };
}

describe('ORM Concurrent Stress', () => {
  let client: ORMRustClient;
  const collection = `stress_test_${Date.now()}`;

  beforeAll(async () => {
    client = ORMRustClient.getInstance();
    await new Promise(resolve => setTimeout(resolve, 500));
  });

  afterAll(async () => {
    // Clean up the entire test collection
    try {
      const all = await client.query<StressEntity>({
        collection,
        limit: 1000,
      });
      if (all.success && all.data) {
        for (const record of all.data) {
          await client.remove(collection, record.data?.id || record.id);
        }
      }
    } catch {
      // Best-effort cleanup
    }
    client.disconnect();
  });

  it('should handle 5 concurrent persona bursts without errors', async () => {
    const PERSONA_COUNT = 5;
    const start = Date.now();

    const results = await Promise.all(
      Array.from({ length: PERSONA_COUNT }, (_, i) =>
        simulatePersonaBurst(client, i, collection)
      )
    );

    const totalElapsed = Date.now() - start;
    const totalCreates = results.reduce((s, r) => s + r.creates, 0);
    const totalQueries = results.reduce((s, r) => s + r.queries, 0);
    const allErrors = results.flatMap(r => r.errors);
    const maxElapsed = Math.max(...results.map(r => r.elapsed));

    console.log(`\n--- 5 Concurrent Personas ---`);
    console.log(`Total: ${totalCreates} creates, ${totalQueries} queries in ${totalElapsed}ms`);
    console.log(`Max persona time: ${maxElapsed}ms`);
    console.log(`Errors: ${allErrors.length}`);
    if (allErrors.length > 0) console.log(`  ${allErrors.join('\n  ')}`);

    expect(allErrors).toHaveLength(0);
    expect(totalCreates).toBe(PERSONA_COUNT * 2);
    expect(totalQueries).toBe(PERSONA_COUNT * 3);
  }, 30_000);

  it('should handle 15 concurrent persona bursts (production load)', async () => {
    const PERSONA_COUNT = 15;
    const start = Date.now();

    const results = await Promise.all(
      Array.from({ length: PERSONA_COUNT }, (_, i) =>
        simulatePersonaBurst(client, i + 100, collection)
      )
    );

    const totalElapsed = Date.now() - start;
    const totalCreates = results.reduce((s, r) => s + r.creates, 0);
    const totalQueries = results.reduce((s, r) => s + r.queries, 0);
    const allErrors = results.flatMap(r => r.errors);
    const maxElapsed = Math.max(...results.map(r => r.elapsed));
    const p95Elapsed = results
      .map(r => r.elapsed)
      .sort((a, b) => a - b)[Math.floor(PERSONA_COUNT * 0.95)];

    console.log(`\n--- 15 Concurrent Personas (Production Load) ---`);
    console.log(`Total: ${totalCreates} creates, ${totalQueries} queries in ${totalElapsed}ms`);
    console.log(`Max persona time: ${maxElapsed}ms, P95: ${p95Elapsed}ms`);
    console.log(`Errors: ${allErrors.length}`);
    if (allErrors.length > 0) console.log(`  ${allErrors.join('\n  ')}`);

    // With the fix (semaphore 16, pool 20), this should complete without errors.
    // Before the fix (semaphore 4, pool 12), this would cascade-timeout.
    expect(allErrors).toHaveLength(0);
    expect(totalCreates).toBe(PERSONA_COUNT * 2);
    expect(totalQueries).toBe(PERSONA_COUNT * 3);

    // P95 latency should be under 10 seconds even under full load
    expect(p95Elapsed).toBeLessThan(10_000);
  }, 60_000);

  it('should handle sustained write pressure (30 rapid creates)', async () => {
    // Simulates the worst case: everyone creates at the same instant
    // All 30 creates funnel through the single SQLite writer thread
    const WRITE_COUNT = 30;
    const start = Date.now();
    const errors: string[] = [];
    const ids: string[] = [];

    const promises = Array.from({ length: WRITE_COUNT }, async (_, i) => {
      const id = generateUUID();
      ids.push(id);
      const result = await client.store<StressEntity>(collection, {
        id,
        personaId: `writer-${i}`,
        content: `Burst write ${i}`,
        category: 'burst',
        priority: i / WRITE_COUNT,
        createdAt: new Date().toISOString(),
      });
      if (!result.success) errors.push(`write-${i}: ${result.error}`);
      return result;
    });

    const results = await Promise.allSettled(promises);
    const elapsed = Date.now() - start;
    const fulfilled = results.filter(r => r.status === 'fulfilled').length;

    console.log(`\n--- 30 Concurrent Writes ---`);
    console.log(`Completed: ${fulfilled}/${WRITE_COUNT} in ${elapsed}ms`);
    console.log(`Avg: ${(elapsed / WRITE_COUNT).toFixed(1)}ms per write`);
    console.log(`Errors: ${errors.length}`);
    if (errors.length > 0) console.log(`  ${errors.slice(0, 5).join('\n  ')}`);

    expect(errors).toHaveLength(0);
    expect(fulfilled).toBe(WRITE_COUNT);

    // 30 serialized writes through single writer should complete in under 15s
    expect(elapsed).toBeLessThan(15_000);
  }, 30_000);

  it('should handle mixed read/write storm (readers should not block on writers)', async () => {
    // Seed some data first
    const seedIds: string[] = [];
    for (let i = 0; i < 10; i++) {
      const id = generateUUID();
      seedIds.push(id);
      await client.store<StressEntity>(collection, {
        id,
        personaId: `seed-${i}`,
        content: `Seed data ${i}`,
        category: 'seed',
        priority: i / 10,
        createdAt: new Date().toISOString(),
      });
    }

    // Now fire 10 writes + 20 reads simultaneously
    const start = Date.now();
    const writeErrors: string[] = [];
    const readErrors: string[] = [];
    const readTimes: number[] = [];
    const writeTimes: number[] = [];

    const writeOps = Array.from({ length: 10 }, async (_, i) => {
      const ws = Date.now();
      const id = generateUUID();
      const result = await client.store<StressEntity>(collection, {
        id,
        personaId: `storm-writer-${i}`,
        content: `Storm write ${i}`,
        category: 'storm',
        priority: 0.5,
        createdAt: new Date().toISOString(),
      });
      writeTimes.push(Date.now() - ws);
      if (!result.success) writeErrors.push(`write-${i}: ${result.error}`);
    });

    const readOps = Array.from({ length: 20 }, async (_, i) => {
      const rs = Date.now();
      const result = await client.query<StressEntity>({
        collection,
        filter: { category: 'seed' },
        limit: 5,
      });
      readTimes.push(Date.now() - rs);
      if (!result.success) readErrors.push(`read-${i}: ${result.error}`);
    });

    await Promise.allSettled([...writeOps, ...readOps]);
    const elapsed = Date.now() - start;

    const avgRead = readTimes.reduce((s, t) => s + t, 0) / readTimes.length;
    const avgWrite = writeTimes.reduce((s, t) => s + t, 0) / writeTimes.length;
    const maxRead = Math.max(...readTimes);

    console.log(`\n--- Mixed Read/Write Storm ---`);
    console.log(`10 writes + 20 reads in ${elapsed}ms`);
    console.log(`Avg read: ${avgRead.toFixed(1)}ms, Max read: ${maxRead}ms`);
    console.log(`Avg write: ${avgWrite.toFixed(1)}ms`);
    console.log(`Write errors: ${writeErrors.length}, Read errors: ${readErrors.length}`);

    expect(writeErrors).toHaveLength(0);
    expect(readErrors).toHaveLength(0);

    // WAL mode: readers should NOT block on writers.
    // If reads are taking >5s, something is wrong with the semaphore or pool.
    expect(maxRead).toBeLessThan(5_000);
  }, 30_000);
});
