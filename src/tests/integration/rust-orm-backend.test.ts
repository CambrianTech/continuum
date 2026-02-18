/**
 * Rust ORM Backend Integration Tests
 *
 * Tests the Rust DataModule through ORMRustClient to validate:
 * 1. Basic CRUD operations work
 * 2. Query filters work correctly
 * 3. Data round-trips correctly (camelCase → snake_case → camelCase)
 *
 * FORCE_TYPESCRIPT_BACKEND must be false for these tests to exercise Rust
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { ORMRustClient } from '../../daemons/data-daemon/server/ORMRustClient';
import type { BaseEntity } from '../../system/data/entities/BaseEntity';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

// Test entity type
interface TestEntity extends BaseEntity {
  name: string;
  email: string;
  age: number;
  isActive: boolean;
  createdAt: string;
  metadata?: Record<string, unknown>;
}

describe('Rust ORM Backend', () => {
  let client: ORMRustClient;
  const testCollection = 'test_rust_orm';
  const testIds: string[] = [];

  beforeAll(async () => {
    client = ORMRustClient.getInstance();
    // Give time to connect
    await new Promise(resolve => setTimeout(resolve, 500));
  });

  afterAll(async () => {
    // Clean up test records
    for (const id of testIds) {
      try {
        await client.remove(testCollection, id);
      } catch {
        // Ignore cleanup errors
      }
    }
    client.disconnect();
  });

  it('should connect to continuum-core', async () => {
    // Connection happens lazily, trigger with a simple query
    const result = await client.listCollections();
    console.log('Collections:', result.data);
    expect(result.success).toBe(true);
    expect(Array.isArray(result.data)).toBe(true);
  });

  it('should create a record', async () => {
    const testId = generateUUID();
    testIds.push(testId);

    const entity: TestEntity = {
      id: testId,
      name: 'Test User',
      email: 'test@example.com',
      age: 25,
      isActive: true,
      createdAt: new Date().toISOString(),
      metadata: { source: 'rust-orm-test' }
    };

    const result = await client.store<TestEntity>(testCollection, entity);
    console.log('Store result:', result);

    expect(result.success).toBe(true);
    expect(result.data?.id).toBe(testId);
  });

  it('should read a record by ID', async () => {
    const testId = generateUUID();
    testIds.push(testId);

    // Create first
    const entity: TestEntity = {
      id: testId,
      name: 'Read Test User',
      email: 'read@example.com',
      age: 30,
      isActive: true,
      createdAt: new Date().toISOString()
    };
    await client.store<TestEntity>(testCollection, entity);

    // Now read
    const result = await client.read<TestEntity>(testCollection, testId);
    console.log('Read result:', result);

    expect(result).not.toBeNull();
    expect(result?.id).toBe(testId);
    expect(result?.name).toBe('Read Test User');
    expect(result?.email).toBe('read@example.com');
  });

  it('should query with simple filter', async () => {
    const testId = generateUUID();
    testIds.push(testId);
    const uniqueEmail = `unique-${Date.now()}@example.com`;

    // Create with unique email
    const entity: TestEntity = {
      id: testId,
      name: 'Query Test User',
      email: uniqueEmail,
      age: 35,
      isActive: true,
      createdAt: new Date().toISOString()
    };
    await client.store<TestEntity>(testCollection, entity);

    // Query by email
    const result = await client.query<TestEntity>({
      collection: testCollection,
      filter: { email: uniqueEmail },
      limit: 1
    });

    console.log('Query result:', JSON.stringify(result, null, 2));

    expect(result.success).toBe(true);
    expect(result.data?.length).toBe(1);
    expect(result.data?.[0]?.data?.email).toBe(uniqueEmail);
  });

  it('should query with $eq operator', async () => {
    const testId = generateUUID();
    testIds.push(testId);
    const uniqueName = `OpTest-${Date.now()}`;

    const entity: TestEntity = {
      id: testId,
      name: uniqueName,
      email: 'op@example.com',
      age: 40,
      isActive: true,
      createdAt: new Date().toISOString()
    };
    await client.store<TestEntity>(testCollection, entity);

    // Query using $eq operator
    const result = await client.query<TestEntity>({
      collection: testCollection,
      filter: { name: { $eq: uniqueName } },
      limit: 1
    });

    console.log('$eq query result:', JSON.stringify(result, null, 2));

    expect(result.success).toBe(true);
    expect(result.data?.length).toBe(1);
  });

  it('should update a record', async () => {
    const testId = generateUUID();
    testIds.push(testId);

    // Create
    const entity: TestEntity = {
      id: testId,
      name: 'Update Test User',
      email: 'update@example.com',
      age: 25,
      isActive: true,
      createdAt: new Date().toISOString()
    };
    await client.store<TestEntity>(testCollection, entity);

    // Update
    const result = await client.update<TestEntity>(testCollection, testId, {
      name: 'Updated Name',
      age: 26
    });

    console.log('Update result:', result);

    expect(result.id).toBe(testId);

    // Verify update
    const read = await client.read<TestEntity>(testCollection, testId);
    expect(read?.name).toBe('Updated Name');
    expect(read?.age).toBe(26);
  });

  it('should delete a record', async () => {
    const testId = generateUUID();

    // Create
    const entity: TestEntity = {
      id: testId,
      name: 'Delete Test User',
      email: 'delete@example.com',
      age: 50,
      isActive: false,
      createdAt: new Date().toISOString()
    };
    await client.store<TestEntity>(testCollection, entity);

    // Delete
    const result = await client.remove(testCollection, testId);
    console.log('Delete result:', result);

    expect(result.success).toBe(true);
    expect(result.data).toBe(true);

    // Verify deletion
    const read = await client.read<TestEntity>(testCollection, testId);
    expect(read).toBeNull();
  });

  it('should count records', async () => {
    const testId1 = generateUUID();
    const testId2 = generateUUID();
    testIds.push(testId1, testId2);
    const countTag = `count-${Date.now()}`;

    // Create 2 records with same tag
    await client.store<TestEntity>(testCollection, {
      id: testId1,
      name: countTag,
      email: 'count1@example.com',
      age: 1,
      isActive: true,
      createdAt: new Date().toISOString()
    });
    await client.store<TestEntity>(testCollection, {
      id: testId2,
      name: countTag,
      email: 'count2@example.com',
      age: 2,
      isActive: true,
      createdAt: new Date().toISOString()
    });

    // Count with filter
    const result = await client.count({
      collection: testCollection,
      filter: { name: countTag }
    });

    console.log('Count result:', result);

    expect(result.success).toBe(true);
    expect(result.data).toBe(2);
  });

  it('should handle camelCase to snake_case conversion', async () => {
    const testId = generateUUID();
    testIds.push(testId);

    // Entity with camelCase fields
    const entity: TestEntity = {
      id: testId,
      name: 'CamelCase Test',
      email: 'camel@example.com',
      age: 99,
      isActive: true,
      createdAt: new Date().toISOString(),
    };
    await client.store<TestEntity>(testCollection, entity);

    // Read should return camelCase
    const result = await client.read<TestEntity>(testCollection, testId);
    console.log('CamelCase read result:', result);

    expect(result).not.toBeNull();
    expect(result?.isActive).toBe(true);
    expect(result?.createdAt).toBeDefined();
  });
});
