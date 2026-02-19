/**
 * UserDirectoryManager Tests
 *
 * TDD approach: Test the infrastructure FIRST before using it
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { UserDirectoryManager } from '../../../system/user/directory/server/UserDirectoryManager';
import * as fs from 'fs';
import * as path from 'path';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

describe('UserDirectoryManager', () => {
  const testBaseDir = '.continuum/test-users';
  const testUserId = 'test-user-123' as UUID;
  let manager: UserDirectoryManager;

  beforeEach(() => {
    // Clean slate for each test
    if (fs.existsSync(testBaseDir)) {
      fs.rmSync(testBaseDir, { recursive: true, force: true });
    }
    manager = new UserDirectoryManager(testBaseDir);
  });

  afterEach(() => {
    // Cleanup after each test
    if (fs.existsSync(testBaseDir)) {
      fs.rmSync(testBaseDir, { recursive: true, force: true });
    }
  });

  describe('getPaths()', () => {
    it('should return correct paths for a user', () => {
      const paths = manager.getPaths(testUserId);

      expect(paths.root).toBe(path.join(testBaseDir, testUserId));
      expect(paths.data).toBe(path.join(testBaseDir, testUserId, 'data'));
      expect(paths.logs).toBe(path.join(testBaseDir, testUserId, 'logs'));
      expect(paths.config).toBe(path.join(testBaseDir, testUserId, 'config'));
      expect(paths.database).toBe(path.join(testBaseDir, testUserId, 'data', 'longterm.db'));
      expect(paths.activityLog).toBe(path.join(testBaseDir, testUserId, 'logs', 'activity.log'));
      expect(paths.preferences).toBe(path.join(testBaseDir, testUserId, 'config', 'preferences.json'));
    });
  });

  describe('ensureDirectories()', () => {
    it('should create all required directories', async () => {
      await manager.ensureDirectories(testUserId);

      const paths = manager.getPaths(testUserId);

      expect(fs.existsSync(paths.root)).toBe(true);
      expect(fs.existsSync(paths.data)).toBe(true);
      expect(fs.existsSync(paths.logs)).toBe(true);
      expect(fs.existsSync(paths.config)).toBe(true);
    });

    it('should not fail if directories already exist', async () => {
      await manager.ensureDirectories(testUserId);
      await manager.ensureDirectories(testUserId); // Call twice

      const paths = manager.getPaths(testUserId);
      expect(fs.existsSync(paths.root)).toBe(true);
    });
  });

  describe('exists()', () => {
    it('should return false for non-existent user', async () => {
      const exists = await manager.exists(testUserId);
      expect(exists).toBe(false);
    });

    it('should return true for existing user', async () => {
      await manager.ensureDirectories(testUserId);
      const exists = await manager.exists(testUserId);
      expect(exists).toBe(true);
    });
  });

  describe('getDatabasePath()', () => {
    it('should return correct database path', () => {
      const dbPath = manager.getDatabasePath(testUserId);
      expect(dbPath).toBe(path.join(testBaseDir, testUserId, 'data', 'longterm.db'));
    });
  });

  describe('listUsers()', () => {
    it('should return empty array when no users exist', async () => {
      const users = await manager.listUsers();
      expect(users).toEqual([]);
    });

    it('should list all user directories', async () => {
      const user1 = 'user1' as UUID;
      const user2 = 'user2' as UUID;

      await manager.ensureDirectories(user1);
      await manager.ensureDirectories(user2);

      const users = await manager.listUsers();
      expect(users).toContain(user1);
      expect(users).toContain(user2);
      expect(users.length).toBe(2);
    });
  });

  describe('deleteUserDirectory()', () => {
    it('should delete user directory', async () => {
      await manager.ensureDirectories(testUserId);
      expect(await manager.exists(testUserId)).toBe(true);

      await manager.deleteUserDirectory(testUserId);
      expect(await manager.exists(testUserId)).toBe(false);
    });

    it('should not fail if directory does not exist', async () => {
      await manager.deleteUserDirectory(testUserId);
      // Should not throw
      expect(await manager.exists(testUserId)).toBe(false);
    });
  });

  describe('getUserDiskUsage()', () => {
    it('should return 0 bytes for non-existent database', async () => {
      const usage = await manager.getUserDiskUsage(testUserId);
      expect(usage.bytes).toBe(0);
      expect(usage.humanReadable).toBe('0 B');
    });

    it('should return actual size for existing database', async () => {
      await manager.ensureDirectories(testUserId);

      // Create a test database file
      const dbPath = manager.getDatabasePath(testUserId);
      fs.writeFileSync(dbPath, 'test data');

      const usage = await manager.getUserDiskUsage(testUserId);
      expect(usage.bytes).toBeGreaterThan(0);
      expect(usage.humanReadable).toContain('B');
    });
  });
});
