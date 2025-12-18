/**
 * Rust Worker Storage Adapter - Integration Test
 *
 * Tests the full flow:
 * 1. TypeScript entity with decorators
 * 2. DataDaemon validation
 * 3. RustWorkerStorageAdapter communication
 * 4. Rust worker database I/O
 * 5. Return entity with types intact
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { RustWorkerStorageAdapter } from '../../daemons/data-daemon/server/RustWorkerStorageAdapter';
import { DataDaemon } from '../../daemons/data-daemon/shared/DataDaemon';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

/**
 * Test entity - simple user with decorators from BaseEntity
 */
interface TestUser {
  id: UUID;
  name: string;
  email: string;
  role: 'human' | 'ai';
  createdAt: string;
  updatedAt: string;
  version: number;
}

describe('RustWorkerStorageAdapter Integration', () => {
  let adapter: RustWorkerStorageAdapter;
  let daemon: DataDaemon;

  beforeAll(async () => {
    // Initialize adapter with connection to Rust worker
    adapter = new RustWorkerStorageAdapter({
      socketPath: '/tmp/data-worker.sock',
      dbHandle: 'default',
      timeout: 5000
    });

    await adapter.initialize({
      type: 'rust-worker',
      namespace: 'test',
      options: {}
    });

    // Create DataDaemon using Rust adapter
    daemon = new DataDaemon(
      {
        strategy: 'sql',
        backend: 'rust-worker',
        namespace: 'test',
        options: {}
      },
      adapter
    );

    await daemon.initialize();
  });

  afterAll(async () => {
    await adapter.close();
  });

  it('should create entity via Rust worker', async () => {
    const testUser: TestUser = {
      id: generateUUID(),
      name: 'Test User',
      email: 'test@example.com',
      role: 'human',
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      version: 1
    };

    const context = {
      sessionId: generateUUID(),
      timestamp: new Date().toISOString(),
      source: 'test'
    };

    // Create via DataDaemon (which uses Rust adapter)
    const created = await daemon.create('users', testUser as any, context);

    expect(created).toBeDefined();
    expect(created.id).toBe(testUser.id);
    expect(created.name).toBe('Test User');
    expect(created.email).toBe('test@example.com');
    expect(created.role).toBe('human');
  });

  it('should read entity via Rust worker', async () => {
    // First create a user
    const userId = generateUUID();
    const testUser: TestUser = {
      id: userId,
      name: 'Alice',
      email: 'alice@example.com',
      role: 'ai',
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      version: 1
    };

    const context = {
      sessionId: generateUUID(),
      timestamp: new Date().toISOString(),
      source: 'test'
    };

    await daemon.create('users', testUser as any, context);

    // Read back
    const result = await daemon.read('users', userId, context);

    expect(result.success).toBe(true);
    expect(result.data).toBeDefined();
    expect(result.data!.data.name).toBe('Alice');
    expect(result.data!.data.email).toBe('alice@example.com');
    expect(result.data!.data.role).toBe('ai');
  });

  it('should query entities via Rust worker', async () => {
    // Create multiple users
    const context = {
      sessionId: generateUUID(),
      timestamp: new Date().toISOString(),
      source: 'test'
    };

    const users = [
      { id: generateUUID(), name: 'Bob', email: 'bob@example.com', role: 'human' as const },
      { id: generateUUID(), name: 'Charlie', email: 'charlie@example.com', role: 'ai' as const },
      { id: generateUUID(), name: 'Diana', email: 'diana@example.com', role: 'ai' as const }
    ];

    for (const user of users) {
      await daemon.create('users', {
        ...user,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        version: 1
      } as any, context);
    }

    // Query all AI users
    const result = await daemon.query({
      collection: 'users',
      filter: { role: 'ai' },
      limit: 10
    }, context);

    expect(result.success).toBe(true);
    expect(result.data).toBeDefined();
    expect(result.data!.length).toBeGreaterThanOrEqual(2); // At least Charlie and Diana

    const aiUsers = result.data!.filter(r => r.data.role === 'ai');
    expect(aiUsers.length).toBeGreaterThanOrEqual(2);
  });

  it('should update entity via Rust worker', async () => {
    // Create user
    const userId = generateUUID();
    const testUser: TestUser = {
      id: userId,
      name: 'Eve',
      email: 'eve@example.com',
      role: 'human',
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      version: 1
    };

    const context = {
      sessionId: generateUUID(),
      timestamp: new Date().toISOString(),
      source: 'test'
    };

    await daemon.create('users', testUser as any, context);

    // Update
    const updated = await daemon.update('users', userId, { email: 'eve.updated@example.com' }, context);

    expect(updated).toBeDefined();
    expect(updated.email).toBe('eve.updated@example.com');
    expect(updated.name).toBe('Eve'); // Other fields unchanged
  });

  it('should handle errors gracefully', async () => {
    const context = {
      sessionId: generateUUID(),
      timestamp: new Date().toISOString(),
      source: 'test'
    };

    // Try to read non-existent user
    const result = await daemon.read('users', generateUUID(), context);

    expect(result.success).toBe(false);
    expect(result.error).toBeDefined();
  });
});
